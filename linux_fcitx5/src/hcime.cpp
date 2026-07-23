#include "hcime/hc_core_ffi.h"

#include <fcitx/action.h>
#include <fcitx/addonfactory.h>
#include <fcitx/addonmanager.h>
#include <fcitx/candidatelist.h>
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
#include <fcitx-utils/log.h>

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
    bool hanNomCandidatePhase = false;
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
    HanNomTelex,
    HanNomVni,
    HanNomViqr,
};

FCITX_CONFIG_ENUM_NAME(HcImeInputMode, "Telex", "VNI", "VIQR", "HanNomTelex", "HanNomVni", "HanNomViqr");

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
    ModeHanNomTelex,
    ModeHanNomVni,
    ModeHanNomViqr,
    SpellCheck,
    AutoRestore,
    DisplayUnderline,
    QuickConsonants,
    PhrasePrediction,
    LearnPhraseRanking,
    ResetHanNomLearning,
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
    Option<bool> escRestoreRaw{this, "EscRestoreRaw", "ESC key restores raw keystrokes", false};
    Option<bool> phrasePrediction{this, "PhrasePrediction", "Predict common two-word Hán Nôm phrases", true};
    Option<bool> learnPhraseRanking{this, "LearnPhraseRanking", "Learn selected Hán Nôm phrase ranking locally", true};)

FCITX_CONFIGURATION(
    HcImeDictionaryConfig,
    Option<std::string> vietnameseDictionaryPath{
        this, "VietnameseDictionaryPath", "Vietnamese dictionary path",
        "/usr/share/fcitx5/bamboo/vietnamese.cm.dict"};
    Option<std::string> englishDictionaryPath{this, "EnglishDictionaryPath", "English dictionary path", ""};
    Option<std::string> hanNomPhraseDictionaryPath{
        this, "HanNomPhraseDictionaryPath", "Optional Hán Nôm phrase TSV (reading<TAB>glyphs)", ""};)

FCITX_CONFIGURATION(
    HcImePerAppConfig,
    Option<std::vector<std::string>> excludedApps{
        this, "ExcludedApps", "Apps forced to English mode (comma-separated executable names)", std::vector<std::string>()};
    Option<std::vector<std::string>> forcedVnApps{
        this, "ForcedVnApps", "Apps forced to Vietnamese mode (comma-separated executable names)", std::vector<std::string>()};
    Option<bool> smartSwitch{this, "SmartSwitch", "Remember Vietnamese/English mode per app", false};
    Option<std::vector<std::string>> surroundingTextApps{
        this, "SurroundingTextApps", "Apps forced to surrounding-text output mode (comma-separated executable names)", std::vector<std::string>()};
    Option<std::vector<std::string>> preeditApps{
        this, "PreeditApps", "Apps forced to preedit output mode (comma-separated executable names)", std::vector<std::string>()};)

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
        case HcImeInputMode::HanNomTelex: return 3;
        case HcImeInputMode::HanNomVni: return 4;
        case HcImeInputMode::HanNomViqr: return 5;
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
        case HcImeInputMode::HanNomTelex: return "Hán Nôm (Telex)";
        case HcImeInputMode::HanNomVni: return "Hán Nôm (VNI)";
        case HcImeInputMode::HanNomViqr: return "Hán Nôm (VIQR)";
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

class HcImeEngine;

class HcNomCandidateWord : public CandidateWord {
public:
    HcNomCandidateWord(Text text, Text comment, int index, HcImeEngine* engine)
        : CandidateWord(std::move(text)), index_(index), engine_(engine) {
        if (!comment.empty()) {
            setComment(std::move(comment));
        }
    }

    void select(InputContext* ic) const override;

private:
    int index_;
    HcImeEngine* engine_;
};

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

    void selectHanNomCandidate(InputContext* ic, int candidateIndex) {
        if (!ic) return;
        auto& state = stateFor(ic);
        int32_t mode = toSessionInputMode(*config_.input->inputMode);
        if (state.session.ptr == nullptr || mode < 3 || mode > 5) return;

        const bool useSurroundingText = shouldUseSurroundingText(ic, state);
        if (!ensureHanNomCandidatePhase(ic, state, mode, useSurroundingText)) return;

        HC_HanNomResultV3 nomResult;
        std::memset(&nomResult, 0, sizeof(nomResult));
        if (hc_session_select_hannom_candidate_v3(state.session.ptr, static_cast<uint16_t>(candidateIndex), &nomResult) != 0) {
            updateHanNomUi(ic, state, nomResult, useSurroundingText);
        }
    }

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
            bool hadLegacyKeys = (rawConfig.valueByPath("InputMethod") != nullptr &&
                                  rawConfig.valueByPath("Input/InputMethod") == nullptr);
            migrateLegacyConfig(rawConfig);
            config_.load(rawConfig, true);
            if (hadLegacyKeys) {
                // Re-save in the new sectioned format so legacy keys are not re-parsed
                safeSaveAsIni(config_, StandardPathsType::Config, kConfigPath);
            }
        }
        applyRuntimeConfig();
        refreshStatusMenu();
        FCITX_INFO() << "HC_IME: active input mode = " << modeLabel(*config_.input->inputMode)
                     << ", spellCheck=" << *config_.behavior->spellCheck
                     << ", autoRestore=" << *config_.behavior->autoRestore;
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

        const std::string input = requestText(event.key());
        auto request = makeKeyRequest(classify(event.key(), input),
                                       input.empty() ? nullptr : input.c_str(), mode);

        if (state.session.ptr == nullptr) {
            state.session.ptr = hc_session_new(mode, 0);
            loadMacrosIntoSession(state.session.ptr, *config_.macroFilePath);
        }

        if (mode >= 3 && mode <= 5 && state.hasActivePreedit) {
            auto* ic = event.inputContext();
            auto candListPtr = ic->inputPanel().candidateList();
            auto* candidateList = candListPtr ? dynamic_cast<CommonCandidateList*>(candListPtr.get()) : nullptr;

            if (event.key().check(FcitxKey_Down) || event.key().check(FcitxKey_KP_Down) ||
                event.key().check(FcitxKey_Right) || event.key().check(FcitxKey_KP_Right) ||
                event.key().check(FcitxKey_Tab)) {
                if (candidateList != nullptr && candidateList->size() > 0) {
                    candidateList->nextCandidate();
                    ic->updateUserInterface(UserInterfaceComponent::InputPanel, true);
                    event.filterAndAccept();
                    return;
                }
            } else if (event.key().check(FcitxKey_Up) || event.key().check(FcitxKey_KP_Up) ||
                       event.key().check(FcitxKey_Left) || event.key().check(FcitxKey_KP_Left) ||
                       event.key().check(FcitxKey_ISO_Left_Tab)) {
                if (candidateList != nullptr && candidateList->size() > 0) {
                    candidateList->prevCandidate();
                    ic->updateUserInterface(UserInterfaceComponent::InputPanel, true);
                    event.filterAndAccept();
                    return;
                }
            } else if (event.key().check(FcitxKey_Page_Down) || event.key().check(FcitxKey_KP_Page_Down) ||
                       event.key().check(FcitxKey_Next) || event.key().check(FcitxKey_KP_Next)) {
                if (candidateList != nullptr && candidateList->hasNext()) {
                    candidateList->next();
                    ic->updateUserInterface(UserInterfaceComponent::InputPanel, true);
                    event.filterAndAccept();
                    return;
                }
            } else if (event.key().check(FcitxKey_Page_Up) || event.key().check(FcitxKey_KP_Page_Up) ||
                       event.key().check(FcitxKey_Prior) || event.key().check(FcitxKey_KP_Prior)) {
                if (candidateList != nullptr && candidateList->hasPrev()) {
                    candidateList->prev();
                    ic->updateUserInterface(UserInterfaceComponent::InputPanel, true);
                    event.filterAndAccept();
                    return;
                }
            } else if (event.key().check(FcitxKey_Return) || event.key().check(FcitxKey_KP_Enter)) {
                if (candidateList != nullptr && candidateList->cursorIndex() >= 0 && candidateList->cursorIndex() < candidateList->size()) {
                    candidateList->candidate(candidateList->cursorIndex()).select(ic);
                    event.filterAndAccept();
                    return;
                }
                if (candidateList != nullptr && candidateList->size() > 0 && commitHanNomReading(ic, state, mode, useSurroundingText)) {
                    event.filterAndAccept();
                    return;
                }
            // Hán Nôm VNI digits always belong to Vietnamese composition, even
            // while a candidate is focused. Candidate selection stays on the
            // navigation keys plus Enter so tone/shape digits reach the core.
            } else if (candidateList != nullptr && input.size() == 1 && input[0] >= '1' && input[0] <= '9' &&
                       mode != 4) {
                const int index = input[0] - '1';
                if (index < candidateList->size()) {
                    candidateList->candidate(index).select(ic);
                    event.filterAndAccept();
                    return;
                }
            } else if (candidateList != nullptr && (input == "=" || input == "]" || input == "+")) {
                if (candidateList->hasNext()) candidateList->next();
                ic->updateUserInterface(UserInterfaceComponent::InputPanel, true);
                event.filterAndAccept();
                return;
            } else if (candidateList != nullptr && (input == "-" || input == "[")) {
                if (candidateList->hasPrev()) candidateList->prev();
                ic->updateUserInterface(UserInterfaceComponent::InputPanel, true);
                event.filterAndAccept();
                return;
            }
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

        if (input.empty() && HasCommandModifier(event.key()) && !event.key().isModifier()) {
            if (state.hasActivePreedit) commitActivePreedit(event, state, mode);
            return;
        }

        if (input.empty() && !isSpecialForwardingKey(event.key()) && !event.key().isModifier()) {
            if (state.hasActivePreedit) commitActivePreedit(event, state, mode);
            return;
        }

        if (mode >= 3 && mode <= 5) {
            HC_HanNomResultV3 nomResult;
            std::memset(&nomResult, 0, sizeof(nomResult));
            int32_t handled = hc_session_handle_key_hannom_v3(state.session.ptr, &request, &nomResult);
            if (handled == 0) return;

            updateHanNomPhase(state, request, nomResult);
            updateHanNomUi(event.inputContext(), state, nomResult, useSurroundingText);
            event.filterAndAccept();
            return;
        }

        const Utf8KeyResult result = handleKeyUtf8(state.session.ptr, &request);
        const std::string& output = result.text;

        if (result.handled == 0) return;

        if (result.errorCode < 0) {
            event.filterAndAccept();
            state.hasActivePreedit = false;
            state.hanNomCandidatePhase = false;
            state.lastCommitTrailingChars = 0;
            state.previousSurroundingText.clear();
            state.surroundingTextEnabled = false;
            clearPreedit(event.inputContext());
            return;
        }

        if (result.statusFlag == HC_STATUS_ESC_RESTORED_RAW) {
            clearPreedit(event.inputContext());
            event.inputContext()->commitString(output);
            state.hasActivePreedit = false;
            state.hanNomCandidatePhase = false;
            state.previousSurroundingText.clear();
            state.surroundingTextEnabled = false;
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
                    clearPreedit(event.inputContext());
                    event.inputContext()->commitString(output);
                }
                updateSmartSwitch(state, appName, result.statusFlag);
                state.hasActivePreedit = false;
                state.hanNomCandidatePhase = false;
                state.lastCommitTrailingChars = 0;
                state.previousSurroundingText.clear();
                state.surroundingTextEnabled = false;
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
                state.hanNomCandidatePhase = false;
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
            configureHanNomOptions(state.session.ptr);
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
        if (state.session.ptr != nullptr) {
            hc_session_flush_hannom_learning(state.session.ptr);
            hc_session_reset(state.session.ptr);
        }
        state.hasActivePreedit = false;
        state.hanNomCandidatePhase = false;
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
        if (state.session.ptr != nullptr) {
            hc_session_flush_hannom_learning(state.session.ptr);
            hc_session_reset(state.session.ptr);
        }
        state.hasActivePreedit = false;
        state.hanNomCandidatePhase = false;
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
        if (!state.previousSurroundingText.empty()) {
            auto currentSurrounding = ic->surroundingText().text();
            if (!currentSurrounding.empty()) {
                auto len = state.previousSurroundingText.size();
                if (currentSurrounding.size() < len ||
                    currentSurrounding.compare(currentSurrounding.size() - len, len, state.previousSurroundingText) != 0) {
                    state.previousSurroundingText.clear();
                }
            }
        }
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
        auto appName = getAppName(ic);
        if (isAppInList(appName, *config_.perApp->preeditApps)) {
            return false;
        }
        if (isAppInList(appName, *config_.perApp->surroundingTextApps)) {
            return true;
        }
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

    void configureHanNomOptions(void* session) {
        if (session == nullptr) return;
        const auto& phrasePath = *config_.dictionary->hanNomPhraseDictionaryPath;
        HC_HanNomOptionsV2 options{static_cast<uint8_t>(*config_.behavior->phrasePrediction),
                                   static_cast<uint8_t>(*config_.behavior->learnPhraseRanking), nullptr,
                                   phrasePath.empty() ? nullptr : phrasePath.c_str()};
        hc_session_set_hannom_options_v2(session, &options);
    }

    void resetAllSessions() {
        for (auto& [_, state] : contexts_) {
            if (state.session.ptr != nullptr) {
                hc_session_flush_hannom_learning(state.session.ptr);
                hc_session_reset(state.session.ptr);
                configureHanNomOptions(state.session.ptr);
            }
            state.hasActivePreedit = false;
            state.hanNomCandidatePhase = false;
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
        state.hanNomCandidatePhase = false;
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
            clearPreedit(event.inputContext());
            event.inputContext()->commitString(commitResult.text);
        }
        state.hasActivePreedit = false;
        state.hanNomCandidatePhase = false;
        state.lastCommitTrailingChars = 0;
        state.previousSurroundingText.clear();
        state.surroundingTextEnabled = false;
    }

    bool tryReconvertLastCommitFromBackspace(KeyEvent& event, ContextState& state, int32_t mode, bool useSurroundingText) {
        if (state.session.ptr == nullptr || state.lastCommitTrailingChars == 0) return false;
        const bool canDeleteSurrounding = useSurroundingText ||
            (event.inputContext()->capabilityFlags().test(CapabilityFlag::SurroundingText) &&
             (event.inputContext()->surroundingText().isValid() || state.lastCommitTrailingChars > 0));
        if (!canDeleteSurrounding) return false;
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
        state.hanNomCandidatePhase = false;
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

    void updateHanNomPhase(ContextState& state, const HC_KeyRequest&, const HC_HanNomResultV3& nomResult) {
        state.hanNomCandidatePhase = nomResult.status_flag != HC_STATUS_COMMIT &&
            nomResult.error_code >= 0 && nomResult.candidate_count > 0;
    }

    bool ensureHanNomCandidatePhase(InputContext* ic, ContextState& state, int32_t mode, bool useSurroundingText) {
        if (state.hanNomCandidatePhase) return true;

        (void)ic; (void)mode; (void)useSurroundingText;
        return false;
    }

    bool commitHanNomReading(InputContext* ic, ContextState& state, int32_t mode, bool useSurroundingText) {
        if (state.session.ptr == nullptr) return false;
        if (!ensureHanNomCandidatePhase(ic, state, mode, useSurroundingText)) return false;

        HC_KeyRequest enterRequest = makeKeyRequest(HC_KEY_ENTER, nullptr, mode);
        HC_HanNomResultV3 nomResult;
        std::memset(&nomResult, 0, sizeof(nomResult));
        if (hc_session_handle_key_hannom_v3(state.session.ptr, &enterRequest, &nomResult) == 0) {
            return false;
        }

        updateHanNomPhase(state, enterRequest, nomResult);
        updateHanNomUi(ic, state, nomResult, useSurroundingText);
        return true;
    }

    void updateHanNomUi(InputContext* ic, ContextState& state, const HC_HanNomResultV3& nomResult, bool useSurroundingText) {
        if (nomResult.status_flag == HC_STATUS_COMMIT) {
            std::string output(reinterpret_cast<const char*>(nomResult.reading), nomResult.reading_len);
            if (useSurroundingText) {
                commitViaSurroundingText(ic, state, output);
            } else {
                clearPreedit(ic);
                ic->commitString(output);
            }
            state.hasActivePreedit = false;
            state.hanNomCandidatePhase = false;
            state.lastCommitTrailingChars = 0;
            state.previousSurroundingText.clear();
            state.surroundingTextEnabled = false;
            ic->inputPanel().setCandidateList(nullptr);
            ic->updateUserInterface(UserInterfaceComponent::InputPanel, true);
            return;
        }

        if (nomResult.status_flag == HC_STATUS_IN_PROGRESS) {
            std::string output(reinterpret_cast<const char*>(nomResult.reading), nomResult.reading_len);
            state.lastCommitTrailingChars = 0;
            state.hasActivePreedit = !output.empty();
            if (nomResult.candidate_count > 0 && nomResult.candidates != nullptr) {
                auto candidateList = std::make_unique<CommonCandidateList>();
                candidateList->setLabels({"1.", "2.", "3.", "4.", "5.", "6.", "7.", "8.", "9."});
                candidateList->setPageSize(9);
                candidateList->setLayoutHint(CandidateLayoutHint::Vertical);

                for (uint16_t i = 0; i < nomResult.candidate_count; ++i) {
                    std::string candStr(reinterpret_cast<const char*>(nomResult.candidates[i].text), nomResult.candidates[i].text_len);
                    Text wordText(candStr, TextFormatFlag::Bold);
                    candidateList->append<HcNomCandidateWord>(wordText, Text(), i, this);
                }
                ic->inputPanel().setCandidateList(std::move(candidateList));
            } else {
                ic->inputPanel().setCandidateList(nullptr);
            }

            if (useSurroundingText && state.hasActivePreedit) {
                applySurroundingTextPreedit(ic, state, output);
            } else {
                setPreedit(ic, output, *config_.behavior->displayUnderline, 0);
            }
            ic->updateUserInterface(UserInterfaceComponent::InputPanel, true);
        }
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
        modeActions_[3] = addToggleAction("HN-TELEX", HcImeMenuItem::ModeHanNomTelex, "Switch to Hán Nôm (Telex)");
        modeActions_[4] = addToggleAction("HN-VNI", HcImeMenuItem::ModeHanNomVni, "Switch to Hán Nôm (VNI)");
        modeActions_[5] = addToggleAction("HN-VIQR", HcImeMenuItem::ModeHanNomViqr, "Switch to Hán Nôm (VIQR)");
        separatorAction_ = addSeparatorAction();
        toggleActions_[0] = addToggleAction("Spell check", HcImeMenuItem::SpellCheck, "Toggle Vietnamese word validation");
        toggleActions_[1] = addToggleAction("Auto restore", HcImeMenuItem::AutoRestore, "Toggle raw-keystroke restore");
        toggleActions_[2] = addToggleAction("Underline", HcImeMenuItem::DisplayUnderline, "Toggle preedit underline");
        toggleActions_[3] = addToggleAction("Quick consonants", HcImeMenuItem::QuickConsonants, "Toggle quick consonant expansion");
        toggleActions_[4] = addToggleAction("Phrase prediction", HcImeMenuItem::PhrasePrediction, "Toggle Hán Nôm phrase predictions");
        toggleActions_[5] = addToggleAction("Learn phrase ranking", HcImeMenuItem::LearnPhraseRanking, "Toggle local Hán Nôm phrase learning");
        resetLearningAction_ = addToggleAction("Reset Hán Nôm learning", HcImeMenuItem::ResetHanNomLearning, "Clear only local Hán Nôm phrase ranking");
        resetLearningAction_->setCheckable(false);
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
        registerStatusAction("hcime-mode-hanteles", modeActions_[3].get());
        registerStatusAction("hcime-mode-hanvni", modeActions_[4].get());
        registerStatusAction("hcime-mode-hanviqr", modeActions_[5].get());
        registerStatusAction("hcime-mode-separator", separatorAction_.get());
        registerStatusAction("hcime-toggle-spell-check", toggleActions_[0].get());
        registerStatusAction("hcime-toggle-auto-restore", toggleActions_[1].get());
        registerStatusAction("hcime-toggle-preedit-underline", toggleActions_[2].get());
        registerStatusAction("hcime-toggle-quick-consonants", toggleActions_[3].get());
        registerStatusAction("hcime-toggle-hannom-phrase-prediction", toggleActions_[4].get());
        registerStatusAction("hcime-toggle-hannom-learning", toggleActions_[5].get());
        registerStatusAction("hcime-reset-hannom-learning", resetLearningAction_.get());
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
        modeActions_[3]->setChecked(*config_.input->inputMode == HcImeInputMode::HanNomTelex);
        modeActions_[4]->setChecked(*config_.input->inputMode == HcImeInputMode::HanNomVni);
        modeActions_[5]->setChecked(*config_.input->inputMode == HcImeInputMode::HanNomViqr);
        toggleActions_[0]->setChecked(*config_.behavior->spellCheck);
        toggleActions_[1]->setChecked(*config_.behavior->autoRestore);
        toggleActions_[2]->setChecked(*config_.behavior->displayUnderline);
        toggleActions_[3]->setChecked(*config_.behavior->quickConsonants);
        toggleActions_[4]->setChecked(*config_.behavior->phrasePrediction);
        toggleActions_[5]->setChecked(*config_.behavior->learnPhraseRanking);
    }

    void attachStatusMenu(InputContext* ic) {
        auto& statusArea = ic->statusArea();
        statusArea.clearGroup(StatusGroup::InputMethod);
        statusArea.addAction(StatusGroup::InputMethod, modeActions_[1].get());
        statusArea.addAction(StatusGroup::InputMethod, modeActions_[0].get());
        statusArea.addAction(StatusGroup::InputMethod, modeActions_[2].get());
        statusArea.addAction(StatusGroup::InputMethod, modeActions_[3].get());
        statusArea.addAction(StatusGroup::InputMethod, modeActions_[4].get());
        statusArea.addAction(StatusGroup::InputMethod, modeActions_[5].get());
        statusArea.addAction(StatusGroup::InputMethod, separatorAction_.get());
        for (const auto& action : toggleActions_) statusArea.addAction(StatusGroup::InputMethod, action.get());
        statusArea.addAction(StatusGroup::InputMethod, resetLearningAction_.get());
        ic->updateUserInterface(UserInterfaceComponent::StatusArea, true);
    }

    void onMenuActivated(HcImeMenuItem item, InputContext* ic) {
        auto* inputConfig = config_.input.mutableValue();
        auto* behaviorConfig = config_.behavior.mutableValue();
        switch (item) {
            case HcImeMenuItem::ModeTelex: *inputConfig->inputMode.mutableValue() = HcImeInputMode::Telex; break;
            case HcImeMenuItem::ModeVni: *inputConfig->inputMode.mutableValue() = HcImeInputMode::Vni; break;
            case HcImeMenuItem::ModeViqr: *inputConfig->inputMode.mutableValue() = HcImeInputMode::Viqr; break;
            case HcImeMenuItem::ModeHanNomTelex: *inputConfig->inputMode.mutableValue() = HcImeInputMode::HanNomTelex; break;
            case HcImeMenuItem::ModeHanNomVni: *inputConfig->inputMode.mutableValue() = HcImeInputMode::HanNomVni; break;
            case HcImeMenuItem::ModeHanNomViqr: *inputConfig->inputMode.mutableValue() = HcImeInputMode::HanNomViqr; break;
            case HcImeMenuItem::SpellCheck: *behaviorConfig->spellCheck.mutableValue() = !*behaviorConfig->spellCheck; break;
            case HcImeMenuItem::AutoRestore: *behaviorConfig->autoRestore.mutableValue() = !*behaviorConfig->autoRestore; break;
            case HcImeMenuItem::DisplayUnderline: *behaviorConfig->displayUnderline.mutableValue() = !*behaviorConfig->displayUnderline; break;
            case HcImeMenuItem::QuickConsonants: *behaviorConfig->quickConsonants.mutableValue() = !*behaviorConfig->quickConsonants; break;
            case HcImeMenuItem::PhrasePrediction: *behaviorConfig->phrasePrediction.mutableValue() = !*behaviorConfig->phrasePrediction; break;
            case HcImeMenuItem::LearnPhraseRanking: *behaviorConfig->learnPhraseRanking.mutableValue() = !*behaviorConfig->learnPhraseRanking; break;
            case HcImeMenuItem::ResetHanNomLearning:
                for (auto& [_, state] : contexts_) if (state.session.ptr != nullptr) hc_session_reset_hannom_learning(state.session.ptr);
                break;
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
    std::array<std::unique_ptr<SimpleAction>, 6> modeActions_;
    std::unique_ptr<SimpleAction> separatorAction_;
    std::array<std::unique_ptr<SimpleAction>, 6> toggleActions_;
    std::unique_ptr<SimpleAction> resetLearningAction_;
    std::vector<Connection> actionConnections_;
    std::vector<Action*> registeredActions_;
};

inline void HcNomCandidateWord::select(InputContext* ic) const {
    if (engine_ && ic) {
        engine_->selectHanNomCandidate(ic, index_);
    }
}

class HcImeFactory final : public AddonFactory {
public:
    AddonInstance* create(AddonManager* manager) override {
        return new HcImeEngine(manager);
    }
};

}  // namespace hcime

FCITX_ADDON_FACTORY(hcime::HcImeFactory)
