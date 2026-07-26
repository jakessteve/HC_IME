#include "hcime_key_handler.h"
#include "hcime_candidate_adapter.h"
#include "hcime_status_menu.h"

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
#include <fcitx-utils/key.h>
#include <fcitx-utils/log.h>
#include <fcitx-utils/standardpaths.h>
#include <fcitx-utils/utf8.h>
#include <fcitx/statusarea.h>
#include <fcitx/userinterface.h>
#include <fcitx/userinterfacemanager.h>

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

FCITX_CONFIG_ENUM_NAME(HcImeInputMode, "Telex", "VNI", "VIQR", "HanNomTelex", "HanNomVni", "HanNomViqr");
FCITX_CONFIG_ENUM_NAME(HcImeEnglishProtection, "Off", "Soft", "Hard");
FCITX_CONFIG_ENUM_NAME(HcImeOutputMode, "Preedit", "SurroundingText");

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

namespace {

static void SetEnvIfNotEmpty(const char* name, const std::string& value) {
    if (!value.empty()) {
        setenv(name, value.c_str(), 1);
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

}  // namespace

class HcImeEngine final : public InputMethodEngineV2 {
public:
    explicit HcImeEngine(AddonManager* manager)
        : instance_(manager != nullptr ? manager->instance() : nullptr) {
        statusMenu_.build(instance_, [this](HcImeMenuItem item, InputContext* ic) {
            onMenuActivated(item, ic);
        });
        statusMenu_.registerAll();
        reloadConfig();
    }

    ~HcImeEngine() override { statusMenu_.unregisterAll(); }

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
        if (state.session.ptr == nullptr || !HcImeKeyHandler::isHanNomInputMode(mode)) return;

        const bool useSurroundingText = shouldUseSurroundingText(ic, state);
        if (!ensureHanNomCandidatePhase(ic, state, mode, useSurroundingText)) return;

        HC_HanNomResultV3 nomResult;
        std::memset(&nomResult, 0, sizeof(nomResult));
        if (hc_session_select_hannom_candidate_v3(state.session.ptr, static_cast<uint16_t>(candidateIndex), &nomResult) != 0) {
            auto onSelect = [this](InputContext* ctx, int idx) { selectHanNomCandidate(ctx, idx); };
            HcImeCandidateAdapter::updateHanNomUi(ic, state, nomResult, useSurroundingText,
                                                  *config_.behavior->displayUnderline, onSelect);
        }
    }

    void setConfig(const RawConfig& config) override {
        RawConfig migratedConfig = config;
        migrateLegacyConfig(migratedConfig);
        config_.load(migratedConfig, true);
        applyRuntimeConfig();
        statusMenu_.refresh(*config_.input->inputMode, *config_.behavior->spellCheck,
                            *config_.behavior->autoRestore, *config_.behavior->displayUnderline,
                            *config_.behavior->quickConsonants, *config_.behavior->phrasePrediction,
                            *config_.behavior->learnPhraseRanking);
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
                safeSaveAsIni(config_, StandardPathsType::Config, kConfigPath);
            }
        }
        applyRuntimeConfig();
        statusMenu_.refresh(*config_.input->inputMode, *config_.behavior->spellCheck,
                            *config_.behavior->autoRestore, *config_.behavior->displayUnderline,
                            *config_.behavior->quickConsonants, *config_.behavior->phrasePrediction,
                            *config_.behavior->learnPhraseRanking);
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

        if (HcImeKeyHandler::isUndoKey(event.key()) && state.hasActivePreedit) {
            HC_KeyRequest undoRequest{HcImeKeyHandler::makeKeyRequest(
                HC_KEY_UNDO, nullptr, mode,
                static_cast<uint8_t>(*config_.input->legacyTone),
                static_cast<uint8_t>(*config_.behavior->spellCheck),
                static_cast<uint8_t>(*config_.behavior->autoRestore),
                static_cast<uint8_t>(*config_.behavior->quickConsonants),
                toEnglishProtectionLevel(*config_.behavior->englishProtection),
                static_cast<uint8_t>(*config_.behavior->macroInEnglish),
                static_cast<uint8_t>(*config_.behavior->escRestoreRaw))};
            const Utf8KeyResult undoResult = HcImeKeyHandler::handleKeyUtf8(state.session.ptr, &undoRequest);
            if (undoResult.handled != 0) {
                if (undoResult.text.empty()) {
                    if (useSurroundingText) {
                        HcImeCandidateAdapter::commitViaSurroundingText(event.inputContext(), state, "");
                        state.surroundingTextEnabled = false;
                    }
                    state.hasActivePreedit = false;
                    HcImeCandidateAdapter::clearPreedit(event.inputContext());
                } else {
                    if (useSurroundingText) {
                        HcImeCandidateAdapter::applySurroundingTextPreedit(event.inputContext(), state, undoResult.text);
                    } else {
                        HcImeCandidateAdapter::setPreedit(event.inputContext(), undoResult.text,
                                                          *config_.behavior->displayUnderline, undoResult.spellCheckStatus);
                    }
                }
                event.filterAndAccept();
            }
            return;
        }

        if (HcImeKeyHandler::isDeleteKey(event.key()) && state.hasActivePreedit) {
            clearActivePreedit(event, state);
            event.filterAndAccept();
            return;
        }

        const std::string input = HcImeKeyHandler::requestText(event.key());
        auto requestV2 = HcImeKeyHandler::makeKeyRequestV2(
            HcImeKeyHandler::classify(event.key(), input),
            input.empty() ? nullptr : input.c_str(), mode,
            static_cast<uint8_t>(*config_.input->legacyTone),
            static_cast<uint8_t>(*config_.behavior->spellCheck),
            static_cast<uint8_t>(*config_.behavior->autoRestore),
            static_cast<uint8_t>(*config_.behavior->quickConsonants),
            toEnglishProtectionLevel(*config_.behavior->englishProtection),
            static_cast<uint8_t>(*config_.behavior->macroInEnglish),
            static_cast<uint8_t>(*config_.behavior->escRestoreRaw));

        if (state.session.ptr == nullptr) {
            state.session.ptr = hc_session_new(mode, 0);
            loadMacrosIntoSession(state.session.ptr, *config_.macroFilePath);
        }

        if (HcImeKeyHandler::isHanNomInputMode(mode) && state.hasActivePreedit) {
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
            } else if (candidateList != nullptr && input.size() == 1 && input[0] >= '1' && input[0] <= '9' &&
                       mode != HC_INPUT_HAN_NOM_VNI) {
                const int index = input[0] - '1';
                if (index < candidateList->size()) {
                    candidateList->candidate(index).select(ic);
                    event.filterAndAccept();
                    return;
                }
            }
        }

        if (HcImeKeyHandler::isEditingPassthroughKey(event.key())) {
            if (state.hasActivePreedit) {
                commitAndForwardKey(event, state, mode);
            } else {
                resetAndForwardKey(event, state);
            }
            return;
        }

        if (HcImeKeyHandler::isBackspaceKey(event.key()) && (!state.hasActivePreedit || HcImeKeyHandler::HasCommandModifier(event.key()))) {
            if (state.hasActivePreedit && HcImeKeyHandler::HasCommandModifier(event.key())) {
                commitAndForwardKey(event, state, mode);
            } else if (tryReconvertLastCommitFromBackspace(event, state, mode, useSurroundingText)) {
                return;
            } else {
                resetAndForwardKey(event, state);
            }
            return;
        }

        if (input.empty() && HcImeKeyHandler::HasCommandModifier(event.key()) && !event.key().isModifier()) {
            if (state.hasActivePreedit) commitActivePreedit(event, state, mode);
            return;
        }

        if (input.empty() && !HcImeKeyHandler::isSpecialForwardingKey(event.key()) && !event.key().isModifier()) {
            if (state.hasActivePreedit) commitActivePreedit(event, state, mode);
            return;
        }

        HC_KeyResultV2 keyResult;
        std::memset(&keyResult, 0, sizeof(keyResult));
        int32_t handled = hc_session_handle_key_v4(state.session.ptr, &requestV2, &keyResult);
        if (handled == 0) return;

        if (HcImeKeyHandler::isHanNomInputMode(mode)) {
            HC_HanNomResultV3 nomResult;
            std::memset(&nomResult, 0, sizeof(nomResult));
            nomResult.status_flag = keyResult.status_flag;
            nomResult.error_code = keyResult.error_code;
            nomResult.reading = reinterpret_cast<const uint8_t*>(keyResult.composition_string);
            nomResult.reading_len = static_cast<uint16_t>(keyResult.composition_len);
            nomResult.candidates = keyResult.candidates;
            nomResult.candidate_count = keyResult.candidate_count;
            nomResult.total_candidate_count = keyResult.total_candidate_count;
            nomResult.page_size = 9;
            nomResult.truncated = 0;
            nomResult.handled = keyResult.handled;

            HC_KeyRequest nomRequest = HcImeKeyHandler::makeKeyRequest(
                requestV2.kind, requestV2.text, mode,
                static_cast<uint8_t>(*config_.input->legacyTone),
                static_cast<uint8_t>(*config_.behavior->spellCheck),
                static_cast<uint8_t>(*config_.behavior->autoRestore),
                static_cast<uint8_t>(*config_.behavior->quickConsonants),
                toEnglishProtectionLevel(*config_.behavior->englishProtection),
                static_cast<uint8_t>(*config_.behavior->macroInEnglish),
                static_cast<uint8_t>(*config_.behavior->escRestoreRaw));
            updateHanNomPhase(state, nomResult);
            if (nomResult.candidate_count > 0) {
                state.hasActivePreedit = true;
            }
            auto onSelect = [this](InputContext* ctx, int idx) { selectHanNomCandidate(ctx, idx); };
            HcImeCandidateAdapter::updateHanNomUi(event.inputContext(), state, nomResult,
                                                  useSurroundingText, *config_.behavior->displayUnderline, onSelect);
            event.filterAndAccept();
            return;
        }

        if (keyResult.error_code < 0) {
            event.filterAndAccept();
            state.hasActivePreedit = false;
            state.hanNomCandidatePhase = false;
            state.lastCommitTrailingChars = 0;
            state.previousSurroundingText.clear();
            state.surroundingTextEnabled = false;
            HcImeCandidateAdapter::clearPreedit(event.inputContext());
            return;
        }

        std::string output;
        if (keyResult.composition_string != nullptr && keyResult.composition_len > 0) {
            output.assign(keyResult.composition_string, keyResult.composition_len);
        }

        if (keyResult.status_flag == HC_STATUS_ESC_RESTORED_RAW) {
            HcImeCandidateAdapter::clearPreedit(event.inputContext());
            event.inputContext()->commitString(output);
            state.hasActivePreedit = false;
            state.hanNomCandidatePhase = false;
            state.previousSurroundingText.clear();
            state.surroundingTextEnabled = false;
            event.filterAndAccept();
            return;
        }

        switch (keyResult.status_flag) {
            case HC_STATUS_IN_PROGRESS:
            case HC_STATUS_RECONVERSION_ACTIVE:
                state.lastCommitTrailingChars = 0;
                state.hasActivePreedit = !output.empty();
                if (output.empty()) {
                    HcImeCandidateAdapter::clearPreedit(event.inputContext());
                } else {
                    if (useSurroundingText && state.hasActivePreedit) {
                        HcImeCandidateAdapter::applySurroundingTextPreedit(event.inputContext(), state, output);
                    } else {
                        HcImeCandidateAdapter::setPreedit(event.inputContext(), output,
                                                          *config_.behavior->displayUnderline, keyResult.spell_check_status);
                    }
                }
                event.filterAndAccept();
                return;
            case HC_STATUS_COMMIT:
            case HC_STATUS_ENGLISH_FALLBACK:
                if (useSurroundingText) {
                    HcImeCandidateAdapter::commitViaSurroundingText(event.inputContext(), state, output);
                } else {
                    HcImeCandidateAdapter::clearPreedit(event.inputContext());
                    event.inputContext()->commitString(output);
                }
                updateSmartSwitch(state, appName, keyResult.status_flag);
                state.hasActivePreedit = false;
                state.hanNomCandidatePhase = false;
                state.lastCommitTrailingChars = 0;
                state.previousSurroundingText.clear();
                state.surroundingTextEnabled = false;
                if (requestV2.kind == HC_KEY_SPACE || requestV2.kind == HC_KEY_BOUNDARY) {
                    state.lastCommitTrailingChars = output.empty() ? 0 : 1;
                    event.inputContext()->forwardKey(event.rawKey(), event.isRelease(), event.time());
                } else if (requestV2.kind == HC_KEY_ENTER) {
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
                HcImeCandidateAdapter::clearPreedit(event.inputContext());
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
        statusMenu_.attach(event.inputContext(), *config_.input->inputMode);
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
        HcImeCandidateAdapter::clearPreedit(event.inputContext());
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
        HcImeCandidateAdapter::clearPreedit(event.inputContext());
    }

    std::string subMode(const InputMethodEntry&, InputContext&) override {
        return modeLabel(*config_.input->inputMode);
    }

private:
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

        bool isHanNom = *config_.input->inputMode >= HcImeInputMode::HanNomTelex;
        uint8_t phrasePrediction = isHanNom ? static_cast<uint8_t>(*config_.behavior->phrasePrediction) : 0;
        uint8_t phraseLearning = isHanNom ? static_cast<uint8_t>(*config_.behavior->learnPhraseRanking) : 0;

        HC_HanNomOptionsV2 options{phrasePrediction, phraseLearning, nullptr,
                                   phrasePath.empty() ? nullptr : phrasePath.c_str()};
        hc_session_set_hannom_options_v2(session, &options);
    }

    void resetAllSessions() {
        int32_t currentMode = toSessionInputMode(*config_.input->inputMode);
        for (auto& [_, state] : contexts_) {
            if (state.session.ptr != nullptr) {
                hc_session_flush_hannom_learning(state.session.ptr);
                hc_session_free(state.session.ptr);
                state.session.ptr = hc_session_new(currentMode, 0);
                loadMacrosIntoSession(state.session.ptr, *config_.macroFilePath);
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
        HcImeCandidateAdapter::clearPreedit(event.inputContext());
    }

    void commitActivePreedit(KeyEvent& event, ContextState& state, int32_t mode) {
        if (!state.hasActivePreedit || state.session.ptr == nullptr) return;
        HC_KeyRequest commitRequest{HcImeKeyHandler::makeKeyRequest(
            HC_KEY_ENTER, nullptr, mode,
            static_cast<uint8_t>(*config_.input->legacyTone),
            static_cast<uint8_t>(*config_.behavior->spellCheck),
            static_cast<uint8_t>(*config_.behavior->autoRestore),
            static_cast<uint8_t>(*config_.behavior->quickConsonants),
            toEnglishProtectionLevel(*config_.behavior->englishProtection),
            static_cast<uint8_t>(*config_.behavior->macroInEnglish),
            static_cast<uint8_t>(*config_.behavior->escRestoreRaw))};
        const Utf8KeyResult commitResult = HcImeKeyHandler::handleKeyUtf8(state.session.ptr, &commitRequest);
        if (state.surroundingTextEnabled) {
            HcImeCandidateAdapter::commitViaSurroundingText(event.inputContext(), state, commitResult.text);
        } else if (!commitResult.text.empty()) {
            HcImeCandidateAdapter::clearPreedit(event.inputContext());
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
        HC_KeyRequest request{HcImeKeyHandler::makeKeyRequest(
            HC_KEY_BACKSPACE, nullptr, mode,
            static_cast<uint8_t>(*config_.input->legacyTone),
            static_cast<uint8_t>(*config_.behavior->spellCheck),
            static_cast<uint8_t>(*config_.behavior->autoRestore),
            static_cast<uint8_t>(*config_.behavior->quickConsonants),
            toEnglishProtectionLevel(*config_.behavior->englishProtection),
            static_cast<uint8_t>(*config_.behavior->macroInEnglish),
            static_cast<uint8_t>(*config_.behavior->escRestoreRaw))};
        const Utf8KeyResult result = HcImeKeyHandler::handleKeyUtf8(state.session.ptr, &request);
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
            HcImeCandidateAdapter::applySurroundingTextPreedit(event.inputContext(), state, result.text);
        } else {
            HcImeCandidateAdapter::setPreedit(event.inputContext(), result.text,
                                              *config_.behavior->displayUnderline, result.spellCheckStatus);
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

    void updateHanNomPhase(ContextState& state, const HC_HanNomResultV3& nomResult) {
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

        HC_KeyRequest enterRequest = HcImeKeyHandler::makeKeyRequest(
            HC_KEY_ENTER, nullptr, mode,
            static_cast<uint8_t>(*config_.input->legacyTone),
            static_cast<uint8_t>(*config_.behavior->spellCheck),
            static_cast<uint8_t>(*config_.behavior->autoRestore),
            static_cast<uint8_t>(*config_.behavior->quickConsonants),
            toEnglishProtectionLevel(*config_.behavior->englishProtection),
            static_cast<uint8_t>(*config_.behavior->macroInEnglish),
            static_cast<uint8_t>(*config_.behavior->escRestoreRaw));
        HC_HanNomResultV3 nomResult;
        std::memset(&nomResult, 0, sizeof(nomResult));
        if (hc_session_handle_key_hannom_v3(state.session.ptr, &enterRequest, &nomResult) == 0) {
            return false;
        }

        updateHanNomPhase(state, nomResult);
        auto onSelect = [this](InputContext* ctx, int idx) { selectHanNomCandidate(ctx, idx); };
        HcImeCandidateAdapter::updateHanNomUi(ic, state, nomResult, useSurroundingText,
                                              *config_.behavior->displayUnderline, onSelect);
        return true;
    }

    void applyRuntimeConfig() {
        SetEnvIfNotEmpty("HC_IME_VI_DICT", *config_.dictionary->vietnameseDictionaryPath);
        SetEnvIfNotEmpty("HC_IME_EN_DICT", *config_.dictionary->englishDictionaryPath);
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
        statusMenu_.refresh(*config_.input->inputMode, *config_.behavior->spellCheck,
                            *config_.behavior->autoRestore, *config_.behavior->displayUnderline,
                            *config_.behavior->quickConsonants, *config_.behavior->phrasePrediction,
                            *config_.behavior->learnPhraseRanking);
        save();
        resetAllSessions();
        if (ic != nullptr) {
            ic->reset();
            ic->updateUserInterface(UserInterfaceComponent::StatusArea, true);
        }
    }

    Instance* instance_ = nullptr;
    std::unordered_map<ICUUID, ContextState, IcuuidHash> contexts_;
    HcImeConfig config_;
    HcImeStatusMenu statusMenu_;
};

class HcImeFactory final : public AddonFactory {
public:
    AddonInstance* create(AddonManager* manager) override {
        return new HcImeEngine(manager);
    }
};

}  // namespace hcime

FCITX_ADDON_FACTORY(hcime::HcImeFactory)
