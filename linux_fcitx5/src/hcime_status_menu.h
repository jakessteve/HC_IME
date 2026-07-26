#pragma once

#include "hcime/hc_core_ffi.h"

#include <fcitx/action.h>
#include <fcitx/inputcontext.h>
#include <fcitx/instance.h>
#include <fcitx/statusarea.h>
#include <fcitx/userinterfacemanager.h>

#include <array>
#include <functional>
#include <memory>
#include <string>
#include <vector>

namespace hcime {

enum class HcImeInputMode {
    Telex,
    Vni,
    Viqr,
    HanNomTelex,
    HanNomVni,
    HanNomViqr,
};

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

enum class HcImeEnglishProtection {
    Off,
    Soft,
    Hard,
};

enum class HcImeOutputMode {
    Preedit,
    SurroundingText,
};

static constexpr const char* modeLabel(HcImeInputMode mode) {
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

static constexpr int32_t kInputModeTelex = 0;
static constexpr const char* kConfigPath = "conf/hcime.conf";

static inline int32_t toSessionInputMode(HcImeInputMode mode) {
    switch (mode) {
        case HcImeInputMode::Telex: return kInputModeTelex;
        case HcImeInputMode::Vni: return 1;
        case HcImeInputMode::Viqr: return 2;
        case HcImeInputMode::HanNomTelex: return HC_INPUT_HAN_NOM_TELEX;
        case HcImeInputMode::HanNomVni: return HC_INPUT_HAN_NOM_VNI;
        case HcImeInputMode::HanNomViqr: return HC_INPUT_HAN_NOM_VIQR;
    }
    return kInputModeTelex;
}

static inline uint8_t toEnglishProtectionLevel(HcImeEnglishProtection level) {
    switch (level) {
        case HcImeEnglishProtection::Off: return 0;
        case HcImeEnglishProtection::Soft: return 1;
        case HcImeEnglishProtection::Hard: return 2;
    }
    return 0;
}

class HcImeStatusMenu {
public:
    using MenuCallback = std::function<void(HcImeMenuItem, fcitx::InputContext*)>;

    void build(fcitx::Instance* instance, MenuCallback onActivated);
    void registerAll();
    void unregisterAll();
    void refresh(HcImeInputMode currentMode, bool spellCheck, bool autoRestore,
                 bool underline, bool quickConsonants, bool phrasePrediction, bool learnPhraseRanking);
    void attach(fcitx::InputContext* ic, HcImeInputMode currentMode);

private:
    fcitx::Instance* instance_ = nullptr;
    MenuCallback onActivated_;
    std::array<std::unique_ptr<fcitx::SimpleAction>, 6> modeActions_;
    std::unique_ptr<fcitx::SimpleAction> separatorAction_;
    std::array<std::unique_ptr<fcitx::SimpleAction>, 6> toggleActions_;
    std::unique_ptr<fcitx::SimpleAction> resetLearningAction_;
    std::vector<fcitx::Connection> actionConnections_;
    std::vector<fcitx::Action*> registeredActions_;

    void registerAction(const std::string& name, fcitx::Action* action);
};

}  // namespace hcime
