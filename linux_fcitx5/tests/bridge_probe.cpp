#if defined(__GNUC__)
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wsubobject-linkage"
#endif
#include "../src/hcime.cpp"
#if defined(__GNUC__)
#pragma GCC diagnostic pop
#endif

#include <fcitx/event.h>
#include <fcitx/inputcontext.h>
#include <fcitx/inputcontextmanager.h>
#include <fcitx/inputmethodentry.h>
#include <fcitx-utils/capabilityflags.h>

#include <cstdlib>
#include <iostream>
#include <string>
#include <utility>
#include <vector>

using namespace fcitx;

class MockInputContext final : public InputContext {
public:
    explicit MockInputContext(InputContextManager& manager) : InputContext(manager, "hcime-mock") {
        created();
        focusIn();
    }

    explicit MockInputContext(InputContextManager& manager, const std::string& program) : InputContext(manager, program) {
        created();
        focusIn();
    }

    ~MockInputContext() override {
        destroy();
    }

    const char* frontend() const override {
        return "mock";
    }

    std::vector<std::string> commits;
    std::vector<KeySym> forwards;
    std::vector<std::pair<int, unsigned int>> surroundingDeletes;
    int preeditUpdates = 0;

protected:
    void commitStringImpl(const std::string& text) override {
        commits.push_back(text);
        if (surroundingText().isValid()) {
            auto cur = surroundingText().text();
            auto newPos = static_cast<unsigned int>(cur.size() + text.size());
            surroundingText().setText(std::string(cur) + text, newPos, newPos);
        }
    }

    void deleteSurroundingTextImpl(int offset, unsigned int size) override {
        surroundingDeletes.emplace_back(offset, size);
        if (surroundingText().isValid()) {
            auto cur = surroundingText().text();
            auto curLen = static_cast<int>(cur.size());
            int start = curLen + offset;
            if (start < 0) start = 0;
            auto end = start + static_cast<int>(size);
            if (end > curLen) end = curLen;
            auto next = std::string(cur.substr(0, start)) + std::string(cur.substr(end));
            auto pos = static_cast<unsigned int>(start);
            surroundingText().setText(next, pos, pos);
        }
    }

    void forwardKeyImpl(const ForwardKeyEvent& key) override {
        forwards.push_back(key.rawKey().sym());
    }

    void updatePreeditImpl() override {
        ++preeditUpdates;
    }
};

static void require(bool ok, const std::string& message) {
    if (!ok) {
        std::cerr << "FAIL: " << message << "\n";
        std::exit(1);
    }
}

static bool send(hcime::HcImeEngine& engine, const InputMethodEntry& entry, MockInputContext& ic, KeySym sym) {
    KeyEvent event(&ic, Key(sym), false, 0);
    engine.keyEvent(entry, event);
    return event.accepted() && event.filtered();
}

static std::string candidateText(const CandidateList& candidates, int index) {
    return candidates.candidate(index).text().toString();
}

static bool candidateTextSegmentHasFormat(const CandidateList& candidates, int index, int segment,
                                          TextFormatFlag flag) {
    const auto& text = candidates.candidate(index).text();
    return segment >= 0 && static_cast<size_t>(segment) < text.size() && text.formatAt(segment).test(flag);
}

static bool candidateCommentEmpty(const CandidateList& candidates, int index) {
    return candidates.candidate(index).comment().empty();
}

static std::vector<Action*> hcimeStatusActions(MockInputContext& ic) {
    return ic.statusArea().actions(StatusGroup::InputMethod);
}

int main() {
    {
        InputContextManager manager;
        MockInputContext ic(manager);
        hcime::HcImeEngine engine(nullptr);
        const auto entries = engine.listInputMethods();
        const auto& entry = entries.front();
        RawConfig config;
        config.setValueByPath("InputMethod", "HanNomVni");
        engine.setConfig(config);
        require(engine.subMode(entry, ic) == "Hán Nôm (VNI)", "mode switches to Hán Nôm (VNI)");

        require(send(engine, entry, ic, FcitxKey_a), "HanNom VNI a accepted");
        require(ic.inputPanel().candidateList() != nullptr,
                "HanNom VNI live reading exposes candidates before its tone trigger");
        require(send(engine, entry, ic, FcitxKey_1), "HanNom VNI tone digit reaches the core");
        require(ic.commits.empty(), "HanNom VNI unfocused tone digit does not select a candidate");
        require(ic.inputPanel().clientPreedit().toString() == "á", "HanNom VNI composes a plus 1 as á");
    }

    {
        InputContextManager manager;
        MockInputContext ic(manager);
        hcime::HcImeEngine engine(nullptr);
        const auto entries = engine.listInputMethods();
        const auto& entry = entries.front();
        RawConfig config;
        config.setValueByPath("InputMethod", "HanNomVni");
        engine.setConfig(config);

        require(send(engine, entry, ic, FcitxKey_a), "HanNom VNI focused-digit a accepted");
        require(send(engine, entry, ic, FcitxKey_Down), "HanNom VNI Down focuses a candidate");
        auto* candidates = ic.inputPanel().candidateList().get();
        require(candidates != nullptr && candidates->cursorIndex() >= 0,
                "HanNom VNI candidate is focused before its tone trigger");
        require(send(engine, entry, ic, FcitxKey_1), "HanNom VNI focused tone digit reaches the core");
        require(ic.commits.empty(), "HanNom VNI focused tone digit does not select a candidate");
        require(ic.inputPanel().clientPreedit().toString() == "á",
                "HanNom VNI focused a plus 1 composes as á");

        require(send(engine, entry, ic, FcitxKey_Down), "HanNom VNI Down focuses the refreshed candidates");
        candidates = ic.inputPanel().candidateList().get();
        require(candidates != nullptr && candidates->cursorIndex() >= 0,
                "HanNom VNI refreshed candidate is focused for Enter");
        const auto focused = candidateText(*candidates, candidates->cursorIndex());
        require(send(engine, entry, ic, FcitxKey_Return), "HanNom VNI focused Enter selection accepted");
        require(ic.commits.size() == 1 && ic.commits.back() == focused,
                "HanNom VNI Enter selects the exact focused candidate");
    }

    {
        InputContextManager manager;
        MockInputContext ic(manager);
        hcime::HcImeEngine engine(nullptr);
        const auto entries = engine.listInputMethods();
        require(entries.size() == 1, "only one input method is exposed");
        require(entries.front().uniqueName() == "hcime", "the exposed input method is hcime");
        const auto& entry = entries.front();

        require(engine.subMode(entry, ic) == "Telex", "default mode is Telex");
        InputContextEvent activateEvent(&ic, EventType::InputContextInputMethodActivated);
        engine.activate(entry, activateEvent);
        const auto actions = hcimeStatusActions(ic);
        require(actions.size() == 14, "status menu exposes six modes, a separator, phrase controls, and reset");
        require(actions[0]->shortText(&ic) == "VNI", "status menu includes VNI mode");
        require(actions[1]->shortText(&ic) == "TELEX", "status menu includes Telex mode");
        require(actions[2]->shortText(&ic) == "VIQR", "status menu includes VIQR mode");
        require(actions[3]->shortText(&ic) == "HN-TELEX", "status menu includes HN-TELEX mode");
        require(actions[4]->shortText(&ic) == "HN-VNI", "status menu includes HN-VNI mode");
        require(actions[5]->shortText(&ic) == "HN-VIQR", "status menu includes HN-VIQR mode");
        require(actions[6]->isSeparator(), "status menu separates mode and behavior groups");
        require(actions[1]->isChecked(&ic), "Telex mode is checked by default");

        require(send(engine, entry, ic, FcitxKey_a), "a accepted");
        require(ic.inputPanel().clientPreedit().toString() == "a", "preedit after a");
        require(send(engine, entry, ic, FcitxKey_s), "tone accepted");
        const auto tonePreedit = ic.inputPanel().clientPreedit();
        require(tonePreedit.toString() == "á", "tone preedit rendered");
        require(tonePreedit.cursor() == static_cast<int>(tonePreedit.toString().size()), "preedit cursor stays at byte end");

        require(send(engine, entry, ic, FcitxKey_Delete), "active delete accepted");
        require(ic.inputPanel().clientPreedit().toString().empty(), "active delete clears preedit");
        require(ic.forwards.empty(), "active delete is not forwarded as a DEL character");
        require(ic.commits.empty(), "active delete does not commit text");
        require(ic.surroundingDeletes.empty(), "active delete does not request surrounding deletion");

        require(send(engine, entry, ic, FcitxKey_BackSpace), "inactive backspace accepted");
        require(ic.forwards.size() == 1 && ic.forwards.back() == FcitxKey_BackSpace, "inactive backspace forwarded to client");
        require(ic.commits.empty(), "inactive backspace does not commit a control character");
        require(ic.surroundingDeletes.empty(), "inactive backspace does not delete surrounding text");

        ic.forwards.clear();
        require(send(engine, entry, ic, FcitxKey_Delete), "inactive delete accepted");
        require(ic.forwards.size() == 1 && ic.forwards.back() == FcitxKey_Delete, "inactive delete forwarded to client");
        require(ic.commits.empty(), "inactive delete does not commit a DEL character");
        require(ic.surroundingDeletes.empty(), "inactive delete does not delete surrounding text");

        ic.forwards.clear();
        require(send(engine, entry, ic, FcitxKey_a), "navigation setup accepted");
        require(ic.inputPanel().clientPreedit().toString() == "a", "preedit before home");
        require(send(engine, entry, ic, FcitxKey_Home), "home accepted");
        require(ic.inputPanel().clientPreedit().toString().empty(), "home clears preedit");
        require(ic.forwards.size() == 1 && ic.forwards.back() == FcitxKey_Home, "home forwarded to client");

        ic.forwards.clear();
        require(send(engine, entry, ic, FcitxKey_a), "end setup accepted");
        require(send(engine, entry, ic, FcitxKey_End), "end accepted");
        require(ic.inputPanel().clientPreedit().toString().empty(), "end clears preedit");
        require(ic.forwards.size() == 1 && ic.forwards.back() == FcitxKey_End, "end forwarded to client");
    }

    {
        InputContextManager manager;
        MockInputContext ic(manager);
        hcime::HcImeEngine engine(nullptr);
        const auto entries = engine.listInputMethods();
        const auto& entry = entries.front();
        InputContextEvent activateEvent(&ic, EventType::InputContextInputMethodActivated);
        engine.activate(entry, activateEvent);
        const auto actions = hcimeStatusActions(ic);
        actions[0]->activate(&ic);
        require(engine.subMode(entry, ic) == "VNI", "status action switches to VNI");
        require(actions[0]->isChecked(&ic), "VNI action becomes checked");
        require(send(engine, entry, ic, FcitxKey_a), "VNI action a accepted");
        require(send(engine, entry, ic, FcitxKey_1), "VNI action tone accepted");
        require(ic.inputPanel().clientPreedit().toString() == "á", "VNI action composes digits");
    }

    {
        InputContextManager manager;
        MockInputContext ic(manager);
        ic.setCapabilityFlags(CapabilityFlags(CapabilityFlag::SurroundingText));
        ic.surroundingText().setText(" ", 1, 1);
        hcime::HcImeEngine engine(nullptr);
        const auto entries = engine.listInputMethods();
        const auto& entry = entries.front();
        InputContextEvent activateEvent(&ic, EventType::InputContextInputMethodActivated);
        engine.activate(entry, activateEvent);
        const auto actions = hcimeStatusActions(ic);
        actions[0]->activate(&ic);

        require(send(engine, entry, ic, FcitxKey_c), "VNI spaced edit c accepted");
        require(send(engine, entry, ic, FcitxKey_a), "VNI spaced edit a accepted");
        require(send(engine, entry, ic, FcitxKey_1), "VNI spaced edit acute accepted");
        require(ic.inputPanel().clientPreedit().toString() == "cá", "VNI spaced edit composes cá");
        require(send(engine, entry, ic, FcitxKey_space), "VNI spaced edit commits with space");
        require(ic.commits.size() == 1 && ic.commits.back() == "cá", "VNI spaced edit commits cá");
        require(ic.forwards.size() == 1 && ic.forwards.back() == FcitxKey_space, "VNI spaced edit forwards space");

        require(send(engine, entry, ic, FcitxKey_BackSpace), "VNI spaced edit reopens committed word");
        require(ic.forwards.size() == 1, "VNI spaced edit consumes reopening backspace");
        require(ic.surroundingDeletes.size() == 1, "VNI spaced edit deletes committed word and trailing space");
        require(ic.surroundingDeletes.back().first == -3 && ic.surroundingDeletes.back().second == 3,
                "VNI spaced edit deletes cá plus the trailing space");
        require(ic.inputPanel().clientPreedit().toString() == "cá", "VNI spaced edit restores cá as preedit");

        require(send(engine, entry, ic, FcitxKey_2), "VNI spaced edit grave accepted");
        require(ic.inputPanel().clientPreedit().toString() == "cà", "VNI spaced edit changes cá to cà");
    }

    {
        InputContextManager manager;
        MockInputContext ic(manager);
        hcime::HcImeEngine engine(nullptr);
        const auto entries = engine.listInputMethods();
        const auto& entry = entries.front();
        InputContextEvent activateEvent(&ic, EventType::InputContextInputMethodActivated);
        engine.activate(entry, activateEvent);
        const auto actions = hcimeStatusActions(ic);
        actions[0]->activate(&ic);
        require(!send(engine, entry, ic, FcitxKey_1), "standalone VNI digit is passed through");
        require(!send(engine, entry, ic, FcitxKey_0), "standalone VNI zero is passed through");
        require(ic.inputPanel().clientPreedit().toString().empty(), "standalone VNI digits do not create preedit");

        require(send(engine, entry, ic, FcitxKey_a), "VNI literal a accepted");
        require(send(engine, entry, ic, FcitxKey_0), "VNI literal zero commits");
        require(ic.commits.size() == 1 && ic.commits.back() == "a0", "VNI a0 commits as literal text");
        require(ic.inputPanel().clientPreedit().toString().empty(), "VNI literal zero clears preedit");
    }

    {
        InputContextManager manager;
        MockInputContext ic(manager);
        hcime::HcImeEngine engine(nullptr);
        const auto entries = engine.listInputMethods();
        const auto& entry = entries.front();
        InputContextEvent activateEvent(&ic, EventType::InputContextInputMethodActivated);
        engine.activate(entry, activateEvent);
        const auto actions = hcimeStatusActions(ic);
        actions[2]->activate(&ic);
        require(engine.subMode(entry, ic) == "VIQR", "status action switches to VIQR");
        require(actions[2]->isChecked(&ic), "VIQR action becomes checked");
        require(send(engine, entry, ic, FcitxKey_a), "VIQR action a accepted");
        require(send(engine, entry, ic, FcitxKey_apostrophe), "VIQR action boundary accepted");
        require(ic.inputPanel().clientPreedit().toString() == "á", "VIQR action composes boundary triggers");
    }

    {
        InputContextManager manager;
        MockInputContext ic(manager);
        ic.setCapabilityFlags(CapabilityFlags(CapabilityFlag::SurroundingText));
        ic.surroundingText().setText("hello", 5, 5);
        ic.updateSurroundingText();

        hcime::HcImeEngine engine(nullptr);
        const auto entries = engine.listInputMethods();
        const auto& entry = entries.front();
        InputContextEvent activateEvent(&ic, EventType::InputContextInputMethodActivated);
        engine.activate(entry, activateEvent);
        RawConfig config;
        config.setValueByPath("Output/OutputMode", "SurroundingText");
        engine.setConfig(config);

        require(send(engine, entry, ic, FcitxKey_a), "surrounding-text a accepted");
        require(ic.commits.size() == 1 && ic.commits.back() == "a", "surrounding-text inserts first composition into surrounding text");
        require(ic.surroundingDeletes.empty(), "surrounding-text first key does not delete existing content");
        require(ic.inputPanel().clientPreedit().toString().empty(), "surrounding-text mode keeps the client preedit empty");

        require(send(engine, entry, ic, FcitxKey_s), "surrounding-text tone accepted");
        require(ic.commits.size() == 2 && ic.commits.back() == "á", "surrounding-text updates the committed composition");
        require(ic.surroundingDeletes.size() == 1, "surrounding-text second key replaces previous text");
        require(ic.surroundingDeletes.back().first == -1 && ic.surroundingDeletes.back().second == 1,
                "surrounding-text second key deletes the previous single character");
        require(ic.inputPanel().clientPreedit().toString().empty(), "surrounding-text mode still avoids client preedit");
    }

    {
        InputContextManager manager;
        MockInputContext ic(manager);

        hcime::HcImeEngine engine(nullptr);
        const auto entries = engine.listInputMethods();
        const auto& entry = entries.front();
        InputContextEvent activateEvent(&ic, EventType::InputContextInputMethodActivated);
        engine.activate(entry, activateEvent);
        RawConfig config;
        config.setValueByPath("Output/OutputMode", "SurroundingText");
        engine.setConfig(config);

        require(send(engine, entry, ic, FcitxKey_a), "surrounding-text fallback a accepted");
        require(ic.inputPanel().clientPreedit().toString() == "a", "surrounding-text fallback uses preedit when capability is missing");
        require(ic.commits.empty(), "surrounding-text fallback does not commit text");
        require(ic.surroundingDeletes.empty(), "surrounding-text fallback does not delete surrounding text");
    }

    {
        InputContextManager manager;
        MockInputContext ic(manager);
        hcime::HcImeEngine engine(nullptr);
        const auto entries = engine.listInputMethods();
        const auto& entry = entries.front();
        RawConfig config;
        config.setValueByPath("InputMethod", "VNI");
        engine.setConfig(config);
        require(engine.subMode(entry, ic) == "VNI", "mode switches to VNI");
        require(send(engine, entry, ic, FcitxKey_a), "VNI a accepted");
        require(send(engine, entry, ic, FcitxKey_1), "VNI tone accepted");
        require(ic.inputPanel().clientPreedit().toString() == "á", "VNI mode composes digits");
    }

    {
        InputContextManager manager;
        MockInputContext ic(manager);
        hcime::HcImeEngine engine(nullptr);
        const auto entries = engine.listInputMethods();
        const auto& entry = entries.front();
        RawConfig config;
        config.setValueByPath("InputMethod", "VIQR");
        engine.setConfig(config);
        require(engine.subMode(entry, ic) == "VIQR", "mode switches to VIQR");
        require(send(engine, entry, ic, FcitxKey_a), "VIQR a accepted");
        require(send(engine, entry, ic, FcitxKey_apostrophe), "VIQR boundary accepted");
        require(ic.inputPanel().clientPreedit().toString() == "á", "VIQR mode composes boundary triggers");
    }

    {
        InputContextManager manager;
        MockInputContext ic(manager, "firefox");
        hcime::HcImeEngine engine(nullptr);
        const auto entries = engine.listInputMethods();
        const auto& entry = entries.front();
        InputContextEvent activateEvent(&ic, EventType::InputContextInputMethodActivated);
        engine.activate(entry, activateEvent);
        RawConfig config;
        config.setValueByPath("Output/OutputMode", "SurroundingText");
        config.setValueByPath("PerApp/PreeditApps/0", "firefox");
        engine.setConfig(config);

        require(send(engine, entry, ic, FcitxKey_a), "per-app preedit a accepted");
        require(ic.inputPanel().clientPreedit().toString() == "a", "per-app preedit override uses preedit for firefox");
        require(ic.commits.empty(), "per-app preedit override does not commit via surrounding text");
        require(ic.surroundingDeletes.empty(), "per-app preedit override does not delete surrounding text");
    }

    {
        InputContextManager manager;
        MockInputContext ic(manager, "chromium");
        ic.setCapabilityFlags(CapabilityFlags(CapabilityFlag::SurroundingText));
        ic.surroundingText().setText("test", 4, 4);
        ic.updateSurroundingText();

        hcime::HcImeEngine engine(nullptr);
        const auto entries = engine.listInputMethods();
        const auto& entry = entries.front();
        InputContextEvent activateEvent(&ic, EventType::InputContextInputMethodActivated);
        engine.activate(entry, activateEvent);
        RawConfig config;
        config.setValueByPath("Output/OutputMode", "Preedit");
        config.setValueByPath("PerApp/SurroundingTextApps/0", "chromium");
        engine.setConfig(config);

        require(send(engine, entry, ic, FcitxKey_a), "per-app surrounding-text a accepted");
        require(ic.commits.size() == 1 && ic.commits.back() == "a", "per-app surrounding-text override commits for chromium");
        require(ic.inputPanel().clientPreedit().toString().empty(), "per-app surrounding-text override avoids client preedit");
    }

    {
        InputContextManager manager;
        MockInputContext ic(manager);
        ic.setCapabilityFlags(CapabilityFlags(CapabilityFlag::SurroundingText));
        ic.surroundingText().setText("hello", 5, 5);
        ic.updateSurroundingText();

        hcime::HcImeEngine engine(nullptr);
        const auto entries = engine.listInputMethods();
        const auto& entry = entries.front();
        InputContextEvent activateEvent(&ic, EventType::InputContextInputMethodActivated);
        engine.activate(entry, activateEvent);
        RawConfig config;
        config.setValueByPath("Output/OutputMode", "SurroundingText");
        engine.setConfig(config);

        require(send(engine, entry, ic, FcitxKey_a), "re-sync a accepted");
        require(ic.commits.size() == 1 && ic.commits.back() == "a", "re-sync first key commits a");

        require(send(engine, entry, ic, FcitxKey_s), "re-sync tone accepted");
        require(ic.commits.size() == 2 && ic.commits.back() == "á", "re-sync second key commits á");

        ic.surroundingText().setText("hello world", 11, 11);
        ic.updateSurroundingText();

        auto commitsBeforeResync = ic.commits.size();
        auto deletesBeforeResync = ic.surroundingDeletes.size();

        require(send(engine, entry, ic, FcitxKey_w), "re-sync w accepted");
        require(ic.commits.size() == commitsBeforeResync + 1, "re-sync after app modification commits new text");
        require(ic.surroundingDeletes.size() == deletesBeforeResync, "re-sync after app modification does not delete stale surrounding");
    }

    {
        InputContextManager manager;
        MockInputContext ic(manager);
        hcime::HcImeEngine engine(nullptr);
        const auto entries = engine.listInputMethods();
        const auto& entry = entries.front();
        RawConfig config;
        config.setValueByPath("InputMethod", "HanNomTelex");
        engine.setConfig(config);
        require(engine.subMode(entry, ic) == "Hán Nôm (Telex)", "mode switches to Hán Nôm (Telex)");

        require(send(engine, entry, ic, FcitxKey_t), "HanNom 1 t accepted");
        require(send(engine, entry, ic, FcitxKey_h), "HanNom 2 h accepted");
        require(send(engine, entry, ic, FcitxKey_i), "HanNom 3 i accepted");
        require(send(engine, entry, ic, FcitxKey_e), "HanNom 4 e1 accepted");
        require(send(engine, entry, ic, FcitxKey_e), "HanNom 5 e2 accepted");
        require(send(engine, entry, ic, FcitxKey_n), "HanNom 6 n accepted");
        require(ic.inputPanel().clientPreedit().toString() == "thiên", "HanNom 7 Telex composes reading thiên");
        require(ic.inputPanel().candidateList() != nullptr, "HanNom live reading populates candidateList before Space");
        require(ic.inputPanel().candidateList()->size() > 0, "HanNom live reading candidateList is non-empty before Space");
        require(candidateCommentEmpty(*ic.inputPanel().candidateList(), 0),
                "HanNom live reading candidates do not show Vietnamese comments");

        require(send(engine, entry, ic, FcitxKey_Return), "HanNom raw Enter without highlight accepted");
        require(ic.commits.size() == 1 && ic.commits.back() == "thiên", "HanNom raw Enter without highlight commits reading");
        require(ic.inputPanel().candidateList() == nullptr, "HanNom raw Enter clears candidateList");
        require(ic.inputPanel().clientPreedit().toString().empty(), "HanNom raw Enter clears preedit");
    }

    {
        InputContextManager manager;
        MockInputContext ic(manager);
        ic.setCapabilityFlags(CapabilityFlags(CapabilityFlag::SurroundingText));
        ic.surroundingText().setText("prefix", 6, 6);
        ic.updateSurroundingText();

        hcime::HcImeEngine engine(nullptr);
        const auto entries = engine.listInputMethods();
        const auto& entry = entries.front();
        RawConfig config;
        config.setValueByPath("InputMethod", "HanNomTelex");
        config.setValueByPath("Output/OutputMode", "SurroundingText");
        engine.setConfig(config);

        require(send(engine, entry, ic, FcitxKey_t), "HanNom surrounding raw Enter t accepted");
        require(send(engine, entry, ic, FcitxKey_h), "HanNom surrounding raw Enter h accepted");
        require(send(engine, entry, ic, FcitxKey_i), "HanNom surrounding raw Enter i accepted");
        require(send(engine, entry, ic, FcitxKey_e), "HanNom surrounding raw Enter e1 accepted");
        require(send(engine, entry, ic, FcitxKey_e), "HanNom surrounding raw Enter e2 accepted");
        require(send(engine, entry, ic, FcitxKey_n), "HanNom surrounding raw Enter n accepted");
        require(ic.inputPanel().clientPreedit().toString().empty(), "HanNom surrounding mode keeps client preedit empty");
        require(ic.inputPanel().candidateList() != nullptr, "HanNom surrounding live candidateList exists before Space");
        require(ic.inputPanel().candidateList()->size() > 0, "HanNom surrounding live candidateList is non-empty");
        require(ic.inputPanel().candidateList()->cursorIndex() == -1, "HanNom surrounding raw Enter starts with no highlight");

        const auto commitsBeforeEnter = ic.commits.size();
        const auto deletesBeforeEnter = ic.surroundingDeletes.size();
        require(send(engine, entry, ic, FcitxKey_Return), "HanNom surrounding raw Enter accepted");
        require(ic.commits.size() == commitsBeforeEnter + 1 && ic.commits.back() == "thiên",
                "HanNom surrounding raw Enter commits reading via core Enter");
        require(ic.surroundingDeletes.size() > deletesBeforeEnter, "HanNom surrounding raw Enter replaces tracked preedit");
        require(ic.inputPanel().candidateList() == nullptr, "HanNom surrounding raw Enter clears candidateList");
        require(ic.inputPanel().clientPreedit().toString().empty(), "HanNom surrounding raw Enter leaves client preedit empty");
    }

    {
        InputContextManager manager;
        MockInputContext ic(manager);
        hcime::HcImeEngine engine(nullptr);
        const auto entries = engine.listInputMethods();
        const auto& entry = entries.front();
        RawConfig config;
        config.setValueByPath("InputMethod", "HanNomTelex");
        engine.setConfig(config);

        require(send(engine, entry, ic, FcitxKey_n), "HanNom nav n accepted");
        require(send(engine, entry, ic, FcitxKey_a), "HanNom nav a accepted");
        require(send(engine, entry, ic, FcitxKey_m), "HanNom nav m accepted");
        require(ic.inputPanel().clientPreedit().toString() == "nam", "HanNom nav composes reading nam");
        require(ic.inputPanel().candidateList() != nullptr, "HanNom nav live candidateList exists before Space");
        auto* liveCandidates = ic.inputPanel().candidateList().get();
        require(liveCandidates->size() > 1, "HanNom nav reading has at least two live candidates");
        require(candidateTextSegmentHasFormat(*liveCandidates, 0, 0, TextFormatFlag::Bold),
                "HanNom candidate text segment zero is bold");
        require(candidateCommentEmpty(*liveCandidates, 0), "HanNom nav candidates do not show Vietnamese comments");

        require(send(engine, entry, ic, FcitxKey_Down), "HanNom Down highlights first live candidate");
        require(send(engine, entry, ic, FcitxKey_Down), "HanNom second Down highlights second live candidate");
        auto* highlightedCandidates = ic.inputPanel().candidateList().get();
        require(highlightedCandidates != nullptr, "HanNom highlighted candidateList remains visible");
        require(highlightedCandidates->cursorIndex() == 1, "HanNom second candidate is highlighted");
        const auto highlighted = candidateText(*highlightedCandidates, highlightedCandidates->cursorIndex());
        require(send(engine, entry, ic, FcitxKey_Return), "HanNom Enter commits highlighted candidate");
        require(ic.commits.size() == 1 && ic.commits.back() == highlighted, "HanNom Enter commits exact highlighted glyph");
        require(ic.inputPanel().candidateList() == nullptr, "HanNom highlighted Enter clears candidateList");
        require(ic.inputPanel().clientPreedit().toString().empty(), "HanNom highlighted Enter clears preedit");

        for (auto key : {FcitxKey_n, FcitxKey_h, FcitxKey_a, FcitxKey_f}) {
            require(send(engine, entry, ic, key), "HanNom page-prefix key accepted");
        }
        require(send(engine, entry, ic, FcitxKey_space), "HanNom page-prefix delimiter accepted");
        require(ic.inputPanel().candidateList() != nullptr, "HanNom page candidateList exists");
        auto* pagedCandidates = dynamic_cast<CommonCandidateList*>(ic.inputPanel().candidateList().get());
        require(pagedCandidates != nullptr && pagedCandidates->hasNext(),
                "HanNom V3 exposes a native Fcitx second candidate page");
        const auto pageOneFirst = candidateText(*ic.inputPanel().candidateList(), 0);
        require(send(engine, entry, ic, FcitxKey_Page_Down), "HanNom PageDown moves candidate page");
        require(candidateText(*ic.inputPanel().candidateList(), 0) != pageOneFirst,
                "HanNom PageDown changes displayed candidates");
        require(send(engine, entry, ic, FcitxKey_Page_Up), "HanNom PageUp restores candidate page");
        require(candidateText(*ic.inputPanel().candidateList(), 0) == pageOneFirst,
                "HanNom PageUp restores displayed candidates");
        require(send(engine, entry, ic, FcitxKey_Page_Down), "HanNom PageDown reopens page two");
        const auto pageTwoFirst = candidateText(*ic.inputPanel().candidateList(), 0);
        require(pageTwoFirst != pageOneFirst, "HanNom page two has a distinct visible candidate");
        require(send(engine, entry, ic, FcitxKey_1), "HanNom page-two numeric selection accepted");
        require(ic.commits.back() == pageTwoFirst,
                "HanNom page-two numeric selection uses the candidate global index");

        for (auto key : {FcitxKey_n, FcitxKey_h, FcitxKey_a, FcitxKey_f}) {
            require(send(engine, entry, ic, key), "HanNom numeric key accepted");
        }
        require(ic.inputPanel().candidateList()->cursorIndex() == -1,
                "HanNom numeric selection starts without a focus highlight");
        const auto numericCandidate = candidateText(*ic.inputPanel().candidateList(), 0);
        require(send(engine, entry, ic, FcitxKey_1), "HanNom numeric selection works before focus");
        require(ic.commits.back() == numericCandidate, "HanNom numeric selection commits visible candidate");

    }

    {
        InputContextManager manager;
        MockInputContext ic(manager);
        hcime::HcImeEngine engine(nullptr);
        const auto entries = engine.listInputMethods();
        const auto& entry = entries.front();
        RawConfig config;
        config.setValueByPath("InputMethod", "HanNomTelex");
        engine.setConfig(config);

        for (auto key : {FcitxKey_t, FcitxKey_h, FcitxKey_a, FcitxKey_n, FcitxKey_h, FcitxKey_f}) {
            require(send(engine, entry, ic, key), "phrase first word key accepted");
        }
        require(send(engine, entry, ic, FcitxKey_space), "phrase delimiter accepted");
        require(ic.inputPanel().candidateList() != nullptr && ic.inputPanel().candidateList()->size() > 0,
                "first phrase word shows typeahead predictions");
        for (auto key : {FcitxKey_p, FcitxKey_h, FcitxKey_o, FcitxKey_o, FcitxKey_s}) {
            require(send(engine, entry, ic, key), "phrase second word key accepted");
        }
        require(ic.inputPanel().candidateList() != nullptr, "exact phrase keeps candidates visible");
        require(candidateText(*ic.inputPanel().candidateList(), 0) == "城庯", "phrase candidate renders full glyph string");
        require(candidateCommentEmpty(*ic.inputPanel().candidateList(), 0),
                "phrase candidates do not show Vietnamese comments");
        require(ic.inputPanel().candidateList()->cursorIndex() == -1,
                "exact phrase starts without a focus highlight");
        require(send(engine, entry, ic, FcitxKey_space), "second phrase Space accepted");
        require(ic.commits.empty(), "second phrase Space does not commit");
        require(ic.inputPanel().candidateList() != nullptr && ic.inputPanel().candidateList()->size() > 0,
                "second phrase Space keeps candidates visible");
        require(candidateText(*ic.inputPanel().candidateList(), 0) == "城庯",
                "second phrase Space preserves the top candidate");
        require(ic.inputPanel().candidateList()->cursorIndex() == -1,
                "second phrase Space keeps the candidate list unfocused");
        require(send(engine, entry, ic, FcitxKey_Return), "phrase unfocused Enter accepted");
        require(ic.commits.size() == 1 && ic.commits.back() == "城庯",
                "phrase unfocused Enter commits the top candidate");
    }

    std::cout << "HC_IME bridge probe passed\n";
}
