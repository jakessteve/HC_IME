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
    }

    void deleteSurroundingTextImpl(int offset, unsigned int size) override {
        surroundingDeletes.emplace_back(offset, size);
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

static std::vector<Action*> hcimeStatusActions(MockInputContext& ic) {
    return ic.statusArea().actions(StatusGroup::InputMethod);
}

int main() {
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
        require(actions.size() == 8, "status menu exposes three modes, a separator, and four behavior toggles");
        require(actions[0]->shortText(&ic) == "VNI", "status menu includes VNI mode");
        require(actions[1]->shortText(&ic) == "TELEX", "status menu includes Telex mode");
        require(actions[2]->shortText(&ic) == "VIQR", "status menu includes VIQR mode");
        require(actions[3]->isSeparator(), "status menu separates mode and behavior groups");
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

    std::cout << "HC_IME bridge probe passed\n";
}
