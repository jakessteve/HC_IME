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
#include <fcitx-utils/capabilityflags.h>
#include <fcitx/statusarea.h>
#include <fcitx/userinterface.h>
#include <fcitx/userinterfacemanager.h>
#include <fcitx-utils/utf8.h>
#include <fcitx-utils/key.h>
#include <fcitx-utils/standardpaths.h>

#include <algorithm>
#include <array>
#include <cctype>
#include <cstdlib>
#include <fstream>
#include <functional>
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

enum class PerAppMode {
    Global,
    ForceEnglish,
    ForceVietnamese,
};

enum class SmartSwitchState {
    Unknown,
    Vietnamese,
    English,
};

struct ContextState {
    SessionHandle session;
    bool hasActivePreedit = false;
    unsigned int lastCommitTrailingChars = 0;
    bool surroundingTextEnabled = false;
    PerAppMode perAppMode = PerAppMode::Global;
    SmartSwitchState smartSwitchState = SmartSwitchState::Unknown;
    std::string previousSurroundingText;
};

enum class HcImeInputMode {
    Telex,
    Vni,
    Viqr,
};

FCITX_CONFIG_ENUM_NAME(HcImeInputMode, "Telex", "VNI", "VIQR");

enum class HcImeEnglishProtection {
    Off,
    Soft,
    Hard,
};

FCITX_CONFIG_ENUM_NAME(HcImeEnglishProtection, "Off", "Soft", "Hard");

enum class HcImeOutputMode {
    Preedit,
    SurroundingText,
};

FCITX_CONFIG_ENUM_NAME(HcImeOutputMode, "Preedit", "SurroundingText");

static constexpr int32_t kInputModeTelex = 0;
static constexpr const char* kConfigPath = "conf/hcime.conf";

enum class HcImeMenuItem {
    ModeTelex,
    ModeVni,
    ModeViqr,
    SpellCheck,
    AutoRestore,
    DisplayUnderline,
    QuickConsonants,
};

FCITX_CONFIGURATION(
    HcImeInputConfig,
    Option<HcImeInputMode> inputMode{this, "InputMethod", "Input mode", HcImeInputMode::Telex};
    Option<bool> legacyTone{this, "LegacyTone", "Use legacy tone placement", false};)

FCITX_CONFIGURATION(
    HcImeBehaviorConfig,
    Option<bool> spellCheck{this, "SpellCheck", "Validate Vietnamese words with dictionaries and rules", true};
    Option<bool> autoRestore{this, "AutoRestore", "Restore invalid Vietnamese sequences to raw keystrokes", true};
    Option<bool> displayUnderline{this, "DisplayUnderline", "Underline the preedit text", false};
    Option<bool> quickConsonants{this, "QuickConsonants", "Enable quick consonant expansion (cc->ch, nn->ng, f->ph, etc.)", false};
    Option<HcImeEnglishProtection> englishProtection{this, "EnglishProtection", "English protection level", HcImeEnglishProtection::Off};
    Option<bool> macroInEnglish{this, "MacroInEnglish", "Allow macro expansion in English mode", false};
    Option<bool> escRestoreRaw{this, "EscRestoreRaw", "ESC key restores raw keystrokes", false};)

FCITX_CONFIGURATION(
    HcImeDictionaryConfig,
    Option<std::string> vietnameseDictionaryPath{
        this, "VietnameseDictionaryPath", "Vietnamese dictionary path",
        "/usr/share/fcitx5/bamboo/vietnamese.cm.dict"};
    Option<std::string> englishDictionaryPath{this, "EnglishDictionaryPath", "English dictionary path", ""};)

FCITX_CONFIGURATION(
    HcImePerAppConfig,
    Option<std::vector<std::string>> excludedApps{
        this, "ExcludedApps", "Apps forced to English mode (comma-separated executable names)", std::vector<std::string>()};
    Option<std::vector<std::string>> forcedVnApps{
        this, "ForcedVnApps", "Apps forced to Vietnamese mode (comma-separated executable names)", std::vector<std::string>()};
    Option<bool> smartSwitch{this, "SmartSwitch", "Remember Vietnamese/English mode per app", false};)

FCITX_CONFIGURATION(
    HcImeOutputConfig,
    Option<HcImeOutputMode> outputMode{this, "OutputMode", "Output mode", HcImeOutputMode::Preedit};)

FCITX_CONFIGURATION(
    HcImeConfig,
    Option<HcImeInputConfig> input{this, "Input", "Input settings", {}};
    Option<HcImeBehaviorConfig> behavior{this, "Behavior", "Typing behavior", {}};
    Option<HcImeDictionaryConfig> dictionary{this, "Dictionary", "Dictionary paths", {}};
    Option<HcImePerAppConfig> perApp{this, "PerApp", "Per-application settings", {}};
    Option<HcImeOutputConfig> output{this, "Output", "Output settings", {}};
    Option<std::string> macroFilePath{this, "MacroFilePath", "Path to macro definitions file", ""};)

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
    if (utf8.size() != 1) return false;
    const auto ch = static_cast<unsigned char>(utf8.front());
    return ch < 0x20 || ch == 0x7F;
}

static int32_t toSessionInputMode(HcImeInputMode mode) {
    switch (mode) {
        case HcImeInputMode::Telex: return kInputModeTelex;
        case HcImeInputMode::Vni: return 1;
        case HcImeInputMode::Viqr: return 2;
    }
    return kInputModeTelex;
}

static uint8_t toEnglishProtectionLevel(HcImeEnglishProtection level) {
    switch (level) {
        case HcImeEnglishProtection::Off: return 0;
        case HcImeEnglishProtection::Soft: return 1;
        case HcImeEnglishProtection::Hard: return 2;
    }
    return 0;
}

static const char* modeLabel(HcImeInputMode mode) {
    switch (mode) {
        case HcImeInputMode::Telex: return "Telex";
        case HcImeInputMode::Vni: return "VNI";
        case HcImeInputMode::Viqr: return "VIQR";
    }
    return "Telex";
}

static bool IsPrintable(const Key& key, std::string& utf8) {
    if (HasCommandModifier(key) || key.isCursorMove() || key.isModifier()) return false;
    utf8 = Key::keySymToUTF8(key.sym());
    return !utf8.empty() && utf8.size() <= 4 && !IsControlUtf8(utf8);
}

static bool IsBoundaryChar(char ch) {
    switch (ch) {
        case ' ': case '.': case ',': case ';': case ':': case '!': case '?':
        case ')': case ']': case '}': case '/': case '\\': case '-': case '_':
        case '"': case '\'':
            return true;
        default: return false;
    }
}

static void copyLegacyConfigValue(RawConfig& config, const char* oldPath, const char* newPath) {
    if (config.valueByPath(newPath) != nullptr) return;
    if (const auto* value = config.valueByPath(oldPath)) {
        config.setValueByPath(newPath, *value);
    }
}

static void migrateLegacyConfig(RawConfig& config) {
    copyLegacyConfigValue(config, "InputMethod", "Input/InputMethod");
    copyLegacyConfigValue(config, "LegacyTone", "Input/LegacyTone");
    copyLegacyConfigValue(config, "SpellCheck", "Behavior/SpellCheck");
    copyLegacyConfigValue(config, "AutoRestore", "Behavior/AutoRestore");
    copyLegacyConfigValue(config, "DisplayUnderline", "Behavior/DisplayUnderline");
    copyLegacyConfigValue(config, "VietnameseDictionaryPath", "Dictionary/VietnameseDictionaryPath");
    copyLegacyConfigValue(config, "EnglishDictionaryPath", "Dictionary/EnglishDictionaryPath");
}

static void loadMacrosIntoSession(void* session, const std::string& macroFilePath) {
    if (session == nullptr || macroFilePath.empty()) return;
    std::string resolvedPath = macroFilePath;
    if (!resolvedPath.empty() && resolvedPath[0] == '~') {
        const char* home = getenv("HOME");
        if (home != nullptr) resolvedPath = std::string(home) + resolvedPath.substr(1);
    }
    hc_session_clear_macros(session);
    std::ifstream file(resolvedPath);
    if (!file.is_open()) return;
    std::string line;
    while (std::getline(file, line)) {
        if (line.empty() || line[0] == '#') continue;
        auto eqPos = line.find('=');
        if (eqPos == std::string::npos || eqPos == 0) continue;
        std::string key = line.substr(0, eqPos);
        std::string value = line.substr(eqPos + 1);
        auto trim = [](std::string& s) {
            while (!s.empty() && std::isspace(static_cast<unsigned char>(s.front()))) s.erase(s.begin());
            while (!s.empty() && std::isspace(static_cast<unsigned char>(s.back()))) s.pop_back();
        };
        trim(key);
        trim(value);
        if (!key.empty() && !value.empty()) {
            hc_session_add_macro(session, key.c_str(), value.c_str());
        }
    }
}

static std::string getAppName(InputContext* ic) {
    if (!ic) return {};
    return ic->program();
}

static bool isAppInList(const std::string& appName, const std::vector<std::string>& list) {
    if (appName.empty()) return false;
    auto lowerApp = appName;
    std::transform(lowerApp.begin(), lowerApp.end(), lowerApp.begin(), ::tolower);
    for (const auto& entry : list) {
        auto lowerEntry = entry;
        std::transform(lowerEntry.begin(), lowerEntry.end(), lowerEntry.begin(), ::tolower);
        if (lowerApp.find(lowerEntry) != std::string::npos) return true;
    }
    return false;
}

struct SurroundingTextDelta {
    unsigned int deleteChars = 0;
    std::string insertText;
};

struct Utf8KeyResult {
    std::string text;
    int32_t statusFlag = HC_STATUS_IN_PROGRESS;
    int32_t errorCode = HC_ERROR_NONE;
    int32_t spellCheckStatus = HC_SPELL_CHECK_VALID;
    uint8_t handled = 0;
};

static SurroundingTextDelta computeSurroundingDiff(const std::string& oldText, const std::string& newText) {
    if (!utf8::validate(oldText) || !utf8::validate(newText)) {
        return {static_cast<unsigned int>(utf8::length(oldText)), newText};
    }

    size_t commonPrefixBytes = 0;
    auto oldIt = oldText.begin();
    auto newIt = newText.begin();
    while (oldIt != oldText.end() && newIt != newText.end()) {
        uint32_t oldChar = 0;
        uint32_t newChar = 0;
        auto oldNext = utf8::getNextChar(oldIt, oldText.end(), &oldChar);
        auto newNext = utf8::getNextChar(newIt, newText.end(), &newChar);
        if (oldChar != newChar) {
            break;
        }
        commonPrefixBytes += static_cast<size_t>(std::distance(oldIt, oldNext));
        oldIt = oldNext;
        newIt = newNext;
    }
    auto commonPrefixChars = static_cast<unsigned int>(utf8::length(oldText, 0, commonPrefixBytes));
    auto deleteChars = static_cast<unsigned int>(utf8::length(oldText) - commonPrefixChars);
    return {deleteChars, newText.substr(commonPrefixBytes)};
}

static Utf8KeyResult handleKeyUtf8(void* session, const HC_KeyRequest* request) {
    auto result = hc_session_handle_key_utf8(session, request);
    Utf8KeyResult output;
    output.statusFlag = result.status_flag;
    output.errorCode = result.error_code;
    output.spellCheckStatus = result.spell_check_status;
    output.handled = result.handled;
    if (result.length > 0 && result.composition_string != nullptr) {
        output.text.assign(result.composition_string, result.length);
    }
    return output;
}

}  // namespace

class HcImeEngine final : public InputMethodEngineV2 {
public:
    explicit HcImeEngine(AddonManager* manager)
        : instance_(manager != nullptr ? manager->instance() : nullptr) {
        buildStatusMenu();
        registerStatusActions();
        reloadConfig();
    }

    ~HcImeEngine() override { unregisterStatusActions(); }

    std::vector<InputMethodEntry> listInputMethods() override {
        std::vector<InputMethodEntry> entries;
        entries.emplace_back("hcime", "HC_IME", "vi", "hcime")
            .setNativeName("HC_IME")
            .setLabel("HC")
            .setIcon("input-keyboard")
            .setConfigurable(true);
        return entries;
    }

    const Configuration* getConfig() const override { return &config_; }

    void setConfig(const RawConfig& config) override {
        RawConfig migratedConfig = config;
        migrateLegacyConfig(migratedConfig);
        config_.load(migratedConfig, true);
        applyRuntimeConfig();
        refreshStatusMenu();
        save();
        resetAllSessions();
    }

    void reloadConfig() override {
        if (instance_ != nullptr) {
            RawConfig rawConfig;
            readAsIni(rawConfig, StandardPathsType::Config, kConfigPath);
            migrateLegacyConfig(rawConfig);
            config_.load(rawConfig, true);
        }
        applyRuntimeConfig();
        refreshStatusMenu();
    }

    void save() override {
        if (instance_ != nullptr) {
            safeSaveAsIni(config_, StandardPathsType::Config, kConfigPath);
        }
    }

    void keyEvent(const InputMethodEntry& entry, KeyEvent& event) override {
        auto& state = stateFor(event.inputContext());
        auto appName = getAppName(event.inputContext());
        resolvePerAppMode(state, appName);

        if (state.perAppMode == PerAppMode::ForceEnglish) {
            return;
        }

        const int32_t mode = toSessionInputMode(*config_.input->inputMode);

        if (event.isRelease()) return;

        const bool useSurroundingText = shouldUseSurroundingText(event.inputContext(), state);

        if (isUndoKey(event.key()) && state.hasActivePreedit) {
            HC_KeyRequest undoRequest{makeKeyRequest(HC_KEY_UNDO, nullptr, mode)};
            const Utf8KeyResult undoResult = handleKeyUtf8(state.session.ptr, &undoRequest);
            if (undoResult.handled != 0) {
                if (undoResult.text.empty()) {
                    if (useSurroundingText) {
                        commitViaSurroundingText(event.inputContext(), state, "");
                        state.surroundingTextEnabled = false;
                    }
                    state.hasActivePreedit = false;
                    clearPreedit(event.inputContext());
                } else {
                    if (useSurroundingText) {
                        applySurroundingTextPreedit(event.inputContext(), state, undoResult.text);
                    } else {
                        setPreedit(event.inputContext(), undoResult.text, *config_.behavior->displayUnderline, undoResult.spellCheckStatus);
                    }
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
            } else if (tryReconvertLastCommitFromBackspace(event, state, mode, useSurroundingText)) {
                return;
            } else {
                resetAndForwardKey(event, state);
            }
            return;
        }

        const std::string input = requestText(event.key());

        if (input.empty() && HasCommandModifier(event.key()) && !event.key().isModifier()) {
            if (state.hasActivePreedit) commitActivePreedit(event, state, mode);
            return;
        }

        if (input.empty() && !isSpecialForwardingKey(event.key()) && !event.key().isModifier()) {
            if (state.hasActivePreedit) commitActivePreedit(event, state, mode);
            return;
        }

        auto request = makeKeyRequest(classify(event.key(), input),
                                       input.empty() ? nullptr : input.c_str(), mode);

        if (state.session.ptr == nullptr) {
            state.session.ptr = hc_session_new(mode, 0);
            loadMacrosIntoSession(state.session.ptr, *config_.macroFilePath);
        }

        const Utf8KeyResult result = handleKeyUtf8(state.session.ptr, &request);
        const std::string& output = result.text;

        if (result.handled == 0) return;

        if (result.errorCode < 0) {
            event.filterAndAccept();
            state.hasActivePreedit = false;
            state.lastCommitTrailingChars = 0;
            state.previousSurroundingText.clear();
            state.surroundingTextEnabled = false;
            clearPreedit(event.inputContext());
            return;
        }

        if (result.statusFlag == HC_STATUS_ESC_RESTORED_RAW) {
            event.inputContext()->commitString(output);
            state.hasActivePreedit = false;
            state.previousSurroundingText.clear();
            state.surroundingTextEnabled = false;
            clearPreedit(event.inputContext());
            event.filterAndAccept();
            return;
        }

        switch (result.statusFlag) {
            case HC_STATUS_IN_PROGRESS:
            case HC_STATUS_RECONVERSION_ACTIVE:
                state.lastCommitTrailingChars = 0;
                state.hasActivePreedit = !output.empty();
                if (output.empty()) {
                    clearPreedit(event.inputContext());
                } else {
                    if (useSurroundingText && state.hasActivePreedit) {
                        applySurroundingTextPreedit(event.inputContext(), state, output);
                    } else {
                        setPreedit(event.inputContext(), output, *config_.behavior->displayUnderline, result.spellCheckStatus);
                    }
                }
                event.filterAndAccept();
                return;
            case HC_STATUS_COMMIT:
            case HC_STATUS_ENGLISH_FALLBACK:
                if (useSurroundingText) {
                    commitViaSurroundingText(event.inputContext(), state, output);
                } else {
                    event.inputContext()->commitString(output);
                }
                updateSmartSwitch(state, appName, result.statusFlag);
                state.hasActivePreedit = false;
                state.lastCommitTrailingChars = 0;
                state.previousSurroundingText.clear();
                state.surroundingTextEnabled = false;
                clearPreedit(event.inputContext());
                if (request.kind == HC_KEY_SPACE || request.kind == HC_KEY_BOUNDARY) {
                    state.lastCommitTrailingChars = output.empty() ? 0 : 1;
                    event.inputContext()->forwardKey(event.rawKey(), event.isRelease(), event.time());
                } else if (request.kind == HC_KEY_ENTER) {
                    event.inputContext()->forwardKey(event.rawKey(), event.isRelease(), event.time());
                }
                event.filterAndAccept();
                return;
            default:
                event.filterAndAccept();
                state.hasActivePreedit = false;
                state.lastCommitTrailingChars = 0;
                state.previousSurroundingText.clear();
                state.surroundingTextEnabled = false;
                clearPreedit(event.inputContext());
                return;
        }
    }

    void activate(const InputMethodEntry& entry, InputContextEvent& event) override {
        auto& state = stateFor(event.inputContext());
        if (state.session.ptr == nullptr) {
            state.session.ptr = hc_session_new(toSessionInputMode(*config_.input->inputMode), 0);
            loadMacrosIntoSession(state.session.ptr, *config_.macroFilePath);
        }
        attachStatusMenu(event.inputContext());
    }

    void deactivate(const InputMethodEntry&, InputContextEvent& event) override {
        auto& state = stateFor(event.inputContext());
        if (state.surroundingTextEnabled && !state.previousSurroundingText.empty()) {
            auto surroundingLen = utf8::length(state.previousSurroundingText);
            if (surroundingLen > 0) {
                event.inputContext()->deleteSurroundingText(-static_cast<int>(surroundingLen), surroundingLen);
            }
        }
        if (state.session.ptr != nullptr) hc_session_reset(state.session.ptr);
        state.hasActivePreedit = false;
        state.lastCommitTrailingChars = 0;
        state.previousSurroundingText.clear();
        state.surroundingTextEnabled = false;
        clearPreedit(event.inputContext());
        event.inputContext()->statusArea().clearGroup(StatusGroup::InputMethod);
        event.inputContext()->updateUserInterface(UserInterfaceComponent::StatusArea, true);
    }

    void reset(const InputMethodEntry&, InputContextEvent& event) override {
        auto& state = stateFor(event.inputContext());
        if (state.surroundingTextEnabled && !state.previousSurroundingText.empty()) {
            auto surroundingLen = utf8::length(state.previousSurroundingText);
            if (surroundingLen > 0) {
                event.inputContext()->deleteSurroundingText(-static_cast<int>(surroundingLen), surroundingLen);
            }
        }
        if (state.session.ptr != nullptr) hc_session_reset(state.session.ptr);
        state.hasActivePreedit = false;
        state.lastCommitTrailingChars = 0;
        state.previousSurroundingText.clear();
        state.surroundingTextEnabled = false;
        clearPreedit(event.inputContext());
    }

    std::string subMode(const InputMethodEntry&, InputContext&) override {
        return modeLabel(*config_.input->inputMode);
    }

private:
    HC_KeyRequest makeKeyRequest(int32_t kind, const char* text, int32_t mode) {
        return HC_KeyRequest{
            kind, text, mode,
            static_cast<uint8_t>(*config_.input->legacyTone),
            static_cast<uint8_t>(*config_.behavior->spellCheck),
            static_cast<uint8_t>(*config_.behavior->autoRestore),
            static_cast<uint8_t>(*config_.behavior->quickConsonants),
            toEnglishProtectionLevel(*config_.behavior->englishProtection),
            static_cast<uint8_t>(*config_.behavior->macroInEnglish),
            static_cast<uint8_t>(*config_.behavior->escRestoreRaw),
        };
    }

    void resolvePerAppMode(ContextState& state, const std::string& appName) {
        state.perAppMode = PerAppMode::Global;
        if (isAppInList(appName, *config_.perApp->excludedApps)) {
            state.perAppMode = PerAppMode::ForceEnglish;
            return;
        }
        if (isAppInList(appName, *config_.perApp->forcedVnApps)) {
            state.perAppMode = PerAppMode::ForceVietnamese;
            return;
        }
        if (*config_.perApp->smartSwitch && state.smartSwitchState != SmartSwitchState::Unknown) {
            // Smart switch influences mode but doesn't override explicit config
        }
    }

    void updateSmartSwitch(ContextState& state, const std::string& appName, int32_t commitStatus) {
        if (!*config_.perApp->smartSwitch || appName.empty()) return;
        if (commitStatus == HC_STATUS_ENGLISH_FALLBACK) {
            state.smartSwitchState = SmartSwitchState::English;
        } else if (commitStatus == HC_STATUS_COMMIT) {
            state.smartSwitchState = SmartSwitchState::Vietnamese;
        }
    }

    void applySurroundingTextPreedit(InputContext* ic, ContextState& state, const std::string& newPreedit) {
        if (state.previousSurroundingText.empty()) {
            ic->commitString(newPreedit);
        } else {
            auto diff = computeSurroundingDiff(state.previousSurroundingText, newPreedit);
            if (diff.deleteChars > 0) {
                ic->deleteSurroundingText(-static_cast<int>(diff.deleteChars), diff.deleteChars);
            }
            if (!diff.insertText.empty()) {
                ic->commitString(diff.insertText);
            }
        }
        state.previousSurroundingText = newPreedit;
    }

    void commitViaSurroundingText(InputContext* ic, ContextState& state, const std::string& committedText) {
        if (!state.previousSurroundingText.empty()) {
            auto surroundingLen = utf8::length(state.previousSurroundingText);
            if (surroundingLen > 0) {
                ic->deleteSurroundingText(-static_cast<int>(surroundingLen), surroundingLen);
            }
        }
        ic->commitString(committedText);
        state.previousSurroundingText.clear();
    }

    bool shouldUseSurroundingText(InputContext* ic, ContextState& state) {
        if (*config_.output->outputMode != HcImeOutputMode::SurroundingText) {
            return false;
        }
        if (!state.surroundingTextEnabled) {
            const auto flags = ic->capabilityFlags();
            state.surroundingTextEnabled =
                flags.test(CapabilityFlag::SurroundingText) && ic->surroundingText().isValid();
        }
        return state.surroundingTextEnabled;
    }

    ContextState& stateFor(InputContext* ic) { return contexts_[ic->uuid()]; }

    void resetAllSessions() {
        for (auto& [_, state] : contexts_) {
            if (state.session.ptr != nullptr) hc_session_reset(state.session.ptr);
            state.hasActivePreedit = false;
            state.lastCommitTrailingChars = 0;
            state.previousSurroundingText.clear();
            state.surroundingTextEnabled = false;
        }
    }

    void clearActivePreedit(KeyEvent& event, ContextState& state) {
        if (state.surroundingTextEnabled && !state.previousSurroundingText.empty()) {
            auto surroundingLen = utf8::length(state.previousSurroundingText);
            if (surroundingLen > 0) {
                event.inputContext()->deleteSurroundingText(-static_cast<int>(surroundingLen), surroundingLen);
            }
        }
        if (state.session.ptr != nullptr) hc_session_reset(state.session.ptr);
        state.hasActivePreedit = false;
        state.lastCommitTrailingChars = 0;
        state.previousSurroundingText.clear();
        state.surroundingTextEnabled = false;
        clearPreedit(event.inputContext());
    }

    void commitActivePreedit(KeyEvent& event, ContextState& state, int32_t mode) {
        if (!state.hasActivePreedit || state.session.ptr == nullptr) return;
        HC_KeyRequest commitRequest{makeKeyRequest(HC_KEY_ENTER, nullptr, mode)};
        const Utf8KeyResult commitResult = handleKeyUtf8(state.session.ptr, &commitRequest);
        if (state.surroundingTextEnabled) {
            commitViaSurroundingText(event.inputContext(), state, commitResult.text);
        } else if (!commitResult.text.empty()) {
            event.inputContext()->commitString(commitResult.text);
        }
        state.hasActivePreedit = false;
        state.lastCommitTrailingChars = 0;
        state.previousSurroundingText.clear();
        state.surroundingTextEnabled = false;
        clearPreedit(event.inputContext());
    }

    bool tryReconvertLastCommitFromBackspace(KeyEvent& event, ContextState& state, int32_t mode, bool useSurroundingText) {
        if (state.session.ptr == nullptr || state.lastCommitTrailingChars == 0) return false;
        HC_KeyRequest request{makeKeyRequest(HC_KEY_BACKSPACE, nullptr, mode)};
        const Utf8KeyResult result = handleKeyUtf8(state.session.ptr, &request);
        if (result.handled == 0 || result.errorCode < 0 ||
            result.statusFlag != HC_STATUS_RECONVERSION_ACTIVE || result.text.empty()) {
            return false;
        }
        const auto committedChars = static_cast<unsigned int>(utf8::length(result.text));
        const auto deleteChars = committedChars + state.lastCommitTrailingChars;
        event.inputContext()->deleteSurroundingText(-static_cast<int>(deleteChars), deleteChars);
        state.lastCommitTrailingChars = 0;
        state.hasActivePreedit = true;
        if (useSurroundingText) {
            applySurroundingTextPreedit(event.inputContext(), state, result.text);
        } else {
            setPreedit(event.inputContext(), result.text, *config_.behavior->displayUnderline, result.spellCheckStatus);
        }
        event.filterAndAccept();
        return true;
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
        SetEnvIfNotEmpty("HC_IME_VI_DICT", *config_.dictionary->vietnameseDictionaryPath);
        SetEnvIfNotEmpty("HC_IME_EN_DICT", *config_.dictionary->englishDictionaryPath);
    }

    void buildStatusMenu() {
        auto addToggleAction = [this](const std::string& text, HcImeMenuItem item, const std::string& tooltip) {
            auto action = std::make_unique<SimpleAction>();
            action->setShortText(text);
            action->setLongText(tooltip);
            action->setCheckable(true);
            actionConnections_.push_back(action->connect<SimpleAction::Activated>(
                [this, item](InputContext* ic) { onMenuActivated(item, ic); }));
            return action;
        };
        auto addSeparatorAction = [this]() {
            auto action = std::make_unique<SimpleAction>();
            action->setSeparator(true);
            return action;
        };

        modeActions_[1] = addToggleAction("VNI", HcImeMenuItem::ModeVni, "Switch to VNI");
        modeActions_[0] = addToggleAction("TELEX", HcImeMenuItem::ModeTelex, "Switch to Telex");
        modeActions_[2] = addToggleAction("VIQR", HcImeMenuItem::ModeViqr, "Switch to VIQR");
        separatorAction_ = addSeparatorAction();
        toggleActions_[0] = addToggleAction("Spell check", HcImeMenuItem::SpellCheck, "Toggle Vietnamese word validation");
        toggleActions_[1] = addToggleAction("Auto restore", HcImeMenuItem::AutoRestore, "Toggle raw-keystroke restore");
        toggleActions_[2] = addToggleAction("Underline", HcImeMenuItem::DisplayUnderline, "Toggle preedit underline");
        toggleActions_[3] = addToggleAction("Quick consonants", HcImeMenuItem::QuickConsonants, "Toggle quick consonant expansion");
        refreshStatusMenu();
    }

    void registerStatusAction(const std::string& name, Action* action) {
        if (instance_ == nullptr || action == nullptr) return;
        if (instance_->userInterfaceManager().registerAction(name, action)) {
            registeredActions_.push_back(action);
        }
    }

    void registerStatusActions() {
        registerStatusAction("hcime-mode-telex", modeActions_[0].get());
        registerStatusAction("hcime-mode-vni", modeActions_[1].get());
        registerStatusAction("hcime-mode-viqr", modeActions_[2].get());
        registerStatusAction("hcime-mode-separator", separatorAction_.get());
        registerStatusAction("hcime-toggle-spell-check", toggleActions_[0].get());
        registerStatusAction("hcime-toggle-auto-restore", toggleActions_[1].get());
        registerStatusAction("hcime-toggle-preedit-underline", toggleActions_[2].get());
        registerStatusAction("hcime-toggle-quick-consonants", toggleActions_[3].get());
    }

    void unregisterStatusActions() {
        if (instance_ == nullptr) { registeredActions_.clear(); return; }
        for (auto* action : registeredActions_) instance_->userInterfaceManager().unregisterAction(action);
        registeredActions_.clear();
    }

    void refreshStatusMenu() {
        modeActions_[0]->setChecked(*config_.input->inputMode == HcImeInputMode::Telex);
        modeActions_[1]->setChecked(*config_.input->inputMode == HcImeInputMode::Vni);
        modeActions_[2]->setChecked(*config_.input->inputMode == HcImeInputMode::Viqr);
        toggleActions_[0]->setChecked(*config_.behavior->spellCheck);
        toggleActions_[1]->setChecked(*config_.behavior->autoRestore);
        toggleActions_[2]->setChecked(*config_.behavior->displayUnderline);
        toggleActions_[3]->setChecked(*config_.behavior->quickConsonants);
    }

    void attachStatusMenu(InputContext* ic) {
        auto& statusArea = ic->statusArea();
        statusArea.clearGroup(StatusGroup::InputMethod);
        statusArea.addAction(StatusGroup::InputMethod, modeActions_[1].get());
        statusArea.addAction(StatusGroup::InputMethod, modeActions_[0].get());
        statusArea.addAction(StatusGroup::InputMethod, modeActions_[2].get());
        statusArea.addAction(StatusGroup::InputMethod, separatorAction_.get());
        for (const auto& action : toggleActions_) statusArea.addAction(StatusGroup::InputMethod, action.get());
        ic->updateUserInterface(UserInterfaceComponent::StatusArea, true);
    }

    void onMenuActivated(HcImeMenuItem item, InputContext* ic) {
        auto* inputConfig = config_.input.mutableValue();
        auto* behaviorConfig = config_.behavior.mutableValue();
        switch (item) {
            case HcImeMenuItem::ModeTelex: *inputConfig->inputMode.mutableValue() = HcImeInputMode::Telex; break;
            case HcImeMenuItem::ModeVni: *inputConfig->inputMode.mutableValue() = HcImeInputMode::Vni; break;
            case HcImeMenuItem::ModeViqr: *inputConfig->inputMode.mutableValue() = HcImeInputMode::Viqr; break;
            case HcImeMenuItem::SpellCheck: *behaviorConfig->spellCheck.mutableValue() = !*behaviorConfig->spellCheck; break;
            case HcImeMenuItem::AutoRestore: *behaviorConfig->autoRestore.mutableValue() = !*behaviorConfig->autoRestore; break;
            case HcImeMenuItem::DisplayUnderline: *behaviorConfig->displayUnderline.mutableValue() = !*behaviorConfig->displayUnderline; break;
            case HcImeMenuItem::QuickConsonants: *behaviorConfig->quickConsonants.mutableValue() = !*behaviorConfig->quickConsonants; break;
        }
        applyRuntimeConfig();
        refreshStatusMenu();
        save();
        resetAllSessions();
        if (ic != nullptr) {
            ic->reset();
            ic->updateUserInterface(UserInterfaceComponent::StatusArea, true);
            ic->updateUserInterface(UserInterfaceComponent::InputPanel, true);
        }
    }

    static void setPreedit(InputContext* ic, const std::string& text, bool underline, int32_t spell_check_status) {
        TextFormatFlag flags = TextFormatFlag::NoFlag;
        if (spell_check_status == HC_SPELL_CHECK_INVALID) {
            flags = TextFormatFlag::HighLight;
        } else if (spell_check_status == HC_SPELL_CHECK_ENGLISH_FALLBACK) {
            flags = TextFormatFlag::Strike;
        } else if (underline) {
            flags = TextFormatFlag::Underline;
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
        if (isBackspaceKey(key)) return HC_KEY_BACKSPACE;
        if (key.check(FcitxKey_Return) || key.check(FcitxKey_KP_Enter)) return HC_KEY_ENTER;
        if (key.check(FcitxKey_Escape)) return HC_KEY_ESCAPE;
        if (input == " ") return HC_KEY_SPACE;
        if (!input.empty() && IsBoundaryChar(input.front()) && input.size() == 1) return HC_KEY_BOUNDARY;
        if (!input.empty()) return HC_KEY_PRINTABLE;
        return HC_KEY_OTHER;
    }

    static bool isSpecialForwardingKey(const Key& key) {
        return isBackspaceKey(key) || key.check(FcitxKey_Return) || key.check(FcitxKey_KP_Enter) ||
               key.check(FcitxKey_Escape);
    }

    static std::string requestText(const Key& key) {
        std::string utf8;
        if (!IsPrintable(key, utf8)) return {};
        return utf8;
    }

    Instance* instance_ = nullptr;
    std::unordered_map<ICUUID, ContextState, IcuuidHash> contexts_;
    HcImeConfig config_;
    std::array<std::unique_ptr<SimpleAction>, 3> modeActions_;
    std::unique_ptr<SimpleAction> separatorAction_;
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
