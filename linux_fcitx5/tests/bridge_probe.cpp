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

int main() {
    InputContextManager manager;
    MockInputContext ic(manager);
    hcime::HcImeEngine engine(nullptr);
    InputMethodEntry entry("hcime-telex", "HC_IME Telex", "vi", "hcime");

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

    MockInputContext vniIc(manager);
    InputMethodEntry vniEntry("hcime-vni", "HC_IME VNI", "vi", "hcime");
    require(send(engine, vniEntry, vniIc, FcitxKey_p), "vni p accepted");
    require(send(engine, vniEntry, vniIc, FcitxKey_h), "vni h accepted");
    require(send(engine, vniEntry, vniIc, FcitxKey_u), "vni u accepted");
    require(send(engine, vniEntry, vniIc, FcitxKey_o), "vni o accepted");
    require(send(engine, vniEntry, vniIc, FcitxKey_n), "vni n accepted");
    require(send(engine, vniEntry, vniIc, FcitxKey_g), "vni g accepted");
    require(send(engine, vniEntry, vniIc, FcitxKey_7), "vni horn accepted");
    require(vniIc.inputPanel().clientPreedit().toString() == "phương", "vni preedit rendered");
    require(send(engine, vniEntry, vniIc, FcitxKey_Delete), "vni active delete accepted");
    require(vniIc.inputPanel().clientPreedit().toString() == "phươn", "vni delete removes final visible char");
    require(send(engine, vniEntry, vniIc, FcitxKey_BackSpace), "vni active backspace accepted");
    require(vniIc.inputPanel().clientPreedit().toString() == "phươ", "vni backspace removes n");
    require(send(engine, vniEntry, vniIc, FcitxKey_BackSpace), "vni active backspace removes vowel");
    require(vniIc.inputPanel().clientPreedit().toString() == "phư", "vni backspace removes whole horned o");

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

    std::cout << "HC_IME bridge probe passed\n";
}
