import Cocoa

// A normal macOS app for exercising the engine: launch it, pick a mode, type.
// No input-method install, no logout, no Accessibility permission.
//
// It drives the same HCSession wrapper and the same key classification as the
// real IMK controller, so behaviour here matches what the installed input
// method does — the only difference is that text lands in this window instead
// of an arbitrary app.

// MARK: - Text view

/// An NSTextView that routes keystrokes through hc_core before they reach the
/// document. It maintains the committed and composing regions itself so the
/// preedit is always visible and cannot be swallowed by NSTextView's
/// marked-text machinery; the engine calls are identical to the IMK path.
final class HCTextView: NSTextView {

    var session: HCSession!
    var settings = HCSettings()
    var onStateChange: (([HCCandidate], String) -> Void)?

    // Nothing in the TextKit ownership chain retains the text storage, so the
    // view must hold it or the whole stack is torn down underneath us.
    private var retainedStorage: NSTextStorage?

    private var candidates: [HCCandidate] = []

    // The tester owns its document outright rather than going through
    // NSTextView's marked-text machinery: it makes the composing region
    // explicit and cannot silently render nothing.
    private var committed = ""
    private var preedit = ""

    // What the last commit put into the document, so a reconversion can take
    // it back out. The core hands back the reopened word expecting the
    // frontend to have removed the committed copy first.
    private var lastCommitted = ""
    private var lastDelimiter = ""

    // The designated initializer must be implemented: NSTextView.init(frame:)
    // funnels into it, and a subclass that omits it traps at runtime.
    override init(frame frameRect: NSRect, textContainer container: NSTextContainer?) {
        super.init(frame: frameRect, textContainer: container)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    /// Builds the text storage / layout manager / container network by hand.
    /// Passing a nil container leaves `textStorage` nil, which makes rendering
    /// silently impossible while key handling still appears to work.
    convenience init(session: HCSession) {
        let storage = NSTextStorage()
        let layoutManager = NSLayoutManager()
        storage.addLayoutManager(layoutManager)
        let container = NSTextContainer(
            size: NSSize(width: 660, height: CGFloat.greatestFiniteMagnitude))
        container.widthTracksTextView = true
        layoutManager.addTextContainer(container)

        self.init(frame: NSRect(x: 0, y: 0, width: 660, height: 320), textContainer: container)
        self.session = session
        self.retainedStorage = storage
    }

    func setMode(_ mode: HCInputMode) {
        flushComposition()
        session.setMode(mode, legacyTone: settings.legacyTone)
        candidates = []
        // Hán Nôm commits CJK into the document; Vietnamese does not and reads
        // better in the system font.
        font = mode.isHanNom ? HanNomFont.font(ofSize: 22) : NSFont.systemFont(ofSize: 22)
        render()
        report(status: "mode: \(mode.displayName)")
    }

    override func keyDown(with event: NSEvent) {
        if HCKeyClassifier.hasCommandModifier(event.modifierFlags) {
            flushComposition()
            super.keyDown(with: event)
            return
        }

        // Navigation keys are ignored rather than forwarded: this view repaints
        // the whole document from `committed`/`preedit`, so letting NSTextView
        // move the insertion point or edit storage behind our back desyncs the
        // model and the next repaint silently reverts it. There is no candidate
        // panel here either — the strip under the text area is not one — so
        // panel navigation has nothing to steer.
        let kind: HCKeyKind
        let text: String?
        switch HCKeyClassifier.classify(event) {
        case .engine(let engineKind, let engineText):
            kind = engineKind
            text = engineText
        case .discardComposition:
            // Forward Delete drops the composition instead of eating a
            // keystroke off the end of it (MAC-06).
            guard !preedit.isEmpty else { return }
            preedit = ""
            session.reset()
            candidates = []
            render()
            report(status: "composition discarded")
            return
        case .navigate, .application:
            return
        }

        if !process(kind: kind, text: text) {
            fallbackEdit(kind: kind, text: text)
        }
    }

    /// Applies a key the engine declined. Previously this called
    /// `super.keyDown`, which edited the text storage directly while
    /// `committed` stayed unchanged — so holding backspace deleted the preedit,
    /// then appeared to stop, because every subsequent repaint restored the
    /// characters NSTextView had removed.
    private func fallbackEdit(kind: HCKeyKind, text: String?) {
        switch kind {
        case .backspace:
            if !preedit.isEmpty {
                preedit.removeLast()
            } else if !committed.isEmpty {
                committed.removeLast()
            } else {
                return
            }
            lastCommitted = ""
            lastDelimiter = ""
        case .enter:
            committed += "\n"
        case .escape:
            preedit = ""
        case .printable, .space, .boundary:
            guard let text else { return }
            committed += preedit + text
            preedit = ""
        case .other, .undo:
            return
        }
        render()
        report(status: "client edit — \(String(describing: kind))")
    }

    /// Runs one classified key through the engine and repaints.
    /// Returns false when the engine declined the key.
    @discardableResult
    func process(kind: HCKeyKind, text: String?) -> Bool {
        // Hán Nôm Telex/VIQR digit selection is the frontend's job; the core
        // returns those keys unhandled on purpose.
        if session.mode.isHanNom, session.mode != .hanNomVni, !candidates.isEmpty,
           let digit = text?.first, digit.isNumber, digit != "0",
           let position = digit.wholeNumberValue, position - 1 < candidates.count
        {
            let selection = session.selectHanNomCandidate(absoluteIndex: position - 1)
            if selection.handled {
                commit(selection.text)
                candidates = []
                render()
                report(status: "selected candidate \(position)")
                return true
            }
        }

        let result = session.handleKey(kind: kind, text: text, settings: settings)
        guard result.handled else {
            flushComposition()
            return false
        }

        if result.errorCode < 0 {
            preedit = ""
            candidates = []
            render()
            report(status: "error \(result.errorCode)")
            return true
        }

        switch result.status {
        case .inProgress, .reconversionActive:
            if result.status == .reconversionActive { withdrawLastCommit() }
            preedit = result.text
            candidates = result.candidates
            render()
            report(status: "composing — \(statusName(result.status))")

        case .commit, .englishFallback:
            commit(result.text)
            candidates = []
            lastCommitted = result.text
            lastDelimiter = ""
            // Space / punctuation are delimiters the document still needs.
            if kind == .space || kind == .boundary, let text {
                committed += text
                lastDelimiter = text
            } else if kind == .enter {
                committed += "\n"
                lastDelimiter = "\n"
            }
            render()
            report(status: "committed — \(statusName(result.status))")

        case .escRestoredRaw:
            commit(result.text)
            candidates = []
            render()
            report(status: "restored raw keystrokes")
        }
        return true
    }

    private func statusName(_ status: HCStatus) -> String {
        switch status {
        case .inProgress: return "in progress"
        case .commit: return "commit"
        case .englishFallback: return "English fallback"
        case .reconversionActive: return "reconversion"
        case .escRestoredRaw: return "esc restored raw"
        }
    }

    private func commit(_ text: String) {
        preedit = ""
        committed += text
    }

    /// Removes the text the previous commit wrote, so a reopened word replaces
    /// it instead of being appended alongside it.
    private func withdrawLastCommit() {
        let written = lastCommitted + lastDelimiter
        guard !written.isEmpty, committed.hasSuffix(written) else { return }
        committed.removeLast(written.count)
        lastCommitted = ""
        lastDelimiter = ""
    }

    /// Ends composition, keeping whatever was in the preedit.
    private func flushComposition() {
        committed += preedit
        preedit = ""
        session.reset()
        candidates = []
        render()
        report(status: "idle")
    }

    /// Repaints the document: committed text plain, the composing region
    /// underlined and tinted. Colours come from the semantic palette so the
    /// window is readable in both light and dark appearance.
    private func render() {
        guard let storage = textStorage else {
            report(status: "ERROR: textStorage is nil — TextKit stack missing")
            return
        }
        let bodyFont = font ?? NSFont.systemFont(ofSize: 22)
        let output = NSMutableAttributedString(
            string: committed,
            attributes: [.font: bodyFont, .foregroundColor: NSColor.labelColor])
        if !preedit.isEmpty {
            output.append(
                NSAttributedString(
                    string: preedit,
                    attributes: [
                        .font: bodyFont,
                        .foregroundColor: NSColor.controlAccentColor,
                        .underlineStyle: NSUnderlineStyle.single.rawValue,
                        .underlineColor: NSColor.controlAccentColor,
                    ]))
        }
        storage.setAttributedString(output)
        let end = NSRange(location: output.length, length: 0)
        setSelectedRange(end)
        scrollRangeToVisible(end)
    }

    /// Clears the document. Used by the Clear button.
    func clearAll() {
        committed = ""
        preedit = ""
        lastCommitted = ""
        lastDelimiter = ""
        session.reset()
        candidates = []
        render()
        report(status: "cleared")
    }

    /// The visible document, for the headless self-test.
    var renderedText: String { textStorage?.string ?? "<no text storage>" }

    /// Exposes the declined-key path so the self-test drives the same sequence
    /// `keyDown` does.
    func fallbackEditForTesting(kind: HCKeyKind, text: String?) {
        fallbackEdit(kind: kind, text: text)
    }

    private func report(status: String) {
        onStateChange?(candidates, status)
    }
}

// MARK: - Window

final class TesterWindowController: NSObject, NSWindowDelegate {

    private let window: NSWindow
    private let textView: HCTextView
    private let candidateLabel = NSTextField(labelWithString: "")
    private let statusLabel = NSTextField(labelWithString: "idle")
    private let fontWarningLabel = NSTextField(labelWithString: "")
    private let installFontButton = NSButton()
    private let modePopup = NSPopUpButton()

    private let modes: [HCInputMode] = [
        .telex, .vni, .viqr, .hanNomTelex, .hanNomVni, .hanNomViqr,
    ]

    override init() {
        let session = HCSession(mode: .telex, legacyTone: false)
        session.configureHanNom()
        textView = HCTextView(session: session)

        window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 720, height: 460),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered, defer: false)
        super.init()

        window.title = "HC_IME Tester"
        window.delegate = self
        window.center()
        buildLayout()

        textView.onStateChange = { [weak self] candidates, status in
            self?.update(candidates: candidates, status: status)
        }
    }

    private func buildLayout() {
        let content = NSView()

        modePopup.addItems(withTitles: modes.map(\.displayName))
        modePopup.target = self
        modePopup.action = #selector(modeChanged)

        let toggles = NSStackView(views: [
            checkbox("Spell check", #selector(toggleSpellCheck), on: true),
            checkbox("Auto-restore", #selector(toggleAutoRestore), on: true),
            checkbox("Quick consonants", #selector(toggleQuickConsonants), on: false),
            checkbox("ESC restores raw", #selector(toggleEscRestoreRaw), on: false),
        ])
        toggles.orientation = .horizontal
        toggles.spacing = 14

        let clearButton = NSButton(title: "Clear", target: self, action: #selector(clearText))
        installFontButton.title = "Install Hán Nôm font"
        installFontButton.target = self
        installFontButton.action = #selector(installNomFont)
        installFontButton.isHidden = HanNomFont.coverageWarning() == nil

        let controls = NSStackView(views: [modePopup, toggles, clearButton, installFontButton])
        controls.orientation = .horizontal
        controls.spacing = 18

        // NSTextView built with a zero frame lays text into a zero-width
        // container and renders nothing. Give it a real size and let the
        // container track the view's width.
        let initialSize = NSSize(width: 660, height: 320)
        textView.frame = NSRect(origin: .zero, size: initialSize)
        textView.minSize = NSSize(width: 0, height: 0)
        textView.maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude,
                                  height: CGFloat.greatestFiniteMagnitude)
        textView.isVerticallyResizable = true
        textView.isHorizontallyResizable = false
        textView.autoresizingMask = [.width]
        textView.textContainerInset = NSSize(width: 6, height: 8)
        textView.textContainer?.containerSize =
            NSSize(width: initialSize.width, height: CGFloat.greatestFiniteMagnitude)
        textView.textContainer?.widthTracksTextView = true

        // Vietnamese modes keep the system font: the Nôm cascade's base family
        // may not cover toned Latin vowels (Songti SC lacks ế and ộ), which
        // renders the preedit mixed-face.
        textView.font = NSFont.systemFont(ofSize: 22)
        textView.textColor = .labelColor
        textView.isRichText = false
        textView.isAutomaticQuoteSubstitutionEnabled = false
        textView.isAutomaticTextReplacementEnabled = false
        textView.isAutomaticSpellingCorrectionEnabled = false
        textView.isAutomaticDashSubstitutionEnabled = false

        let scroll = NSScrollView()
        scroll.hasVerticalScroller = true
        scroll.borderType = .bezelBorder
        scroll.documentView = textView

        // Cascade over installed Hán Nôm fonts rather than the system font,
        // which covers none of the extended planes Nôm needs.
        candidateLabel.font = HanNomFont.font(ofSize: 22)
        candidateLabel.textColor = .secondaryLabelColor
        candidateLabel.stringValue = "Hán Nôm candidates appear here — press 1–9 to select"

        fontWarningLabel.font = NSFont.systemFont(ofSize: 11)
        fontWarningLabel.textColor = .systemOrange
        fontWarningLabel.stringValue = HanNomFont.coverageWarning() ?? ""
        fontWarningLabel.isHidden = HanNomFont.coverageWarning() == nil

        statusLabel.font = NSFont.monospacedSystemFont(ofSize: 11, weight: .regular)
        statusLabel.textColor = .tertiaryLabelColor

        let stack = NSStackView(views: [
            controls, scroll, candidateLabel, fontWarningLabel, statusLabel,
        ])
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 10
        stack.edgeInsets = NSEdgeInsets(top: 16, left: 16, bottom: 16, right: 16)
        stack.translatesAutoresizingMaskIntoConstraints = false
        content.addSubview(stack)

        NSLayoutConstraint.activate([
            stack.topAnchor.constraint(equalTo: content.topAnchor),
            stack.leadingAnchor.constraint(equalTo: content.leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: content.trailingAnchor),
            stack.bottomAnchor.constraint(equalTo: content.bottomAnchor),
            scroll.widthAnchor.constraint(equalTo: stack.widthAnchor, constant: -32),
            scroll.heightAnchor.constraint(greaterThanOrEqualToConstant: 300),
        ])

        window.contentView = content
    }

    private func checkbox(_ title: String, _ action: Selector, on: Bool) -> NSButton {
        let button = NSButton(checkboxWithTitle: title, target: self, action: action)
        button.state = on ? .on : .off
        return button
    }

    func show() {
        window.makeKeyAndOrderFront(nil)
        window.makeFirstResponder(textView)
        NSApp.activate(ignoringOtherApps: true)
    }

    private func update(candidates: [HCCandidate], status: String) {
        let candidateFont = HanNomFont.font(ofSize: 22)

        if candidates.isEmpty {
            candidateLabel.attributedStringValue = NSAttributedString(
                string: textView.session.mode.isHanNom ? "no candidates" : "",
                attributes: [.font: candidateFont, .foregroundColor: NSColor.secondaryLabelColor])
        } else {
            let line = NSMutableAttributedString()
            for (offset, candidate) in candidates.prefix(9).enumerated() {
                let drawable = HanNomFont.canDisplay(candidate.text, using: candidateFont)
                line.append(
                    NSAttributedString(
                        string: "\(offset + 1).",
                        attributes: [
                            .font: NSFont.systemFont(ofSize: 13),
                            .foregroundColor: NSColor.tertiaryLabelColor,
                        ]))
                line.append(
                    NSAttributedString(
                        string: candidate.text,
                        attributes: [
                            .font: candidateFont,
                            .foregroundColor: drawable
                                ? NSColor.labelColor : NSColor.systemOrange,
                        ]))
                // A box with no explanation is indistinguishable from a bug.
                // Name the codepoint so the glyph is still identifiable.
                if !drawable {
                    line.append(
                        NSAttributedString(
                            string: "(\(HanNomFont.codepoints(of: candidate.text)))",
                            attributes: [
                                .font: NSFont.monospacedSystemFont(ofSize: 10, weight: .regular),
                                .foregroundColor: NSColor.systemOrange,
                            ]))
                }
                line.append(NSAttributedString(string: "   "))
            }
            candidateLabel.attributedStringValue = line
        }

        let undrawable = candidates.filter {
            !HanNomFont.canDisplay($0.text, using: candidateFont)
        }.count
        let generated = candidates.filter { $0.kind == 2 }.count

        var parts = ["\(candidates.count) candidate(s)"]
        if generated > 0 { parts.append("\(generated) generated") }
        if undrawable > 0 { parts.append("\(undrawable) missing glyphs") }
        statusLabel.stringValue = "\(status)   ·   " + parts.joined(separator: "  ·  ")
    }

    @objc private func modeChanged() {
        textView.setMode(modes[modePopup.indexOfSelectedItem])
        window.makeFirstResponder(textView)
    }

    @objc private func clearText() {
        textView.clearAll()
        window.makeFirstResponder(textView)
    }

    /// Fetches the optional Hán Nôm font from Apple's font catalog. This is the
    /// only thing that actually removes the boxes — the rest of the font code
    /// only reports the problem.
    @objc private func installNomFont() {
        installFontButton.isEnabled = false
        installFontButton.title = "Downloading…"
        HanNomFont.downloadNomFont { [weak self] percent in
            self?.installFontButton.title = "Downloading \(Int(percent))%"
        } completion: { [weak self] installed in
            guard let self else { return }
            self.installFontButton.isEnabled = true
            self.installFontButton.title = installed ? "Font installed" : "Install Hán Nôm font"
            self.installFontButton.isHidden = installed
            let warning = HanNomFont.coverageWarning()
            self.fontWarningLabel.stringValue = warning ?? ""
            self.fontWarningLabel.isHidden = warning == nil
            self.textView.setMode(self.modes[self.modePopup.indexOfSelectedItem])
        }
    }

    @objc private func toggleSpellCheck(_ sender: NSButton) {
        textView.settings.spellCheck = sender.state == .on
    }
    @objc private func toggleAutoRestore(_ sender: NSButton) {
        textView.settings.autoRestore = sender.state == .on
    }
    @objc private func toggleQuickConsonants(_ sender: NSButton) {
        textView.settings.quickConsonants = sender.state == .on
    }
    @objc private func toggleEscRestoreRaw(_ sender: NSButton) {
        textView.settings.escRestoreRaw = sender.state == .on
    }

    func windowWillClose(_ notification: Notification) {
        NSApp.terminate(nil)
    }
}

// MARK: - Entry point

final class AppDelegate: NSObject, NSApplicationDelegate {
    var controller: TesterWindowController?

    func applicationDidFinishLaunching(_ notification: Notification) {
        controller = TesterWindowController()
        controller?.show()
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
}

/// The kind a typed character would be classified as, taken from the shipping
/// classifier's boundary set rather than a copy of it — the self-test must not
/// be able to disagree with the code it is testing about what a boundary is.
func selfTestKind(_ character: Character) -> HCKeyKind {
    if character == " " { return .space }
    return HCKeyClassifier.boundaryCharacters.contains(character) ? .boundary : .printable
}

// `--selftest` drives the view headlessly and prints what actually landed in
// the text storage. This catches "engine fine, nothing on screen" bugs without
// needing a display.
if CommandLine.arguments.contains("--selftest") {
    let session = HCSession(mode: .telex, legacyTone: false)
    session.configureHanNom()
    let view = HCTextView(session: session)
    view.font = NSFont.systemFont(ofSize: 22)

    guard view.textStorage != nil else {
        print("FAIL: textStorage is nil — the TextKit stack was never built")
        exit(1)
    }
    print("textStorage: present")

    var failures = 0
    func check(_ label: String, _ keys: String, _ expected: String) {
        let session = HCSession(mode: .telex, legacyTone: false)
        let view = HCTextView(session: session)
        view.font = NSFont.systemFont(ofSize: 22)
        for character in keys {
            let text = String(character)
            let kind = selfTestKind(character)
            view.process(kind: kind, text: text)
        }
        let actual = view.renderedText
        let ok = actual == expected
        if !ok { failures += 1 }
        print("[\(ok ? "PASS" : "FAIL")] \(label): \"\(keys)\" rendered \"\(actual)\""
            + (ok ? "" : " (expected \"\(expected)\")"))
    }

    check("preedit visible", "tieengs", "tiếng")
    check("commit on space", "tieengs ", "tiếng ")
    check("d stroke", "ddaay", "đây")
    check("toggle off", "aaa", "aa")
    check("english restore", "test ", "test ")

    // Regression: the core reopens a recent commit as an editable preedit and
    // returns ReconversionActive. Without withdrawing the committed copy first
    // the word was appended to itself — "tiếng tiếng".
    func checkBackspace(_ label: String, _ keys: String, _ expected: String) {
        let session = HCSession(mode: .telex, legacyTone: false)
        let view = HCTextView(session: session)
        view.font = NSFont.systemFont(ofSize: 22)
        for character in keys {
            let text = String(character)
            let kind = selfTestKind(character)
            view.process(kind: kind, text: text)
        }
        view.process(kind: .backspace, text: nil)
        let actual = view.renderedText
        let ok = actual == expected
        if !ok { failures += 1 }
        print("[\(ok ? "PASS" : "FAIL")] \(label): \"\(keys)\" + backspace rendered \"\(actual)\""
            + (ok ? "" : " (expected \"\(expected)\")"))
    }
    checkBackspace("reconversion, no duplicate", "tieengs ", "tiếng")
    // Backspace pops the raw keystroke, not the rendered character: dropping
    // the Telex tone key "s" re-renders "tieeng" as "tiêng".
    checkBackspace("mid-word backspace", "tieengs", "tiêng")

    // Regression: holding backspace deleted the preedit, then appeared to
    // freeze — the view forwarded declined keys to NSTextView, whose edits the
    // next repaint reverted. Backspace must drain the document to empty.
    func checkHoldBackspace(_ label: String, _ mode: HCInputMode, _ keys: String, _ presses: Int) {
        let session = HCSession(mode: mode, legacyTone: false)
        session.configureHanNom()
        let view = HCTextView(session: session)
        view.font = NSFont.systemFont(ofSize: 22)
        for character in keys {
            let text = String(character)
            let kind = selfTestKind(character)
            view.process(kind: kind, text: text)
        }
        let typed = view.renderedText
        let start = Date()
        for _ in 0..<presses {
            if !view.process(kind: .backspace, text: nil) {
                view.fallbackEditForTesting(kind: .backspace, text: nil)
            }
        }
        let elapsed = Date().timeIntervalSince(start) * 1000
        let actual = view.renderedText
        let ok = actual.isEmpty && elapsed < 1000
        if !ok { failures += 1 }
        print("[\(ok ? "PASS" : "FAIL")] \(label): \"\(typed)\" + \(presses)x backspace -> "
            + "\"\(actual)\" in \(String(format: "%.0f", elapsed)) ms"
            + (ok ? "" : "  (expected empty, under 1000 ms)"))
    }
    checkHoldBackspace("hold backspace, Telex", .telex, "tieengs vieejt nam ", 40)
    checkHoldBackspace("hold backspace, VNI", .vni, "tie61ng2 Vie65t nam ", 40)
    checkHoldBackspace("hold backspace, Hán Nôm", .hanNomTelex, "hais", 20)

    // Everything above drives HCTextView, which is a copy of the controller's
    // logic — passing here says nothing about the code the installed input
    // method runs. This drives the shipping HCIMEInputController itself
    // (MAC-08).
    print("\n== HCIMEInputController against a mock IMKTextInput ==")
    failures += runControllerSelfTest()

    print(failures == 0 ? "\nrender path OK" : "\n\(failures) failure(s)")
    exit(failures == 0 ? 0 : 1)
}

let app = NSApplication.shared
app.setActivationPolicy(.regular)
let delegate = AppDelegate()
app.delegate = delegate
app.run()
