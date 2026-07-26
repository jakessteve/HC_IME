#include "hcime_status_menu.h"

#include <fcitx/inputcontext.h>
#include <fcitx/statusarea.h>
#include <fcitx/userinterface.h>

namespace hcime {

using namespace fcitx;

void HcImeStatusMenu::build(Instance* instance, MenuCallback onActivated) {
    instance_ = instance;
    onActivated_ = std::move(onActivated);

    auto addToggleAction = [this](const std::string& text, HcImeMenuItem item, const std::string& tooltip) {
        auto action = std::make_unique<SimpleAction>();
        action->setShortText(text);
        action->setLongText(tooltip);
        action->setCheckable(true);
        actionConnections_.push_back(action->connect<SimpleAction::Activated>(
            [this, item](InputContext* ic) {
                if (onActivated_) onActivated_(item, ic);
            }));
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
    refresh(HcImeInputMode::Telex, true, true, false, false, true, true);
}

void HcImeStatusMenu::registerAction(const std::string& name, Action* action) {
    if (instance_ == nullptr || action == nullptr) return;
    if (instance_->userInterfaceManager().registerAction(name, action)) {
        registeredActions_.push_back(action);
    }
}

void HcImeStatusMenu::registerAll() {
    registerAction("hcime-mode-telex", modeActions_[0].get());
    registerAction("hcime-mode-vni", modeActions_[1].get());
    registerAction("hcime-mode-viqr", modeActions_[2].get());
    registerAction("hcime-mode-hanteles", modeActions_[3].get());
    registerAction("hcime-mode-hanvni", modeActions_[4].get());
    registerAction("hcime-mode-hanviqr", modeActions_[5].get());
    registerAction("hcime-mode-separator", separatorAction_.get());
    registerAction("hcime-toggle-spell-check", toggleActions_[0].get());
    registerAction("hcime-toggle-auto-restore", toggleActions_[1].get());
    registerAction("hcime-toggle-preedit-underline", toggleActions_[2].get());
    registerAction("hcime-toggle-quick-consonants", toggleActions_[3].get());
    registerAction("hcime-toggle-hannom-phrase-prediction", toggleActions_[4].get());
    registerAction("hcime-toggle-hannom-learning", toggleActions_[5].get());
    registerAction("hcime-reset-hannom-learning", resetLearningAction_.get());
}

void HcImeStatusMenu::unregisterAll() {
    if (instance_ == nullptr) {
        registeredActions_.clear();
        return;
    }
    for (auto* action : registeredActions_) {
        instance_->userInterfaceManager().unregisterAction(action);
    }
    registeredActions_.clear();
}

void HcImeStatusMenu::refresh(HcImeInputMode currentMode, bool spellCheck, bool autoRestore,
                              bool underline, bool quickConsonants, bool phrasePrediction, bool learnPhraseRanking) {
    modeActions_[0]->setChecked(currentMode == HcImeInputMode::Telex);
    modeActions_[1]->setChecked(currentMode == HcImeInputMode::Vni);
    modeActions_[2]->setChecked(currentMode == HcImeInputMode::Viqr);
    modeActions_[3]->setChecked(currentMode == HcImeInputMode::HanNomTelex);
    modeActions_[4]->setChecked(currentMode == HcImeInputMode::HanNomVni);
    modeActions_[5]->setChecked(currentMode == HcImeInputMode::HanNomViqr);
    toggleActions_[0]->setChecked(spellCheck);
    toggleActions_[1]->setChecked(autoRestore);
    toggleActions_[2]->setChecked(underline);
    toggleActions_[3]->setChecked(quickConsonants);
    toggleActions_[4]->setChecked(phrasePrediction);
    toggleActions_[5]->setChecked(learnPhraseRanking);
}

void HcImeStatusMenu::attach(InputContext* ic, HcImeInputMode currentMode) {
    auto& statusArea = ic->statusArea();
    statusArea.clearGroup(StatusGroup::InputMethod);
    statusArea.addAction(StatusGroup::InputMethod, modeActions_[1].get());
    statusArea.addAction(StatusGroup::InputMethod, modeActions_[0].get());
    statusArea.addAction(StatusGroup::InputMethod, modeActions_[2].get());
    statusArea.addAction(StatusGroup::InputMethod, modeActions_[3].get());
    statusArea.addAction(StatusGroup::InputMethod, modeActions_[4].get());
    statusArea.addAction(StatusGroup::InputMethod, modeActions_[5].get());
    statusArea.addAction(StatusGroup::InputMethod, separatorAction_.get());
    statusArea.addAction(StatusGroup::InputMethod, toggleActions_[0].get());
    statusArea.addAction(StatusGroup::InputMethod, toggleActions_[1].get());
    statusArea.addAction(StatusGroup::InputMethod, toggleActions_[2].get());
    statusArea.addAction(StatusGroup::InputMethod, toggleActions_[3].get());

    if (currentMode >= HcImeInputMode::HanNomTelex) {
        statusArea.addAction(StatusGroup::InputMethod, toggleActions_[4].get());
        statusArea.addAction(StatusGroup::InputMethod, toggleActions_[5].get());
        statusArea.addAction(StatusGroup::InputMethod, resetLearningAction_.get());
    }
    ic->updateUserInterface(UserInterfaceComponent::StatusArea, true);
}

}  // namespace hcime
