#include "hcime/hc_core_ffi.h"

#include <fcitx/action.h>
#include <fcitx/addonfactory.h>
#include <fcitx/addonmanager.h>
#include <fcitx-config/configuration.h>
#include <fcitx-config/enum.h>
#include <fcitx-config/iniparser.h>
#include <fcitx-config/option.h>
#include <fcitx/inputcontext.h>
#include <fcitx/inputmethodengine.h>
#include <fcitx/inputmethodentry.h>
#include <fcitx/inputpanel.h>
#include <fcitx/instance.h>
#include <fcitx/menu.h>
#include <fcitx/text.h>
#include <fcitx/statusarea.h>
#include <fcitx/userinterface.h>
#include <fcitx/userinterfacemanager.h>
#include <fcitx-utils/utf8.h>
#include <fcitx-utils/key.h>
#include <fcitx-utils/standardpaths.h>

#include <array>
#include <cctype>
#include <cstdlib>
#include <memory>
#include <string>
#include <unordered_map>
#include <vector>

namespace hcime {

using namespace fcitx;

namespace {

struct SessionHandle {
    void* ptr = nullptr;
    ~SessionHandle() {
        if (ptr != nullptr) {
            hc_session_free(ptr);
        }
    }
};

struct IcuuidHash {
    size_t operator()(const ICUUID& uuid) const noexcept {
        size_t value = 0;
        for (auto byte : uuid) {
            value = value * 131u + byte;
        }
        return value;
    }
};

struct ContextState {
    SessionHandle session;
    bool hasActivePreedit = false;
};

struct StateHandle {
    HC_State* state = nullptr;

    explicit StateHandle(HC_State* value) : state(value) {}

    ~StateHandle() {
        if (state != nullptr) {
            hc_state_free(state);
        }
    }

    StateHandle(const StateHandle&) = delete;
    StateHandle& operator=(const StateHandle&) = delete;
};

static constexpr int32_t kInputModeTelex = 0;
static constexpr const char* kConfigPath = "conf/hcime.conf";

enum class HcImeMenuItem {
    LegacyTone,
    SpellCheck,
    AutoRestore,
    DisplayUnderline,
};

FCITX_CONFIGURATION(
    HcImeConfig,
    Option<bool> legacyTone{this, "LegacyTone", "Use legacy tone placement", false};
    Option<bool> spellCheck{this, "SpellCheck", "Validate Vietnamese words with dictionaries and rules", true};
    Option<bool> autoRestore{this, "AutoRestore", "Restore invalid Vietnamese sequences to raw keystrokes", true};
    Option<bool> displayUnderline{this, "DisplayUnderline", "Underline the preedit text", false};
    Option<std::string> vietnameseDictionaryPath{
        this,
        "VietnameseDictionaryPath",
        "Vietnamese dictionary path",
        "/usr/share/fcitx5/bamboo/vietnamese.cm.dict"};
    Option<std::string> englishDictionaryPath{this, "EnglishDictionaryPath", "English dictionary path", ""};)

static void SetEnvIfNotEmpty(const char* name, const std::string& value) {
    if (!value.empty()) {
        setenv(name, value.c_str(), 1);
    }
}

static bool HasCommandModifier(const Key& key) {
    const auto states = key.states();
    return states.test(KeyState::Ctrl) || states.test(KeyState::Alt) || states.test(KeyState::Super) ||
           states.test(KeyState::Super2) || states.test(KeyState::Hyper) || states.test(KeyState::Hyper2) ||
           states.test(KeyState::Meta);
}

static bool IsControlUtf8(const std::string& utf8) {
    if (utf8.size() != 1) {
        return false;
    }
    const auto ch = static_cast<unsigned char>(utf8.front());
    return ch < 0x20 || ch == 0x7F;
}

static bool IsPrintable(const Key& key, std::string& utf8) {
    if (HasCommandModifier(key) || key.isCursorMove() || key.isModifier()) {
        return false;
    }
    utf8 = Key::keySymToUTF8(key.sym());
    return !utf8.empty() && utf8.size() <= 4 && !IsControlUtf8(utf8);
}

static bool IsBoundaryChar(char ch) {
    switch (ch) {
        case ' ':
        case '.':
        case ',':
        case ';':
        case ':':
        case '!':
        case '?':
        case ')':
        case ']':
        case '}':
        case '/':
        case '\\':
        case '-':
        case '_':
        case '"':
        case '\'':
            return true;
        default:
            return false;
    }
}

static void AppendUtf8(std::string& result, uint32_t cp) {
    if (cp <= 0x7F) {
        result.push_back(static_cast<char>(cp));
    } else if (cp <= 0x7FF) {
        result.push_back(static_cast<char>(0xC0 | ((cp >> 6) & 0x1F)));
        result.push_back(static_cast<char>(0x80 | (cp & 0x3F)));
    } else if (cp <= 0xFFFF) {
        result.push_back(static_cast<char>(0xE0 | ((cp >> 12) & 0x0F)));
        result.push_back(static_cast<char>(0x80 | ((cp >> 6) & 0x3F)));
        result.push_back(static_cast<char>(0x80 | (cp & 0x3F)));
    } else {
        result.push_back(static_cast<char>(0xF0 | ((cp >> 18) & 0x07)));
        result.push_back(static_cast<char>(0x80 | ((cp >> 12) & 0x3F)));
        result.push_back(static_cast<char>(0x80 | ((cp >> 6) & 0x3F)));
        result.push_back(static_cast<char>(0x80 | (cp & 0x3F)));
    }
}

static std::string StateToUtf8(const HC_State& state) {
    if (state.composition_string == nullptr || state.length == 0) {
        return {};
    }
    std::string result;
    result.reserve(state.length * 3);
    const auto* data = state.composition_string;
    for (size_t i = 0; i < state.length; ++i) {
        uint32_t cp = data[i];
        if (cp >= 0xD800 && cp <= 0xDBFF) {
            if (i + 1 < state.length) {
                const uint32_t low = data[i + 1];
                if (low >= 0xDC00 && low <= 0xDFFF) {
                    cp = 0x10000 + ((cp - 0xD800) << 10) + (low - 0xDC00);
                    ++i;
                } else {
                    cp = 0xFFFD;
                }
            } else {
                cp = 0xFFFD;
            }
        } else if (cp >= 0xDC00 && cp <= 0xDFFF) {
            cp = 0xFFFD;
        }
        AppendUtf8(result, cp);
    }
    return result;
}

}  // namespace

class HcImeEngine final : public InputMethodEngineV2 {
public:
    explicit HcImeEngine(AddonManager* manager) : instance_(manager != nullptr ? manager->instance() : nullptr) {
        buildStatusMenu();
        registerStatusActions();
        reloadConfig();
    }

    ~HcImeEngine() override {
        unregisterStatusActions();
    }

    std::vector<InputMethodEntry> listInputMethods() override {
        std::vector<InputMethodEntry> entries;
        entries.emplace_back("hcime", "HC_IME", "vi", "hcime")
            .setNativeName("HC_IME")
            .setLabel("HC")
            .setIcon("input-keyboard")
            .setConfigurable(true);
        return entries;
    }

    const Configuration* getConfig() const override {
        return &config_;
    }

    void setConfig(const RawConfig& config) override {
        config_.load(config, true);
        applyRuntimeConfig();
        refreshStatusMenu();
        save();
        resetAllSessions();
    }

    void reloadConfig() override {
        readAsIni(config_, StandardPathsType::Config, kConfigPath);
        applyRuntimeConfig();
        refreshStatusMenu();
    }

    void save() override {
        safeSaveAsIni(config_, StandardPathsType::Config, kConfigPath);
    }

    void keyEvent(const InputMethodEntry& entry, KeyEvent& event) override {
        auto& state = stateFor(event.inputContext());

        if (event.isRelease()) {
            return;
        }

        const int32_t mode = kInputModeTelex;

        if (isUndoKey(event.key()) && state.hasActivePreedit) {
            HC_KeyRequest undoRequest{
                HC_KEY_UNDO,
                nullptr,
                mode,
                static_cast<uint8_t>(*config_.legacyTone),
                static_cast<uint8_t>(*config_.spellCheck),
                static_cast<uint8_t>(*config_.autoRestore),
            };
            HC_KeyResult undoResult = hc_session_handle_key(state.session.ptr, &undoRequest);
            StateHandle undoState{&undoResult.state};
            const std::string undoOutput = StateToUtf8(undoResult.state);
            if (undoResult.handled != 0) {
                if (undoOutput.empty()) {
                    state.hasActivePreedit = false;
                    clearPreedit(event.inputContext());
                } else {
                    setPreedit(event.inputContext(), undoOutput, *config_.displayUnderline, undoResult.state.spell_check_status);
                }
                event.filterAndAccept();
            }
            return;
        }

        if (isDeleteKey(event.key()) && state.hasActivePreedit) {
            clearActivePreedit(event, state);
            event.filterAndAccept();
            return;
        }

        if (isEditingPassthroughKey(event.key())) {
            if (state.hasActivePreedit) {
                commitAndForwardKey(event, state, mode);
            } else {
                resetAndForwardKey(event, state);
            }
            return;
        }

        if (isBackspaceKey(event.key()) && (!state.hasActivePreedit || HasCommandModifier(event.key()))) {
            if (state.hasActivePreedit && HasCommandModifier(event.key())) {
                commitAndForwardKey(event, state, mode);
            } else {
                resetAndForwardKey(event, state);
            }
            return;
        }

        const std::string input = requestText(event.key());

        if (input.empty() && HasCommandModifier(event.key()) && !event.key().isModifier()) {
            if (state.hasActivePreedit) {
                commitActivePreedit(event, state, mode);
            }
            return;
        }

        if (input.empty() && !isSpecialForwardingKey(event.key()) && !event.key().isModifier()) {
            if (state.hasActivePreedit) {
                commitActivePreedit(event, state, mode);
            }
            return;
        }

        const HC_KeyRequest request{
            classify(event.key(), input),
            input.empty() ? nullptr : input.c_str(),
            mode,
            static_cast<uint8_t>(*config_.legacyTone),
            static_cast<uint8_t>(*config_.spellCheck),
            static_cast<uint8_t>(*config_.autoRestore),
        };

        if (state.session.ptr == nullptr) {
            state.session.ptr = hc_session_new(mode, 0);
        }

        HC_KeyResult result = hc_session_handle_key(state.session.ptr, &request);
        StateHandle resultState{&result.state};
        const std::string output = StateToUtf8(result.state);

        if (result.handled == 0) {
            return;
        }

        if (result.state.error_code < 0) {
            event.filterAndAccept();
            clearPreedit(event.inputContext());
            return;
        }

        switch (result.state.status_flag) {
            case HC_STATUS_IN_PROGRESS:
            case HC_STATUS_RECONVERSION_ACTIVE:
                state.hasActivePreedit = !output.empty();
                if (output.empty()) {
                    clearPreedit(event.inputContext());
                } else {
                    setPreedit(event.inputContext(), output, *config_.displayUnderline, result.state.spell_check_status);
                }
                event.filterAndAccept();
                return;
            case HC_STATUS_COMMIT:
            case HC_STATUS_ENGLISH_FALLBACK:
                event.inputContext()->commitString(output);
                state.hasActivePreedit = false;
                clearPreedit(event.inputContext());
                if (request.kind == HC_KEY_SPACE || request.kind == HC_KEY_ENTER || request.kind == HC_KEY_BOUNDARY) {
                    event.inputContext()->forwardKey(event.rawKey(), event.isRelease(), event.time());
                }
                event.filterAndAccept();
                return;
            default:
                event.filterAndAccept();
                clearPreedit(event.inputContext());
                return;
        }
    }

    void activate(const InputMethodEntry& entry, InputContextEvent& event) override {
        auto& state = stateFor(event.inputContext());
        if (state.session.ptr == nullptr) {
            state.session.ptr = hc_session_new(kInputModeTelex, 0);
        }
        attachStatusMenu(event.inputContext());
    }

    void deactivate(const InputMethodEntry&, InputContextEvent& event) override {
        auto& state = stateFor(event.inputContext());
        if (state.session.ptr != nullptr) {
            hc_session_reset(state.session.ptr);
        }
        state.hasActivePreedit = false;
        clearPreedit(event.inputContext());
        event.inputContext()->statusArea().clearGroup(StatusGroup::InputMethod);
        event.inputContext()->updateUserInterface(UserInterfaceComponent::StatusArea, true);
    }

    void reset(const InputMethodEntry&, InputContextEvent& event) override {
        auto& state = stateFor(event.inputContext());
        if (state.session.ptr != nullptr) {
            hc_session_reset(state.session.ptr);
        }
        state.hasActivePreedit = false;
        clearPreedit(event.inputContext());
    }

    std::string subMode(const InputMethodEntry&, InputContext&) override {
        return {};
    }

private:
    ContextState& stateFor(InputContext* ic) {
        return contexts_[ic->uuid()];
    }

    void resetAllSessions() {
        for (auto& [_, state] : contexts_) {
            if (state.session.ptr != nullptr) {
                hc_session_reset(state.session.ptr);
            }
            state.hasActivePreedit = false;
        }
    }

    void clearActivePreedit(KeyEvent& event, ContextState& state) {
        if (state.session.ptr != nullptr) {
            hc_session_reset(state.session.ptr);
        }
        state.hasActivePreedit = false;
        clearPreedit(event.inputContext());
    }

    void deleteActivePreeditCharacter(KeyEvent& event, ContextState& state, int32_t mode) {
        if (state.session.ptr == nullptr) {
            state.hasActivePreedit = false;
            clearPreedit(event.inputContext());
            event.filterAndAccept();
            return;
        }

        HC_KeyRequest deleteRequest{
            HC_KEY_BACKSPACE,
            nullptr,
            mode,
            static_cast<uint8_t>(*config_.legacyTone),
            static_cast<uint8_t>(*config_.spellCheck),
            static_cast<uint8_t>(*config_.autoRestore),
        };
        HC_KeyResult deleteResult = hc_session_handle_key(state.session.ptr, &deleteRequest);
        StateHandle deleteState{&deleteResult.state};
        const std::string output = StateToUtf8(deleteResult.state);

        if (deleteResult.handled == 0) {
            return;
        }

        if (deleteResult.state.error_code < 0 || output.empty()) {
            state.hasActivePreedit = false;
            clearPreedit(event.inputContext());
        } else {
            state.hasActivePreedit = true;
            setPreedit(event.inputContext(), output, *config_.displayUnderline, deleteResult.state.spell_check_status);
        }
        event.filterAndAccept();
    }

    void commitActivePreedit(KeyEvent& event, ContextState& state, int32_t mode) {
        if (!state.hasActivePreedit || state.session.ptr == nullptr) {
            return;
        }
        HC_KeyRequest commitRequest{
            HC_KEY_ENTER,
            nullptr,
            mode,
            static_cast<uint8_t>(*config_.legacyTone),
            static_cast<uint8_t>(*config_.spellCheck),
            static_cast<uint8_t>(*config_.autoRestore),
        };
        HC_KeyResult commitResult = hc_session_handle_key(state.session.ptr, &commitRequest);
        StateHandle commitState{&commitResult.state};
        const std::string committedText = StateToUtf8(commitResult.state);
        if (!committedText.empty()) {
            event.inputContext()->commitString(committedText);
        }
        state.hasActivePreedit = false;
        clearPreedit(event.inputContext());
    }

    void resetAndForwardKey(KeyEvent& event, ContextState& state) {
        clearActivePreedit(event, state);
        event.inputContext()->forwardKey(event.rawKey(), event.isRelease(), event.time());
        event.filterAndAccept();
    }

    void commitAndForwardKey(KeyEvent& event, ContextState& state, int32_t mode) {
        commitActivePreedit(event, state, mode);
        event.inputContext()->forwardKey(event.rawKey(), event.isRelease(), event.time());
        event.filterAndAccept();
    }

    void applyRuntimeConfig() {
        SetEnvIfNotEmpty("HC_IME_VI_DICT", *config_.vietnameseDictionaryPath);
        SetEnvIfNotEmpty("HC_IME_EN_DICT", *config_.englishDictionaryPath);
    }

    void buildStatusMenu() {
        statusMenu_ = std::make_unique<Menu>();
        statusRootAction_ = std::make_unique<SimpleAction>();
        statusRootAction_->setShortText("HC_IME");
        statusRootAction_->setLongText("HC_IME");
        statusRootAction_->setIcon("input-keyboard");
        statusRootAction_->setMenu(statusMenu_.get());

        auto addToggleAction = [this](const std::string& text, HcImeMenuItem item, const std::string& tooltip) {
            auto action = std::make_unique<SimpleAction>();
            action->setShortText(text);
            action->setLongText(tooltip);
            action->setCheckable(true);
            actionConnections_.push_back(action->connect<SimpleAction::Activated>(
                [this, item](InputContext* ic) { onMenuActivated(item, ic); }));
            statusMenu_->addAction(action.get());
            return action;
        };

        toggleActions_[0] = addToggleAction("Legacy tone placement", HcImeMenuItem::LegacyTone,
                                            "Toggle legacy tone placement");
        toggleActions_[1] = addToggleAction("Spell check", HcImeMenuItem::SpellCheck,
                                            "Toggle Vietnamese word validation");
        toggleActions_[2] = addToggleAction("Auto restore", HcImeMenuItem::AutoRestore,
                                            "Toggle raw-keystroke restore for invalid sequences");
        toggleActions_[3] = addToggleAction("Underline preedit", HcImeMenuItem::DisplayUnderline,
                                            "Toggle underline styling for the preedit");

        refreshStatusMenu();
    }

    void registerStatusAction(const std::string& name, Action* action) {
        if (instance_ == nullptr || action == nullptr) {
            return;
        }
        if (instance_->userInterfaceManager().registerAction(name, action)) {
            registeredActions_.push_back(action);
        }
    }

    void registerStatusActions() {
        registerStatusAction("hcime-status", statusRootAction_.get());
        registerStatusAction("hcime-toggle-legacy-tone", toggleActions_[0].get());
        registerStatusAction("hcime-toggle-spell-check", toggleActions_[1].get());
        registerStatusAction("hcime-toggle-auto-restore", toggleActions_[2].get());
        registerStatusAction("hcime-toggle-preedit-underline", toggleActions_[3].get());
    }

    void unregisterStatusActions() {
        if (instance_ == nullptr) {
            registeredActions_.clear();
            return;
        }
        for (auto* action : registeredActions_) {
            instance_->userInterfaceManager().unregisterAction(action);
        }
        registeredActions_.clear();
    }

    void refreshStatusMenu() {
        toggleActions_[0]->setChecked(*config_.legacyTone);
        toggleActions_[1]->setChecked(*config_.spellCheck);
        toggleActions_[2]->setChecked(*config_.autoRestore);
        toggleActions_[3]->setChecked(*config_.displayUnderline);
    }

    void attachStatusMenu(InputContext* ic) {
        auto& statusArea = ic->statusArea();
        statusArea.clearGroup(StatusGroup::InputMethod);
        for (const auto& action : toggleActions_) {
            statusArea.addAction(StatusGroup::InputMethod, action.get());
        }
        ic->updateUserInterface(UserInterfaceComponent::StatusArea, true);
    }

    void onMenuActivated(HcImeMenuItem item, InputContext* ic) {
        switch (item) {
            case HcImeMenuItem::LegacyTone:
                *config_.legacyTone.mutableValue() = !*config_.legacyTone;
                break;
            case HcImeMenuItem::SpellCheck:
                *config_.spellCheck.mutableValue() = !*config_.spellCheck;
                break;
            case HcImeMenuItem::AutoRestore:
                *config_.autoRestore.mutableValue() = !*config_.autoRestore;
                break;
            case HcImeMenuItem::DisplayUnderline:
                *config_.displayUnderline.mutableValue() = !*config_.displayUnderline;
                break;
        }

        applyRuntimeConfig();
        refreshStatusMenu();
        save();
        if (ic != nullptr) {
            ic->reset();
            ic->updateUserInterface(UserInterfaceComponent::StatusArea, true);
            ic->updateUserInterface(UserInterfaceComponent::InputPanel, true);
        }
    }

    static void setPreedit(InputContext* ic, const std::string& text, bool underline, int32_t spell_check_status) {
        TextFormatFlag flags = TextFormatFlag::NoFlag;
        
        // Set flags based on priority: spell check status takes precedence
        if (spell_check_status == HC_SPELL_CHECK_INVALID) {
            flags = TextFormatFlag::HighLight;  // Highlight invalid words
        } else if (spell_check_status == HC_SPELL_CHECK_ENGLISH_FALLBACK) {
            flags = TextFormatFlag::Strike;  // Strike-through for English fallback
        } else if (underline) {
            flags = TextFormatFlag::Underline;  // Normal underline
        }
        
        Text preedit(text, flags);
        preedit.setCursor(static_cast<int>(text.size()));
        ic->inputPanel().setClientPreedit(preedit);
        ic->inputPanel().setPreedit(preedit);
        ic->updatePreedit();
    }

    static void clearPreedit(InputContext* ic) {
        ic->inputPanel().setClientPreedit(Text());
        ic->inputPanel().setPreedit(Text());
        ic->updatePreedit();
    }

    static bool isEditingPassthroughKey(const Key& key) {
        return key.isCursorMove() || isDeleteKey(key) || key.check(FcitxKey_Home) || key.check(FcitxKey_KP_Home) ||
               key.check(FcitxKey_End) || key.check(FcitxKey_KP_End) || key.check(FcitxKey_Begin) ||
               key.check(FcitxKey_KP_Begin) || key.check(FcitxKey_Prior) || key.check(FcitxKey_KP_Prior) ||
               key.check(FcitxKey_Next) || key.check(FcitxKey_KP_Next) || key.check(FcitxKey_Insert) ||
               key.check(FcitxKey_KP_Insert) || key.check(FcitxKey_Tab) || key.check(FcitxKey_KP_Tab) ||
               key.check(FcitxKey_ISO_Left_Tab) || key.check(FcitxKey_KP_BackTab);
    }

    static bool isBackspaceKey(const Key& key) {
        return key.check(FcitxKey_BackSpace) || key.check(FcitxKey_osfBackSpace);
    }

    static bool isDeleteKey(const Key& key) {
        return key.check(FcitxKey_Delete) || key.check(FcitxKey_KP_Delete) || key.check(FcitxKey_osfDelete) ||
               key.check(FcitxKey_DeleteChar) || key.check(FcitxKey_hpDeleteChar);
    }

    static bool isUndoKey(const Key& key) {
        return key.check(FcitxKey_Z, KeyState::Ctrl) || key.check(FcitxKey_z, KeyState::Ctrl);
    }

    static int32_t classify(const Key& key, const std::string& input) {
        if (isBackspaceKey(key)) {
            return HC_KEY_BACKSPACE;
        }
        if (key.check(FcitxKey_Return) || key.check(FcitxKey_KP_Enter)) {
            return HC_KEY_ENTER;
        }
        if (key.check(FcitxKey_Escape)) {
            return HC_KEY_ESCAPE;
        }
        if (input == " ") {
            return HC_KEY_SPACE;
        }
        if (!input.empty() && IsBoundaryChar(input.front()) && input.size() == 1) {
            return HC_KEY_BOUNDARY;
        }
        if (!input.empty()) {
            return HC_KEY_PRINTABLE;
        }
        return HC_KEY_OTHER;
    }

    static bool isSpecialForwardingKey(const Key& key) {
        return isBackspaceKey(key) || key.check(FcitxKey_Return) || key.check(FcitxKey_KP_Enter) ||
               key.check(FcitxKey_Escape);
    }

    static std::string requestText(const Key& key) {
        std::string utf8;
        if (!IsPrintable(key, utf8)) {
            return {};
        }
        return utf8;
    }

    Instance* instance_ = nullptr;
    std::unordered_map<ICUUID, ContextState, IcuuidHash> contexts_;
    HcImeConfig config_;
    std::unique_ptr<Menu> statusMenu_;
    std::unique_ptr<SimpleAction> statusRootAction_;
    std::array<std::unique_ptr<SimpleAction>, 4> toggleActions_;
    std::vector<Connection> actionConnections_;
    std::vector<Action*> registeredActions_;
};

class HcImeFactory final : public AddonFactory {
public:
    AddonInstance* create(AddonManager* manager) override {
        return new HcImeEngine(manager);
    }
};

}  // namespace hcime

FCITX_ADDON_FACTORY(hcime::HcImeFactory)
