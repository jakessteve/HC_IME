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

namespace {

void clearTrackedSurrounding(ContextState& state) {
    state.clearPreviousSurrounding();
}

size_t utf8ByteOffset(const std::string& text, unsigned int charOffset) {
    if (charOffset == 0) return 0;
    return static_cast<size_t>(utf8::ncharByteLength(text.begin(), charOffset));
}

bool trackedSurroundingMatches(InputContext* ic, const ContextState& state) {
    if (ic == nullptr || state.previousSurroundingText.empty()) return false;
    const auto& surrounding = ic->surroundingText();
    if (!surrounding.isValid() || surrounding.cursor() != surrounding.anchor() ||
        surrounding.cursor() != state.previousSurroundingCursor ||
        surrounding.anchor() != state.previousSurroundingAnchor ||
        !utf8::validate(surrounding.text())) {
        return false;
    }
    const auto cursor = surrounding.cursor();
    const auto trackedChars = static_cast<unsigned int>(utf8::length(state.previousSurroundingText));
    const auto surroundingChars = static_cast<unsigned int>(utf8::length(surrounding.text()));
    if (cursor > surroundingChars || cursor < trackedChars) return false;
    const auto startByte = utf8ByteOffset(surrounding.text(), cursor - trackedChars);
    const auto cursorByte = utf8ByteOffset(surrounding.text(), cursor);
    return surrounding.text().compare(startByte, state.previousSurroundingText.size(),
                                      state.previousSurroundingText) == 0 &&
        surrounding.text().substr(cursorByte) == state.previousSurroundingSuffix;
}

void captureTrackedSurrounding(InputContext* ic, ContextState& state) {
    if (ic == nullptr || !ic->surroundingText().isValid() ||
        ic->surroundingText().cursor() != ic->surroundingText().anchor() ||
        !utf8::validate(ic->surroundingText().text())) {
        clearTrackedSurrounding(state);
        return;
    }
    const auto& surrounding = ic->surroundingText();
    const auto surroundingChars = static_cast<unsigned int>(utf8::length(surrounding.text()));
    if (surrounding.cursor() > surroundingChars) {
        clearTrackedSurrounding(state);
        return;
    }
    const auto cursorByte = utf8ByteOffset(surrounding.text(), surrounding.cursor());
    state.previousSurroundingSuffix = surrounding.text().substr(cursorByte);
    state.previousSurroundingCursor = surrounding.cursor();
    state.previousSurroundingAnchor = surrounding.anchor();
}

}  // namespace

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
    if (ic == nullptr || !ic->surroundingText().isValid()) {
        clearTrackedSurrounding(state);
        state.surroundingTextEnabled = false;
        state.surroundingTextSuppressed = true;
        if (ic != nullptr) setPreedit(ic, newPreedit, false, 0);
        return;
    }
    if (state.previousSurroundingText.empty()) {
        ic->commitString(newPreedit);
        state.previousSurroundingText = newPreedit;
        captureTrackedSurrounding(ic, state);
        return;
    }
    if (!trackedSurroundingMatches(ic, state)) {
        // The client moved the cursor, selected text, or changed the suffix.
        // Do not issue a destructive replacement against an untrusted cache.
        clearTrackedSurrounding(state);
        state.surroundingTextEnabled = false;
        state.surroundingTextSuppressed = true;
        setPreedit(ic, newPreedit, false, 0);
        return;
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
    captureTrackedSurrounding(ic, state);
}

bool HcImeCandidateAdapter::commitViaSurroundingText(InputContext* ic, ContextState& state,
                                                     const std::string& committedText) {
    if (state.previousSurroundingText.empty() || trackedSurroundingMatches(ic, state)) {
        if (!state.previousSurroundingText.empty()) {
        auto surroundingLen = utf8::length(state.previousSurroundingText);
        if (surroundingLen > 0) {
            ic->deleteSurroundingText(-static_cast<int>(surroundingLen), surroundingLen);
        }
        }
        ic->commitString(committedText);
    } else {
        // Preserve the client's text on drift; never delete an unrelated suffix.
        clearTrackedSurrounding(state);
        state.surroundingTextEnabled = false;
        state.surroundingTextSuppressed = true;
        ic->commitString(committedText);
        return false;
    }
    clearTrackedSurrounding(state);
    return true;
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
        state.pendingCommit.clear();
        state.clearPreviousSurrounding();
        state.surroundingTextEnabled = false;
        state.surroundingTextSuppressed = false;
        ic->inputPanel().setCandidateList(nullptr);
        ic->updateUserInterface(UserInterfaceComponent::InputPanel, true);
        return;
    }

    if (nomResult.status_flag == HC_STATUS_IN_PROGRESS) {
        std::string output(reinterpret_cast<const char*>(nomResult.reading), nomResult.reading_len);
        state.pendingCommit.clear();
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
