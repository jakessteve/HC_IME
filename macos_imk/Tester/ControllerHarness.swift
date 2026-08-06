import Carbon
import Cocoa
import InputMethodKit

// Headless harness for the shipping `HCIMEInputController`.
//
// Everything else in the tester exercises `HCTextView`, which is a *copy* of the
// controller's logic — so a controller-only defect could not fail any test, and
// two release blockers shipped as "verified" (MAC-08). This file drives the real
// `HCIMEInputController` object, through the real `handle(_:client:)` entry
// point, against a mock client that implements `IMKTextInput`.
//
// What it covers: everything the controller does through the `IMKTextInput`
// protocol — marked text, insertions, replacement ranges, and what it returns to
// IMK for each key. What it does not cover: IMK itself (event delivery,
// keybinding, `didCommandBySelector:`), the candidate panel (`IMKCandidates` is
// nil here), the input-source menu, and the real behaviour of any application —
// `MockTextInput` is a model of a well-behaved TSMDocumentAccess client, not of
// Safari or Word.

// MARK: - Mock client

/// A UTF-16 document with a caret and a marked range, behaving as
/// `IMKTextInput` documents a client should.
///
/// `supportsDocumentAccess = false` reproduces the large family of clients that
/// do not implement TSMDocumentAccess: per Apple's header, `selectedRange` and
/// `length` answer `NSNotFound` and replacement ranges are ignored outright.
final class MockTextInput: NSObject, IMKTextInput {

    private(set) var document = NSMutableString()
    /// Insertion point, as a UTF-16 offset into `document`.
    private(set) var caret = 0
    private(set) var mark: NSRange?
    var supportsDocumentAccess = true

    /// Every `IMKTextInput` call the controller made, for tests that care about
    /// the calls and not only about the resulting text.
    private(set) var calls: [String] = []

    var text: String { document as String }

    // MARK: IMKTextInput

    func insertText(_ string: Any!, replacementRange: NSRange) {
        let inserted = MockTextInput.plainText(string)
        calls.append("insertText(\"\(inserted)\", \(MockTextInput.describe(replacementRange)))")
        replace(range: resolve(replacementRange), with: inserted, marking: false)
    }

    func setMarkedText(_ string: Any!, selectionRange: NSRange, replacementRange: NSRange) {
        let marked = MockTextInput.plainText(string)
        calls.append("setMarkedText(\"\(marked)\", \(MockTextInput.describe(replacementRange)))")
        replace(range: resolve(replacementRange), with: marked, marking: true,
                caretWithin: selectionRange.location)
    }

    func selectedRange() -> NSRange {
        supportsDocumentAccess
            ? NSRange(location: caret, length: 0)
            : NSRange(location: NSNotFound, length: NSNotFound)
    }

    func markedRange() -> NSRange { mark ?? NSRange(location: NSNotFound, length: NSNotFound) }

    func attributedSubstring(from range: NSRange) -> NSAttributedString! {
        // Apple's header: without TSMDocumentAccess this does not read the
        // requested range at all — the client answers with its selected text.
        guard supportsDocumentAccess else { return NSAttributedString(string: "") }
        guard range.location != NSNotFound, range.location >= 0, range.length >= 0,
              range.location + range.length <= document.length
        else { return nil }
        return NSAttributedString(string: document.substring(with: range))
    }

    func length() -> Int { supportsDocumentAccess ? document.length : NSNotFound }

    func characterIndex(
        for point: NSPoint, tracking mappingMode: IMKLocationToOffsetMappingMode,
        inMarkedRange: UnsafeMutablePointer<ObjCBool>!
    ) -> Int { NSNotFound }

    func attributes(
        forCharacterIndex index: Int, lineHeightRectangle lineRect: UnsafeMutablePointer<NSRect>!
    ) -> [AnyHashable: Any]! { [:] }

    func validAttributesForMarkedText() -> [Any]! { [] }
    func overrideKeyboard(withKeyboardNamed keyboardUniqueName: String!) {}
    func selectMode(_ modeIdentifier: String!) {}
    func supportsUnicode() -> Bool { true }
    func bundleIdentifier() -> String! { "dev.hcime.mockclient" }
    func windowLevel() -> CGWindowLevel { 0 }
    func supportsProperty(_ property: TSMDocumentPropertyTag) -> Bool { supportsDocumentAccess }
    func uniqueClientIdentifierString() -> String! { "mock" }

    func string(from range: NSRange, actualRange: NSRangePointer!) -> String! {
        attributedSubstring(from: range)?.string
    }

    func firstRect(forCharacterRange aRange: NSRange, actualRange: NSRangePointer!) -> NSRect {
        .zero
    }

    // MARK: The application's own editing

    /// Text that reaches the document without any input-method involvement: a
    /// paste, an autocompletion, or the user clicking into another spot and
    /// typing while a different input source is active. This is the situation
    /// MAC-01 destroys text in — IMK sends no notification for any of it.
    func applicationInserts(_ text: String) {
        document.replaceCharacters(in: NSRange(location: caret, length: 0), with: text)
        caret += (text as NSString).length
        mark = nil
    }

    /// The application deleting one character back, which is what it does with
    /// the Backspace an input method returns `false` for.
    func applicationDeletesBackward() {
        guard caret > 0 else { return }
        let start = document.rangeOfComposedCharacterSequence(at: caret - 1).location
        document.deleteCharacters(in: NSRange(location: start, length: caret - start))
        caret = start
        mark = nil
    }

    /// Slack, Discord and Messages: Enter sends the message and empties the
    /// field. No input-method callback accompanies any of it.
    func clearDocumentLikeSendOnEnter() {
        document.setString("")
        caret = 0
        mark = nil
    }

    /// A caret move with no edit — an arrow key, or a mouse click.
    func applicationMovesCaret(by delta: Int) {
        caret = max(0, min(document.length, caret + delta))
        mark = nil
    }

    // MARK: Internals

    private func resolve(_ replacementRange: NSRange) -> NSRange {
        // A client without TSMDocumentAccess ignores the replacement range.
        if supportsDocumentAccess, replacementRange.location != NSNotFound,
           replacementRange.location >= 0,
           replacementRange.location + replacementRange.length <= document.length
        {
            return replacementRange
        }
        return mark ?? NSRange(location: caret, length: 0)
    }

    private func replace(
        range: NSRange, with text: String, marking: Bool, caretWithin offset: Int = 0
    ) {
        document.replaceCharacters(in: range, with: text)
        let length = (text as NSString).length
        if marking, length > 0 {
            mark = NSRange(location: range.location, length: length)
            caret = range.location + max(0, min(offset, length))
        } else {
            mark = nil
            caret = range.location + length
        }
    }

    private static func plainText(_ string: Any!) -> String {
        (string as? String) ?? (string as? NSAttributedString)?.string ?? ""
    }

    private static func describe(_ range: NSRange) -> String {
        range.location == NSNotFound ? "unranged" : "{\(range.location), \(range.length)}"
    }
}

// MARK: - Keys

/// One key as IMK delivers it, paired with what the application does to the
/// document when the input method returns `false`. Modelling that second half
/// matters: a suppressed reconversion is only correct if the Backspace still
/// reaches the document.
enum HarnessKey {
    case character(String)
    case backspace
    case enter
    case escape
    case arrowLeft
    case arrowRight
    /// Any other key, exactly as AppKit delivers it. The application does
    /// nothing with these when the controller declines them, so a test that
    /// cares about what the application would do has to say so itself.
    case raw(characters: String, keyCode: UInt16, flags: NSEvent.ModifierFlags)

    /// A function key, Help, Clear or Menu. AppKit puts these in `characters`
    /// as private-use scalars — the thing MAC-03 mistook for text.
    static func functionKey(_ scalar: UInt32, keyCode: UInt16) -> HarnessKey {
        .raw(characters: String(Character(Unicode.Scalar(scalar)!)), keyCode: keyCode, flags: [
            .function,
        ])
    }

    static let forwardDelete = HarnessKey.functionKey(0xF728, keyCode: 117)
    static let tab = HarnessKey.raw(characters: "\t", keyCode: 48, flags: [])
    static let arrowDown = HarnessKey.functionKey(0xF701, keyCode: 125)
    static let arrowUp = HarnessKey.functionKey(0xF700, keyCode: 126)
    static let pageDown = HarnessKey.functionKey(0xF72D, keyCode: 121)
    static let pageUp = HarnessKey.functionKey(0xF72C, keyCode: 116)

    /// Option-typed text: on a US layout Option+A produces "å".
    static func option(_ characters: String, keyCode: UInt16) -> HarnessKey {
        .raw(characters: characters, keyCode: keyCode, flags: [.option])
    }

    /// The system-wide delete-previous-word.
    static let optionBackspace = HarnessKey.raw(
        characters: "\u{7F}", keyCode: 51, flags: [.option])

    static func command(_ characters: String, keyCode: UInt16) -> HarnessKey {
        .raw(characters: characters, keyCode: keyCode, flags: [.command])
    }

    private var keyCode: UInt16 {
        switch self {
        case .character(let text): return text == " " ? 49 : 0
        case .backspace: return 51
        case .enter: return 36
        case .escape: return 53
        case .arrowLeft: return 123
        case .arrowRight: return 124
        case .raw(_, let keyCode, _): return keyCode
        }
    }

    private var characters: String {
        switch self {
        case .character(let text): return text
        case .raw(let characters, _, _): return characters
        default: return ""
        }
    }

    private var flags: NSEvent.ModifierFlags {
        if case .raw(_, _, let flags) = self { return flags }
        return []
    }

    var event: NSEvent {
        NSEvent.keyEvent(
            with: .keyDown, location: .zero, modifierFlags: flags, timestamp: 0, windowNumber: 0,
            context: nil, characters: characters, charactersIgnoringModifiers: characters,
            isARepeat: false, keyCode: keyCode)!
    }

    func applyToClient(_ client: MockTextInput) {
        switch self {
        case .character(let text): client.applicationInserts(text)
        case .backspace: client.applicationDeletesBackward()
        case .enter: client.applicationInserts("\n")
        case .escape, .raw: break
        case .arrowLeft: client.applicationMovesCaret(by: -1)
        case .arrowRight: client.applicationMovesCaret(by: 1)
        }
    }
}

// MARK: - Mock candidate panel

/// A stand-in for `IMKCandidates`, which cannot be constructed without an
/// `IMKServer` and is therefore nil in this process.
///
/// It models the panel's state — visible or not, which row is highlighted — and
/// records what the controller asked of it. It does **not** model the real
/// panel's own handling of the events it is given, its drawing, or its
/// selection-key machinery; those stay untested here.
final class FakeCandidatePanel: HCCandidatePanel {

    weak var controller: HCIMEInputController?

    private(set) var isPanelVisible = false
    private(set) var rows = 0
    private(set) var highlightedRow: Int?
    private(set) var moves: [HCPanelMove] = []
    private(set) var selectionKeys: [Int] = []
    private(set) var updates = 0

    func showPanel() { isPanelVisible = true }

    func hidePanel() {
        isPanelVisible = false
        highlightedRow = nil
    }

    /// The real panel answers `updateCandidates` by calling back into the
    /// controller's `candidates(_:)`; so does this one, so the row count is the
    /// controller's own list rather than something the test made up.
    func updatePanel() {
        updates += 1
        rows = (controller?.candidates(nil) as? [String])?.count ?? 0
        if let highlighted = highlightedRow, highlighted >= rows { highlightedRow = nil }
    }

    func setPanelAttributes(_ attributes: [AnyHashable: Any]) {}

    func setPanelSelectionKeys(_ keyCodes: [Int]) { selectionKeys = keyCodes }

    func movePanelSelection(_ move: HCPanelMove) -> Bool {
        moves.append(move)
        guard rows > 0 else { return false }
        let page = 9
        switch move {
        case .next: highlightedRow = min((highlightedRow ?? -1) + 1, rows - 1)
        case .previous: highlightedRow = max((highlightedRow ?? 1) - 1, 0)
        case .nextPage: highlightedRow = min((highlightedRow ?? 0) + page, rows - 1)
        case .previousPage: highlightedRow = max((highlightedRow ?? 0) - page, 0)
        }
        return true
    }

    var highlightedCandidateIndex: Int? { highlightedRow }
}

// MARK: - Harness

/// A controller plus the client it is typing into.
final class ControllerHarness {

    let controller: HCIMEInputController
    let client = MockTextInput()
    let panel: FakeCandidatePanel?

    /// - Parameters:
    ///   - mode: the mode to pin, or nil to keep whatever the controller
    ///     resolved from `inputSourceID` — which is what MAC-05 is about.
    ///   - inputSourceID: the input source the user is supposed to have picked.
    ///     Always assigned, so a machine's real Text Input Sources database can
    ///     never leak into a test.
    ///   - panel: installs a stand-in candidate panel for the lifetime of the
    ///     harness. `IMKCandidates` is nil in this process, so without one every
    ///     panel path is dead code.
    init(
        mode: HCInputMode? = .telex, documentAccess: Bool = true, inputSourceID: String? = nil,
        panel: FakeCandidatePanel? = nil
    ) {
        HCIMEInputController.currentInputSourceID = { inputSourceID }
        // The same construction IMK performs, minus the server connection.
        controller = HCIMEInputController(server: nil, delegate: nil, client: nil)
        client.supportsDocumentAccess = documentAccess
        self.panel = panel
        panel?.controller = controller
        HCIMEApplication.candidatesPanel = panel
        controller.configureForTesting(mode: mode, settings: HCSettings())
    }

    /// Takes the stand-in panel back out of the process-wide slot. Every test
    /// that installs one must call this, or the next test inherits it.
    func teardown() {
        HCIMEApplication.candidatesPanel = nil
        HCIMEInputController.currentInputSourceID = { nil }
    }

    /// Sends one key and, when the controller declines it, lets the application
    /// have it — exactly the IMK contract.
    @discardableResult
    func send(_ key: HarnessKey) -> Bool {
        let handled = controller.handle(key.event, client: client)
        if !handled { key.applyToClient(client) }
        return handled
    }

    /// Sends a key and reports what the controller returned without letting the
    /// application act, for assertions about the controller in isolation.
    @discardableResult
    func sendWithoutApplication(_ key: HarnessKey) -> Bool {
        controller.handle(key.event, client: client)
    }

    func type(_ keys: String) {
        for character in keys { send(.character(String(character))) }
    }
}

// MARK: - Tests

/// Runs the controller regression suite. Returns the number of failures.
func runControllerSelfTest() -> Int {
    var failures = 0

    func check(_ label: String, _ actual: String, _ expected: String) {
        let ok = actual == expected
        if !ok { failures += 1 }
        print(
            "[\(ok ? "PASS" : "FAIL")] controller: \(label) — \(actual.debugDescription)"
                + (ok ? "" : " (expected \(expected.debugDescription))"))
    }

    func check(_ label: String, _ actual: Bool, _ expected: Bool) {
        let ok = actual == expected
        if !ok { failures += 1 }
        print(
            "[\(ok ? "PASS" : "FAIL")] controller: \(label) — \(actual)"
                + (ok ? "" : " (expected \(expected))"))
    }

    // The controller has to be constructible headlessly for any of this to mean
    // anything; if IMK ever stops allowing it, say so rather than skip silently.
    let smoke = ControllerHarness()
    smoke.type("tieengs")
    check("preedit reaches the client", smoke.client.text, "tiếng")
    check("preedit is marked", smoke.client.markedRange() == NSRange(location: 0, length: 5), true)
    smoke.send(.character(" "))
    check("commit on space", smoke.client.text, "tiếng ")

    // The ordinary reconversion: Backspace straight after a commit reopens the
    // word in place. This must keep working — a fix for MAC-01/02 that simply
    // switched reconversion off would pass every other test in this file.
    do {
        let harness = ControllerHarness()
        harness.type("tieengs ")
        let handled = harness.send(.backspace)
        check("reconversion after commit is handled", handled, true)
        check("reconversion replaces the commit in place", harness.client.text, "tiếng")
        check(
            "reconversion leaves the word marked",
            harness.client.markedRange() == NSRange(location: 0, length: 5), true)
    }

    // Same, committed with a boundary character rather than a space.
    do {
        let harness = ControllerHarness()
        harness.type("tieengs.")
        check("boundary commit", harness.client.text, "tiếng.")
        harness.send(.backspace)
        check("reconversion after a boundary commit", harness.client.text, "tiếng")
    }

    // MAC-01. Text arrives with no IMK callback — a click elsewhere and typing,
    // a paste, an autocompletion. The recorded commit is now nowhere near the
    // caret, and the old controller deleted six characters of it anyway:
    // "tiếng ABCDEFGHIJ" became "tiếng ABCDtiếng".
    do {
        let harness = ControllerHarness()
        harness.type("tieengs ")
        harness.client.applicationInserts("ABCDEFGHIJ")
        check("MAC-01 document before Backspace", harness.client.text, "tiếng ABCDEFGHIJ")
        let handled = harness.sendWithoutApplication(.backspace)
        check("MAC-01 unverifiable reconversion is declined", handled, false)
        check(
            "MAC-01 caret moved by an unseen edit — document untouched",
            harness.client.text, "tiếng ABCDEFGHIJ")
        // Declining is only correct if the key still does its job.
        HarnessKey.backspace.applyToClient(harness.client)
        check("MAC-01 the Backspace still reaches the client",
              harness.client.text, "tiếng ABCDEFGHI")
    }

    // MAC-01, the arrow-key half: arrows are classified as nil and returned
    // unhandled, and used to leave the recorded commit in place. With the caret
    // one to the left the old controller inserted a second copy of the word:
    // "tiếngtiếng ".
    do {
        let harness = ControllerHarness()
        harness.type("tieengs ")
        check("arrow key is the application's", harness.send(.arrowLeft), false)
        let handled = harness.sendWithoutApplication(.backspace)
        check("MAC-01 reconversion after an arrow key is declined", handled, false)
        check("MAC-01 arrow key — document untouched", harness.client.text, "tiếng ")
    }

    // The caret is back where the commit left it, and the text there really is
    // the committed text — but nothing observed the round trip, so the recorded
    // commit is gone and the reconversion is declined rather than guessed at.
    do {
        let harness = ControllerHarness()
        harness.type("tieengs ")
        harness.send(.arrowLeft)
        harness.send(.arrowRight)
        let handled = harness.sendWithoutApplication(.backspace)
        check("MAC-01 arrow away and back still invalidates", handled, false)
        check("MAC-01 arrow away and back — document untouched", harness.client.text, "tiếng ")
    }

    // MAC-02. A client with no TSMDocumentAccess answers NSNotFound for
    // selectedRange and ignores replacement ranges, so the reopened word cannot
    // substitute for the committed one. The old controller marked it anyway and
    // the client appended it: "tiếng tiếng".
    do {
        let harness = ControllerHarness(documentAccess: false)
        harness.type("tieengs ")
        check("MAC-02 document before Backspace", harness.client.text, "tiếng ")
        let handled = harness.sendWithoutApplication(.backspace)
        check("MAC-02 reconversion without document access is declined", handled, false)
        check("MAC-02 no duplicate is inserted", harness.client.text, "tiếng ")
        HarnessKey.backspace.applyToClient(harness.client)
        check("MAC-02 the Backspace still reaches the client", harness.client.text, "tiếng")
    }

    // A document that no longer holds the committed text at the caret, with the
    // caret exactly where it was left: length arithmetic alone cannot tell this
    // apart from the safe case, only reading the range back can.
    do {
        let harness = ControllerHarness()
        harness.type("tieengs ")
        harness.client.applicationDeletesBackward()  // the space
        harness.client.applicationInserts("!")  // same length, different text
        let handled = harness.sendWithoutApplication(.backspace)
        check("MAC-01 rewritten range is declined", handled, false)
        check("MAC-01 rewritten range — document untouched", harness.client.text, "tiếng!")
    }

    // Mid-word Backspace is not a reconversion and must be unaffected: the core
    // pops the raw keystroke, so dropping the Telex tone key re-renders "tiêng".
    do {
        let harness = ControllerHarness()
        harness.type("tieengs")
        check("mid-word backspace is handled", harness.send(.backspace), true)
        check("mid-word backspace", harness.client.text, "tiêng")
    }

    // Holding Backspace past the reconversion must drain the document rather
    // than stall or resurrect text.
    do {
        let harness = ControllerHarness()
        harness.type("tieengs vieejt ")
        for _ in 0..<40 { harness.send(.backspace) }
        check("hold backspace drains the document", harness.client.text, "")
    }

    // MAC-03. AppKit delivers F1–F20, Help, Clear and Menu in `characters` as
    // private-use scalars, which passed the "is it a control character" filter
    // and were classified as printable text. Every one of them was swallowed, in
    // every application, whether or not anything was composing: with HC_IME
    // selected the whole F-row stopped working.
    do {
        let harness = ControllerHarness()
        let keys: [(String, HarnessKey)] = [
            ("F1", .functionKey(0xF704, keyCode: 122)),
            ("F5", .functionKey(0xF708, keyCode: 96)),
            ("F11", .functionKey(0xF70E, keyCode: 103)),
            ("F12", .functionKey(0xF70F, keyCode: 111)),
            ("Help", .functionKey(0xF746, keyCode: 114)),
            ("Clear", .functionKey(0xF739, keyCode: 71)),
            ("Menu", .functionKey(0xF735, keyCode: 110)),
        ]
        for (name, key) in keys {
            check(
                "MAC-03 \(name) with nothing composing is the application's",
                harness.sendWithoutApplication(key), false)
        }
        check("MAC-03 no function key reached the document", harness.client.text, "")
    }
    do {
        let harness = ControllerHarness()
        harness.type("tieengs")
        check(
            "MAC-03 F12 during a composition is still the application's",
            harness.sendWithoutApplication(.functionKey(0xF70F, keyCode: 111)), false)
        check("MAC-03 the composition is committed before the app sees it",
              harness.client.text, "tiếng")
    }

    // The classifier is the one the tester window uses too, so these cover both
    // frontends at once — the two used to hold a copy each and were wrong in the
    // same two ways.
    do {
        func classified(_ key: HarnessKey) -> HCKeyAction { HCKeyClassifier.classify(key.event) }
        check("classifier: Delete is a Backspace",
              classified(.backspace) == .engine(.backspace, nil), true)
        check("classifier: fn+Delete is not a Backspace",
              classified(.forwardDelete) == .discardComposition, true)
        check("classifier: F1 is not text", classified(.functionKey(0xF704, keyCode: 122))
            == .application, true)
        check("classifier: a letter is text",
              classified(.character("a")) == .engine(.printable, "a"), true)
        check("classifier: Space is a space", classified(.character(" ")) == .engine(.space, " "),
              true)
        check("classifier: Tab steps forward", classified(.tab) == .navigate(.next), true)
        check("classifier: Shift+Tab steps back",
              classified(.raw(characters: "\t", keyCode: 48, flags: [.shift]))
                  == .navigate(.previous), true)
        check("classifier: Home is the application's",
              classified(.functionKey(0xF729, keyCode: 115)) == .application, true)
    }

    // MAC-04. The preedit renders the *Vietnamese* reading of the keystrokes —
    // "test" shows as "tét" — and committing that verbatim wrote "tét" into the
    // document on focus loss, on Tab, and on any Cmd/Ctrl shortcut. The core
    // resolves it to "test" when the word is ended; ask it, as the addon does.
    do {
        let harness = ControllerHarness()
        harness.type("test")
        check("MAC-04 the preedit really is the Vietnamese reading",
              harness.client.text, "tét")
        harness.controller.commitComposition(harness.client)
        check("MAC-04 commitComposition commits the resolved word",
              harness.client.text, "test")
    }
    do {
        let harness = ControllerHarness()
        harness.type("test")
        harness.controller.deactivateServer(harness.client)
        check("MAC-04 focus loss commits the resolved word", harness.client.text, "test")
    }
    do {
        let harness = ControllerHarness()
        harness.type("test")
        check("MAC-04 Tab is the application's", harness.sendWithoutApplication(.tab), false)
        check("MAC-04 Tab commits the resolved word", harness.client.text, "test")
    }
    do {
        let harness = ControllerHarness()
        harness.type("test")
        check(
            "MAC-04 Cmd+C is the application's",
            harness.sendWithoutApplication(.command("c", keyCode: 8)), false)
        check("MAC-04 Cmd+C commits the resolved word", harness.client.text, "test")
    }
    do {
        // The same path must not "resolve" a genuine Vietnamese word into
        // anything else.
        let harness = ControllerHarness()
        harness.type("tieengs")
        harness.controller.commitComposition(harness.client)
        check("MAC-04 a Vietnamese word commits as Vietnamese", harness.client.text, "tiếng")
    }

    // MAC-05. IMK builds one controller per client application, lazily, so a
    // controller created after the user picked VNI never hears about it —
    // `setValue(forTag:)` is the only correction path and it does not fire.
    // Every newly focused app started in Telex and rendered VNI keystrokes
    // literally.
    do {
        let harness = ControllerHarness(mode: nil, inputSourceID: "dev.hcime.inputmethod.VNI")
        harness.type("tie61ng ")
        check("MAC-05 a controller built under VNI starts in VNI",
              harness.client.text, "tiếng ")
        harness.teardown()
    }
    do {
        let harness = ControllerHarness(
            mode: nil, inputSourceID: "dev.hcime.inputmethod.HanNomVIQR")
        check("MAC-05 Hán Nôm VIQR resolves too", harness.controller.testingMode == .hanNomViqr, true)
        harness.teardown()
    }
    do {
        // The mode can also change while this controller exists — another app's
        // controller is the one that gets `setValue(forTag:)`. Activation is
        // when this one can still find out.
        let harness = ControllerHarness()
        HCIMEInputController.currentInputSourceID = { "dev.hcime.inputmethod.VNI" }
        harness.controller.activateServer(harness.client)
        harness.controller.configureForTesting(mode: nil, settings: HCSettings())
        harness.type("tie61ng ")
        check("MAC-05 activateServer re-reads the input source",
              harness.client.text, "tiếng ")
        harness.teardown()
    }
    do {
        // ...but an identifier that is not one of ours must leave the mode
        // alone rather than reset it to Telex.
        let harness = ControllerHarness(mode: .vni)
        HCIMEInputController.currentInputSourceID = { "com.apple.keylayout.US" }
        harness.controller.activateServer(harness.client)
        harness.controller.configureForTesting(mode: nil, settings: HCSettings())
        harness.type("tie61ng ")
        check("MAC-05 an unrecognised input source does not reset the mode",
              harness.client.text, "tiếng ")
        harness.teardown()
    }

    // MAC-06. Forward Delete was mapped onto Backspace, so fn+Delete ate a
    // keystroke off the end of the preedit, and after a commit it reopened the
    // committed word — deleting *backwards* with a key that deletes forwards.
    do {
        let harness = ControllerHarness()
        harness.type("tieengs")
        check("MAC-06 Forward Delete with a composition is swallowed",
              harness.sendWithoutApplication(.forwardDelete), true)
        check("MAC-06 Forward Delete discards the composition, not one keystroke",
              harness.client.text, "")
    }
    do {
        let harness = ControllerHarness()
        harness.type("tieengs ")
        check("MAC-06 Forward Delete after a commit is the application's",
              harness.sendWithoutApplication(.forwardDelete), false)
        check("MAC-06 Forward Delete does not reopen the last commit",
              harness.client.text, "tiếng ")
    }
    do {
        // Caret before "abc": the application deletes the character after it.
        // The input method must not touch the document at all.
        let harness = ControllerHarness()
        harness.client.applicationInserts("abc")
        harness.client.applicationMovesCaret(by: -3)
        check("MAC-06 Forward Delete with nothing composing is the application's",
              harness.sendWithoutApplication(.forwardDelete), false)
        check("MAC-06 the document is untouched by the input method",
              harness.client.text, "abc")
    }

    // MAC-07. Option is macOS's Alt, which the addon counts as a command
    // modifier. Without it, Option-typed characters were fed to the composition
    // engine and every Option+letter shortcut was consumed — and
    // Option+Backspace, the system-wide delete-previous-word, reopened the last
    // commit instead.
    do {
        let harness = ControllerHarness()
        check("MAC-07 Option+A is the application's",
              harness.sendWithoutApplication(.option("å", keyCode: 0)), false)
        check("MAC-07 Option+A never enters the Vietnamese buffer",
              harness.client.text, "")
    }
    do {
        let harness = ControllerHarness()
        harness.type("tieengs ")
        check("MAC-07 Option+Backspace is the application's",
              harness.sendWithoutApplication(.optionBackspace), false)
        check("MAC-07 Option+Backspace does not reconvert", harness.client.text, "tiếng ")
    }
    do {
        // With a composition in flight the addon commits it and forwards the key
        // (`commitAndForwardKey`), so the word is not lost.
        let harness = ControllerHarness()
        harness.type("test")
        check("MAC-07 Option+Backspace during a composition is the application's",
              harness.sendWithoutApplication(.optionBackspace), false)
        check("MAC-07 the composition is committed, resolved, first",
              harness.client.text, "test")
    }

    // MAC-09. A printable the core declines used to return false with the
    // controller still believing its text was marked, while the client had
    // already finalised it — so the next keystroke marked the word a second
    // time. Hán Nôm Telex, 5 candidates for "hai", user presses 9.
    do {
        let harness = ControllerHarness(mode: .hanNomTelex)
        harness.type("hai")
        check("MAC-09 the Hán Nôm reading is composing", harness.client.text, "hai")
        check("MAC-09 an out-of-range digit is the application's",
              harness.send(.character("9")), false)
        check("MAC-09 the composition is finalised before the digit lands",
              harness.client.text, "hai9")
        harness.send(.character("s"))
        check("MAC-09 the next keystroke does not duplicate the word",
              harness.client.text, "hai9s")
    }

    // MAC-10. Enter armed a reconversion whose "delimiter" was a newline this
    // process never wrote and cannot see. In a send-on-enter app (Slack,
    // Discord, Messages) the field is emptied instead; in an ordinary one the
    // Backspace ate the newline and put the sent word back as a preedit. The
    // addon arms only on Space and boundary characters.
    do {
        let harness = ControllerHarness()
        harness.type("tieengs")
        harness.send(.enter)
        check("MAC-10 Enter commits and the application writes the newline",
              harness.client.text, "tiếng\n")
        check("MAC-10 Backspace after Enter is the application's",
              harness.sendWithoutApplication(.backspace), false)
        check("MAC-10 the committed word is not resurrected", harness.client.text, "tiếng\n")
        HarnessKey.backspace.applyToClient(harness.client)
        check("MAC-10 the Backspace still reaches the client", harness.client.text, "tiếng")
    }
    do {
        // The send-on-enter case itself: the document the controller thought it
        // could prove is gone entirely.
        let harness = ControllerHarness()
        harness.type("tieengs")
        harness.send(.enter)
        harness.client.clearDocumentLikeSendOnEnter()
        check("MAC-10 Backspace in a send-on-enter app is declined",
              harness.sendWithoutApplication(.backspace), false)
        check("MAC-10 the compose box stays empty", harness.client.text, "")
    }

    // MAC-11. The candidate panel was created, shown and hidden but never sent
    // a key event: arrows fell through to the application and moved the caret
    // under a live preedit, and Hán Nôm VNI — where digits are tone triggers and
    // cannot select — had no keyboard selection at all.
    //
    // What is tested here: that the controller routes each navigation key to the
    // panel, consumes it rather than leaking it to the application, and commits
    // the row the panel says is highlighted. What is NOT tested: `IMKCandidates`
    // itself — its interpretation of the synthesized events, its drawing, its
    // paging and its selection keys. `IMKCandidates(server:)` is nil without an
    // IMKServer, so none of that can run in this process.
    do {
        let panel = FakeCandidatePanel()
        let harness = ControllerHarness(mode: .hanNomTelex, panel: panel)
        harness.type("hai")
        check("MAC-11 the panel is shown for Hán Nôm candidates", panel.isPanelVisible, true)
        check("MAC-11 the panel was given the controller's candidates", panel.rows > 1, true)

        check("MAC-11 Down is taken by the panel", harness.sendWithoutApplication(.arrowDown), true)
        check("MAC-11 Down highlights the first candidate", panel.highlightedRow == 0, true)
        check("MAC-11 Tab moves forward too", harness.sendWithoutApplication(.tab), true)
        check("MAC-11 Right moves forward too", harness.sendWithoutApplication(.arrowRight), true)
        check("MAC-11 three forward moves land on row 2", panel.highlightedRow == 2, true)
        check("MAC-11 Up moves back", harness.sendWithoutApplication(.arrowUp), true)
        check("MAC-11 Left moves back", harness.sendWithoutApplication(.arrowLeft), true)
        check("MAC-11 two moves back land on row 0", panel.highlightedRow == 0, true)
        check("MAC-11 Page Down is taken by the panel",
              harness.sendWithoutApplication(.pageDown), true)
        check("MAC-11 Page Up is taken by the panel", harness.sendWithoutApplication(.pageUp), true)
        check("MAC-11 the moves reached the panel in order",
              panel.moves == [.next, .next, .next, .previous, .previous, .nextPage, .previousPage],
              true)
        check("MAC-11 navigating never touches the document", harness.client.text, "hai")

        // Enter takes the highlighted row, not the reading.
        harness.sendWithoutApplication(.arrowDown)
        harness.sendWithoutApplication(.arrowDown)
        check("MAC-11 the panel is highlighting row 2", panel.highlightedRow == 2, true)
        let expected = expectedHanNomCandidate(mode: .hanNomTelex, keys: "hai", index: 2)
        check("MAC-11 Enter is taken by the panel", harness.sendWithoutApplication(.enter), true)
        check("MAC-11 Enter commits the highlighted candidate", harness.client.text, expected)
        check("MAC-11 the panel is dismissed after selection", panel.isPanelVisible, false)
        harness.teardown()
    }
    do {
        // Digits select candidates in Telex and VIQR, so the panel is given the
        // digit selection keys...
        let panel = FakeCandidatePanel()
        let harness = ControllerHarness(mode: .hanNomTelex, panel: panel)
        harness.type("hai")
        check("MAC-11 Telex gets digit selection keys",
              panel.selectionKeys == HCIMEApplication.digitSelectionKeyCodes, true)
        harness.teardown()
    }
    do {
        // ...and in Hán Nôm VNI they are tone and shape triggers, so the panel
        // is given none and the arrows are the only way to select.
        let panel = FakeCandidatePanel()
        let harness = ControllerHarness(mode: .hanNomVni, panel: panel)
        harness.type("hai")
        check("MAC-11 VNI gets no digit selection keys", panel.selectionKeys.isEmpty, true)
        check("MAC-11 VNI still shows the panel", panel.isPanelVisible, true)
        check("MAC-11 VNI can select with the arrows",
              harness.sendWithoutApplication(.arrowDown), true)
        let expected = expectedHanNomCandidate(mode: .hanNomVni, keys: "hai", index: 0)
        harness.sendWithoutApplication(.enter)
        check("MAC-11 VNI Enter commits the highlighted candidate",
              harness.client.text, expected)
        harness.teardown()
    }
    do {
        // With no panel up, navigation keys stay the application's — the panel
        // must never become an excuse to swallow an arrow key.
        let panel = FakeCandidatePanel()
        let harness = ControllerHarness(mode: .telex, panel: panel)
        harness.type("tieengs")
        check("MAC-11 no panel in Vietnamese mode", panel.isPanelVisible, false)
        check("MAC-11 the arrow is still the application's",
              harness.sendWithoutApplication(.arrowDown), false)
        check("MAC-11 and the composition is committed first",
              harness.client.text, "tiếng")
        harness.teardown()
    }

    return failures
}

/// The candidate the engine ranks at `index` for `keys`, from an independent
/// session. Lets a test assert *which* candidate was committed without pinning a
/// glyph the dictionary may reorder — local ranking makes the order a function
/// of the machine, so the expectation has to come from the same engine.
/// Learning is off here for the same reason it is off in the controller under
/// test: the suite must not rewrite the user's history file.
private func expectedHanNomCandidate(mode: HCInputMode, keys: String, index: Int) -> String {
    let session = HCSession(mode: mode, legacyTone: false)
    session.configureHanNom(learning: false)
    let settings = HCSettings()
    for character in keys {
        _ = session.handleKey(kind: .printable, text: String(character), settings: settings)
    }
    return session.selectHanNomCandidate(absoluteIndex: index).text
}
