#include "hcime_candidate_adapter.h"
#include "hcime_key_handler.h"

#include <fcitx/inputcontext.h>
#include <fcitx/inputpanel.h>
#include <fcitx/userinterface.h>
#include <fcitx-utils/capabilityflags.h>
#include <fcitx-utils/utf8.h>

#include <cstring>
#include <string>

namespace hcime {

using namespace fcitx;

std::unique_ptr<CommonCandidateList> HcImeCandidateAdapter::buildCandidateList(
    const HC_HanNomResultV3& result, HcNomCandidateWord::SelectCallback onSelect) {
    auto candidateList = std::make_unique<CommonCandidateList>();
    candidateList->setLayoutHint(CandidateLayoutHint::Horizontal);
    candidateList->setPageSize(9);
    for (uint16_t i = 0; i < result.candidate_count; ++i) {
        std::string candStr(reinterpret_cast<const char*>(result.candidates[i].text),
                            result.candidates[i].text_len);
        Text wordText(candStr, TextFormatFlag::NoFlag);
        Text commentText;
        candidateList->append<HcNomCandidateWord>(wordText, commentText, i, onSelect);
    }
    return candidateList;
}

void HcImeCandidateAdapter::applySurroundingTextPreedit(InputContext* ic, ContextState& state,
                                                        const std::string& newPreedit) {
    if (state.previousSurroundingText.empty() || !ic->surroundingText().isValid()) {
        state.previousSurroundingText.clear();
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

void HcImeCandidateAdapter::commitViaSurroundingText(InputContext* ic, ContextState& state,
                                                     const std::string& committedText) {
    if (!state.previousSurroundingText.empty()) {
        auto surroundingLen = utf8::length(state.previousSurroundingText);
        if (surroundingLen > 0) {
            ic->deleteSurroundingText(-static_cast<int>(surroundingLen), surroundingLen);
        }
    }
    ic->commitString(committedText);
    state.previousSurroundingText.clear();
}

void HcImeCandidateAdapter::setPreedit(InputContext* ic, const std::string& text,
                                       bool underline, int32_t spellCheckStatus) {
    TextFormatFlag flags = TextFormatFlag::NoFlag;
    if (spellCheckStatus == HC_SPELL_CHECK_INVALID) {
        flags = TextFormatFlag::HighLight;
    } else if (spellCheckStatus == HC_SPELL_CHECK_ENGLISH_FALLBACK) {
        flags = TextFormatFlag::Strike;
    } else if (underline) {
        flags = TextFormatFlag::Underline;
    }
    Text preedit(text, flags);
    preedit.setCursor(static_cast<int>(text.size()));
    ic->inputPanel().setClientPreedit(preedit);
    if (ic->capabilityFlags().test(CapabilityFlag::Preedit)) {
        ic->inputPanel().setPreedit(Text());
    } else {
        ic->inputPanel().setPreedit(preedit);
    }
    ic->updatePreedit();
}

void HcImeCandidateAdapter::clearPreedit(InputContext* ic) {
    ic->inputPanel().setClientPreedit(Text());
    ic->inputPanel().setPreedit(Text());
    ic->updatePreedit();
}

void HcImeCandidateAdapter::updateHanNomUi(InputContext* ic, ContextState& state,
                                           const HC_HanNomResultV3& nomResult,
                                           bool useSurroundingText, bool displayUnderline,
                                           HcNomCandidateWord::SelectCallback onSelect) {
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
            ic->inputPanel().setCandidateList(buildCandidateList(nomResult, onSelect));
        } else {
            ic->inputPanel().setCandidateList(nullptr);
        }

        if (useSurroundingText && state.hasActivePreedit) {
            applySurroundingTextPreedit(ic, state, output);
        } else {
            setPreedit(ic, output, displayUnderline, 0);
        }
        ic->updateUserInterface(UserInterfaceComponent::InputPanel, true);
    }
}

}  // namespace hcime
