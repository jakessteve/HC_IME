#pragma once

#include "hcime/hc_core_ffi.h"

#include <fcitx/candidatelist.h>
#include <fcitx/text.h>

#include <functional>
#include <memory>
#include <string>

namespace fcitx {
class InputContext;
class InputPanel;
}  // namespace fcitx

namespace hcime {

struct ContextState;

class HcNomCandidateWord : public fcitx::CandidateWord {
public:
    using SelectCallback = std::function<void(fcitx::InputContext*, int)>;

    HcNomCandidateWord(fcitx::Text text, fcitx::Text comment, int index, SelectCallback onSelect)
        : CandidateWord(std::move(text)), index_(index), onSelect_(std::move(onSelect)) {
        if (!comment.empty()) {
            setComment(std::move(comment));
        }
    }

    void select(fcitx::InputContext* ic) const override {
        if (onSelect_) onSelect_(ic, index_);
    }

private:
    int index_;
    SelectCallback onSelect_;
};

class HcImeCandidateAdapter {
public:
    static std::unique_ptr<fcitx::CommonCandidateList> buildCandidateList(
        const HC_HanNomResultV3& result, HcNomCandidateWord::SelectCallback onSelect);

    static void applySurroundingTextPreedit(fcitx::InputContext* ic, ContextState& state,
                                            const std::string& newPreedit);

    static bool commitViaSurroundingText(fcitx::InputContext* ic, ContextState& state,
                                         const std::string& committedText);

    static void setPreedit(fcitx::InputContext* ic, const std::string& text,
                           bool underline, int32_t spellCheckStatus);

    static void clearPreedit(fcitx::InputContext* ic);

    static void updateHanNomUi(fcitx::InputContext* ic, ContextState& state,
                               const HC_HanNomResultV3& nomResult,
                               bool useSurroundingText, bool displayUnderline,
                               HcNomCandidateWord::SelectCallback onSelect);
};

}  // namespace hcime
