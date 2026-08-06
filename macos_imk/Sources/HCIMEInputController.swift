import Carbon
import Cocoa
import InputMethodKit

/// What a key event means to the frontend, before the core sees it.
///
/// The input method and the standalone tester both classify through
/// `HCKeyClassifier`, so the two cannot disagree about what a key is. They used
/// to hold a copy each, and the copies were wrong in the same two ways: every
/// function key passed as printable text (MAC-03) and Forward Delete was mapped
/// onto Backspace (MAC-06).
enum HCKeyAction: Equatable {
    /// The core's key taxonomy — this goes to `hc_session_handle_key_v4`.
    case engine(HCKeyKind, String?)

    /// Forward Delete. Not a Backspace: with a composition in flight it
    /// discards the composition and is swallowed (`hcime.cpp:311-315`), and with
    /// nothing composing it belongs to the application, which deletes *forwards*
    /// — it is in the addon's `isEditingPassthroughKey`
    /// (`hcime_key_handler.cpp:50`).
    case discardComposition

    /// Candidate-panel navigation while a panel is up, and the application's key
    /// otherwise. The addon routes the same set (`hcime.cpp:347-405`).
    case navigate(HCPanelMove)

    /// Never the input method's. Any composition in flight is resolved and
    /// committed first, then the application receives the key.
    case application
}

/// A movement inside the candidate panel.
enum HCPanelMove: Equatable {
    case next, previous, nextPage, previousPage

    /// The arrow or page key this movement means, as AppKit would deliver it.
    /// `IMKCandidates` is driven through `interpretKeyEvents`, which takes
    /// events; synthesizing one from the movement keeps Tab, Right and Down
    /// equivalent instead of leaving each panel type to interpret them.
    var canonicalEvent: NSEvent? {
        let keyCode: UInt16
        let scalar: Unicode.Scalar
        switch self {
        case .next: (keyCode, scalar) = (125, Unicode.Scalar(NSDownArrowFunctionKey)!)
        case .previous: (keyCode, scalar) = (126, Unicode.Scalar(NSUpArrowFunctionKey)!)
        case .nextPage: (keyCode, scalar) = (121, Unicode.Scalar(NSPageDownFunctionKey)!)
        case .previousPage: (keyCode, scalar) = (116, Unicode.Scalar(NSPageUpFunctionKey)!)
        }
        let characters = String(Character(scalar))
        return NSEvent.keyEvent(
            with: .keyDown, location: .zero, modifierFlags: [.function, .numericPad], timestamp: 0,
            windowNumber: 0, context: nil, characters: characters,
            charactersIgnoringModifiers: characters, isARepeat: false, keyCode: keyCode)
    }
}

/// Maps an `NSEvent` onto `HCKeyAction`, mirroring `HcImeKeyHandler::classify`
/// in the Fcitx5 addon so both frontends feed the core the same key kinds.
enum HCKeyClassifier {

    /// Characters that end a word and commit the buffer. Same set as the
    /// addon's `IsBoundaryChar`.
    static let boundaryCharacters: Set<Character> = [
        " ", ".", ",", ";", ":", "!", "?", ")", "]", "}", "/", "\\", "-", "_", "\"", "'",
    ]

    /// AppKit reports F1–F20, Help, Insert, Clear, Menu, Undo/Redo, Find, Break
    /// and ScrollLock in `event.characters` as private-use scalars in
    /// U+F700–U+F8FF. They are not text. `Key::keySymToUTF8` returns nothing for
    /// them on the Fcitx5 side, so they classify as `HC_KEY_OTHER`
    /// (`hcime_key_handler.cpp:70`); letting them through as printable swallowed
    /// every F-key in every application, composing or not (MAC-03).
    static let functionKeyScalars: ClosedRange<UInt32> = 0xF700...0xF8FF

    static func classify(_ event: NSEvent) -> HCKeyAction {
        switch event.keyCode {
        case 51: return .engine(.backspace, nil)  // Delete (backwards)
        case 117: return .discardComposition  // fn+Delete / Forward Delete
        case 36, 76: return .engine(.enter, nil)  // Return, Keypad Enter
        case 53: return .engine(.escape, nil)  // Escape
        // Tab and Shift+Tab step through candidates when a panel is up, exactly
        // as FcitxKey_Tab / FcitxKey_ISO_Left_Tab do (`hcime.cpp:354`, `:363`).
        case 48: return .navigate(event.modifierFlags.contains(.shift) ? .previous : .next)
        case 125, 124: return .navigate(.next)  // Down, Right
        case 126, 123: return .navigate(.previous)  // Up, Left
        case 121: return .navigate(.nextPage)  // Page Down
        case 116: return .navigate(.previousPage)  // Page Up
        case 115, 119: return .application  // Home, End
        default: break
        }

        guard let characters = event.characters, !characters.isEmpty,
              let first = characters.unicodeScalars.first,
              // Control characters, as `Key::keySymToUTF8` filters them.
              first.value >= 0x20, first.value != 0x7F,
              !functionKeyScalars.contains(first.value)
        else { return .application }

        if characters == " " { return .engine(.space, " ") }
        if characters.count == 1, let ch = characters.first, boundaryCharacters.contains(ch) {
            return .engine(.boundary, characters)
        }
        return .engine(.printable, characters)
    }

    /// Modifiers that make a key a shortcut rather than text. The addon's
    /// `HasCommandModifier` counts Alt, Meta, Super and Hyper alongside Ctrl
    /// (`hcime_key_handler.cpp:7-12`); Option is macOS's Alt, and leaving it out
    /// fed every Option-typed character into the Vietnamese buffer and consumed
    /// every Option shortcut (MAC-07).
    static func hasCommandModifier(_ flags: NSEvent.ModifierFlags) -> Bool {
        flags.contains(.command) || flags.contains(.control) || flags.contains(.option)
    }
}

/// InputMethodKit controller for HC_IME.
///
/// Key routing deliberately mirrors `HcImeKeyHandler::classify` in the Fcitx5
/// addon so both frontends feed the core the same key kinds. Where macOS
/// differs from Fcitx5 it is noted inline.
@objc(HCIMEInputController)
final class HCIMEInputController: IMKInputController {

    private var session: HCSession
    private var settings = HCSettings.load()
    private var markedText = ""
    private var candidates: [HCCandidate] = []

    /// A commit that may still be withdrawn from the document, if — and only
    /// if — the document can be shown to still hold it.
    ///
    /// The core reopens a recent commit as an editable preedit
    /// (ReconversionActive) and expects the frontend to have removed the
    /// committed copy first; the Fcitx5 addon does this with
    /// `deleteSurroundingText`. It records the text it wrote rather than just
    /// its length (`ContextState::pendingCommit`, `hcime.cpp:833`) precisely so
    /// that `validatePendingCommit` can prove the document still matches before
    /// anything is deleted. This is the same record for the same reason.
    private struct PendingCommit {
        /// The text `insertText` put into the document.
        let text: String
        /// What the application was left to write after it: the space or
        /// boundary character this controller returned `false` for, the newline
        /// an Enter usually produces, or nothing. This is a prediction about
        /// another process's behaviour, never an observation — which is exactly
        /// why it is checked against the document before it is acted on.
        let delimiter: String

        var written: String { text + delimiter }
    }

    private var pendingCommit: PendingCommit?

    override init!(server: IMKServer!, delegate: Any!, client inputClient: Any!) {
        let mode = HCIMEInputController.modeForCurrentInputSource() ?? .telex
        session = HCSession(mode: mode, legacyTone: settings.legacyTone)
        super.init(server: server, delegate: delegate, client: inputClient)
        session.configureHanNom()
        loadMacros()
    }

    // MARK: - Mode

    /// The identifier of the input source the user is typing with, or nil when
    /// it cannot be read.
    ///
    /// This is a stored closure rather than a direct call so the self-test can
    /// drive mode resolution without a Text Input Sources database; nothing else
    /// reassigns it.
    static var currentInputSourceID: () -> String? = {
        guard let source = TISCopyCurrentKeyboardInputSource()?.takeRetainedValue(),
              let property = TISGetInputSourceProperty(source, kTISPropertyInputSourceID)
        else { return nil }
        return Unmanaged<CFString>.fromOpaque(property).takeUnretainedValue() as String
    }

    /// Resolves the active input mode from the input source the user picked in
    /// the macOS menu bar, or nil when the identifier is not one of ours.
    ///
    /// IMK creates one controller per client application, and it creates them
    /// lazily — the first time the user focuses an app. A controller that
    /// guessed Telex left every newly focused application in Telex no matter
    /// which input source was selected (MAC-05). Returning nil rather than a
    /// guess matters: an identifier we do not recognise must leave the mode
    /// alone instead of resetting a mode `setValue(forTag:)` already set.
    private static func modeForCurrentInputSource() -> HCInputMode? {
        guard let identifier = currentInputSourceID() else { return nil }
        return HCInputMode.from(inputSourceID: identifier)
    }

    /// Switches the engine to `mode`. `HCSession.setMode` builds a new session,
    /// which drops the macro table with it, so the macros are reloaded here.
    private func applyMode(_ mode: HCInputMode) {
        guard mode != session.mode else { return }
        session.setMode(mode, legacyTone: settings.legacyTone)
        loadMacros()
    }

    override func setValue(_ value: Any!, forTag tag: Int, client sender: Any!) {
        // IMK delivers the selected input mode's TISInputSourceID here.
        guard let identifier = value as? String,
              let mode = HCInputMode.from(inputSourceID: identifier)
        else { return }
        clearComposition(commit: false, client: sender)
        applyMode(mode)
    }

    // MARK: - Lifecycle

    /// Shared by the panel and by `displayString`, so coverage detection is
    /// evaluated against the font the panel actually draws with.
    static let candidatePointSize: CGFloat = 20

    override func activateServer(_ sender: Any!) {
        settings = HCSettings.load()
        pendingCommit = nil
        // This controller may have been built while a different input source was
        // active, and it is only ever told about a mode change through
        // `setValue(forTag:)` — which never arrives for a controller that was
        // constructed after the user made their choice. Activation is the moment
        // the mode is knowable, so re-read it (MAC-05).
        if let mode = HCIMEInputController.modeForCurrentInputSource() { applyMode(mode) }
        // Re-scan, then push the result to the panel: refreshing the cache
        // without updating the panel's font attribute leaves it on the family
        // chosen at server startup.
        HanNomFont.refresh()
        HCIMEApplication.candidatesPanel?.setPanelAttributes(HCIMEApplication.panelAttributes())
        session.reset()
        markedText = ""
        candidates = []
    }

    override func deactivateServer(_ sender: Any!) {
        clearComposition(commit: true, client: sender)
        session.flushLearning()
        session.reset()
    }

    override func commitComposition(_ sender: Any!) {
        clearComposition(commit: true, client: sender)
    }

    override func recognizedEvents(_ sender: Any!) -> Int {
        Int(NSEvent.EventTypeMask.keyDown.rawValue)
    }

    // MARK: - Key handling

    override func handle(_ event: NSEvent!, client sender: Any!) -> Bool {
        guard let event, event.type == .keyDown else { return false }

        // Command/Control/Option shortcuts belong to the application. Flush any
        // in-flight composition first so the app sees committed text, then let
        // the shortcut through untouched. Option is included because it is
        // macOS's Alt, which the addon counts as a command modifier: without it
        // Option+A typed "å" into the Vietnamese buffer and Option+Backspace —
        // the system's delete-previous-word — triggered a reconversion (MAC-07).
        // The addon routes a modified Backspace to `commitAndForwardKey`
        // (`hcime.cpp:450-459`), which is what this does.
        if HCKeyClassifier.hasCommandModifier(event.modifierFlags) {
            // Whatever the shortcut does to the document happens without any
            // further callback, so a recorded commit is no longer provable.
            pendingCommit = nil
            if !markedText.isEmpty { clearComposition(commit: true, client: sender) }
            return false
        }

        let kind: HCKeyKind
        let text: String?
        switch HCKeyClassifier.classify(event) {
        case .engine(let engineKind, let engineText):
            kind = engineKind
            text = engineText

        case .discardComposition:
            // Forward Delete removes the character *after* the caret. With a
            // composition in flight the addon drops the composition and accepts
            // the key (`hcime.cpp:311-315`); with nothing composing it is an
            // editing passthrough and the application does the deleting.
            pendingCommit = nil
            guard !markedText.isEmpty else { return false }
            clearComposition(commit: false, client: sender)
            return true

        case .navigate(let move):
            if movePanelSelection(move) { return true }
            // No panel to steer: the caret moves and this controller is never
            // told, so anything recorded about the last commit is stale (MAC-01).
            pendingCommit = nil
            if !markedText.isEmpty { clearComposition(commit: true, client: sender) }
            return false

        case .application:
            pendingCommit = nil
            if !markedText.isEmpty { clearComposition(commit: true, client: sender) }
            return false
        }

        // Hán Nôm Telex/VIQR: the core returns digits unhandled on purpose, so
        // candidate selection is the frontend's job. In Hán Nôm VNI digits stay
        // tone/shape triggers and must fall through to the core untouched.
        if session.mode.isHanNom, session.mode != .hanNomVni, !candidates.isEmpty,
           let digit = text?.first, digit.isNumber, digit != "0",
           let index = digit.wholeNumberValue
        {
            let absolute = index - 1
            if absolute < candidates.count {
                let result = session.selectHanNomCandidate(absoluteIndex: absolute)
                if result.handled {
                    pendingCommit = nil
                    insert(result.text, client: sender)
                    candidates = []
                    hideCandidates()
                    return true
                }
            }
        }

        // Enter takes the candidate the panel has highlighted, as
        // `hcime.cpp:386-391` does; with nothing highlighted it falls through to
        // the core, which commits the reading.
        if kind == .enter, session.mode.isHanNom, !candidates.isEmpty,
           let panel = HCIMEApplication.candidatesPanel, panel.isPanelVisible,
           let index = panel.highlightedCandidateIndex, index < candidates.count
        {
            let result = session.selectHanNomCandidate(absoluteIndex: index)
            if result.handled {
                pendingCommit = nil
                insert(result.text, client: sender)
                candidates = []
                hideCandidates()
                return true
            }
        }

        let result = session.handleKey(kind: kind, text: text, settings: settings)
        guard result.handled else {
            // The core declined the key. Anything still in the preedit must be
            // flushed before the client receives the raw key, or the text lands
            // out of order — and the client is about to edit the document
            // itself, which invalidates the recorded commit. Printables are no
            // exception: a declined digit left the controller believing text was
            // still marked while the client had finalised it, and the next
            // keystroke re-marked the word a second time (MAC-09).
            pendingCommit = nil
            if !markedText.isEmpty { clearComposition(commit: true, client: sender) }
            return false
        }

        if result.errorCode < 0 {
            clearComposition(commit: false, client: sender)
            return true
        }

        switch result.status {
        case .inProgress:
            // A live preedit sits between the last commit and the caret, so the
            // recorded range no longer describes the document. Fcitx5 clears
            // `pendingCommit` on IN_PROGRESS for the same reason
            // (`hcime.cpp:575`).
            pendingCommit = nil
            setMarked(result.text, client: sender)
            updateCandidates(result.candidates)
            return true

        case .reconversionActive:
            // The core has reopened the last commit as an editable preedit and
            // expects the committed copy to have been taken back out of the
            // document first. Do that only against a range that has been read
            // back and compared; if it cannot be, abandon the reconversion
            // rather than delete text that was never verified (MAC-01) or leave
            // a second copy behind (MAC-02).
            guard let withdrawal = withdrawalRange(reopening: result.text, client: sender) else {
                abandonReconversion(client: sender)
                return false
            }
            pendingCommit = nil
            setMarked(result.text, client: sender, replacing: withdrawal)
            updateCandidates(result.candidates)
            return true

        case .commit, .englishFallback:
            insert(result.text, client: sender)
            candidates = []
            hideCandidates()
            armPendingCommit(text: result.text, kind: kind, keyText: text)
            // Space / boundary / Enter are delimiters the application still
            // needs to receive — the Fcitx5 addon forwards the raw key here, and
            // returning false is the IMK equivalent.
            return !(kind == .space || kind == .boundary || kind == .enter)

        case .escRestoredRaw:
            pendingCommit = nil
            insert(result.text, client: sender)
            candidates = []
            hideCandidates()
            return true
        }
    }

    // MARK: - Reconversion

    /// Records what a commit put into the document so a following Backspace can
    /// prove it is still there before withdrawing it.
    ///
    /// Hán Nôm is never armed: the core reopens the Latin *reading*, not the
    /// glyphs that were committed, so there is nothing the frontend could match
    /// the document against. The Fcitx5 addon refuses Hán Nôm reconversion for
    /// the same reason (`hcime.cpp:867`).
    private func armPendingCommit(text: String, kind: HCKeyKind, keyText: String?) {
        pendingCommit = nil
        guard !session.mode.isHanNom, !text.isEmpty else { return }
        switch kind {
        case .space, .boundary:
            // This controller returns false for these, so the application — not
            // this process — writes the delimiter.
            guard let delimiter = keyText, !delimiter.isEmpty else { return }
            pendingCommit = PendingCommit(text: text, delimiter: delimiter)
        case .enter:
            // Never armed. What Enter does to the document is the application's
            // business and varies wildly — Slack, Discord and Messages send the
            // message and empty the field, so the "newline" this controller would
            // record is not there at all (MAC-10). The addon arms only on Space
            // and boundary characters for the same reason (`hcime.cpp:605-609`).
            return
        default:
            // The committing key was consumed by the core, so nothing follows
            // the commit in the document.
            pendingCommit = PendingCommit(text: text, delimiter: "")
        }
    }

    /// The document range the reopened word may replace, or `nil` when deleting
    /// it cannot be shown to be safe.
    ///
    /// This is the IMK counterpart of the addon's `validatePendingCommit`
    /// (`hcime.cpp:819`). Fcitx5 can compare a full surrounding-text snapshot;
    /// IMK has no surrounding-text notification at all — a mouse click moves the
    /// caret and this controller is never told — so the only trustworthy check
    /// is to read the exact range back out of the client and compare it
    /// character for character with what was committed. Arithmetic on
    /// `selectedRange()` alone is what destroyed the user's text in MAC-01;
    /// nothing here deletes on arithmetic.
    private func withdrawalRange(reopening reopened: String, client sender: Any!) -> NSRange? {
        guard let pending = pendingCommit, markedText.isEmpty else { return nil }
        // The core must be handing back the very text that was committed. If it
        // reopened anything else, the recorded range describes different text —
        // the addon makes the same comparison at `hcime.cpp:882`.
        guard reopened == pending.text else { return nil }
        guard let client = sender as? IMKTextInput else { return nil }

        let written = pending.written as NSString
        let length = written.length
        guard length > 0 else { return nil }

        // A client without TSMDocumentAccess reports NSNotFound here and ignores
        // replacement ranges outright, so a substitution is impossible in it and
        // an unranged `setMarkedText` would simply duplicate the word (MAC-02).
        // A non-empty selection means Backspace belongs to that selection.
        let selection = client.selectedRange()
        guard selection.location != NSNotFound, selection.length == 0,
              selection.location >= length
        else { return nil }

        // An inline session of our own would sit between the commit and the
        // caret; `markedText.isEmpty` above should already exclude it.
        let marked = client.markedRange()
        guard marked.location == NSNotFound || marked.length == 0 else { return nil }

        let range = NSRange(location: selection.location - length, length: length)
        guard let substring = client.attributedSubstring(from: range) else { return nil }
        // A client that answers with something other than the range asked for
        // (a split surrogate pair, or the selection returned by a client with no
        // document access) has not verified anything.
        let document = substring.string
        guard (document as NSString).length == length, document == (written as String)
        else { return nil }
        return range
    }

    /// Drops a reconversion that cannot be performed safely. The core has
    /// already reopened the commit into its buffer, so that has to be discarded
    /// too, and the Backspace is handed back to the application — which deletes
    /// one character, which is what the user pressed it for. The addon's
    /// `resetAndForwardKey` (`hcime.cpp:456`) does exactly this.
    private func abandonReconversion(client sender: Any!) {
        pendingCommit = nil
        session.reset()
        candidates = []
        hideCandidates()
        guard !markedText.isEmpty else { return }
        markedText = ""
        (sender as? IMKTextInput)?.setMarkedText(
            "", selectionRange: NSRange(location: 0, length: 0),
            replacementRange: NSRange(location: NSNotFound, length: 0))
    }

    // MARK: - Client text

    private func setMarked(_ text: String, client sender: Any!, replacing: NSRange? = nil) {
        markedText = text
        guard let client = sender as? IMKTextInput else { return }

        // On reconversion the marked text REPLACES the characters the last
        // commit wrote, rather than sitting after them. `replacing` is only ever
        // non-nil for a range `withdrawalRange` has read back and compared.
        let replacementRange = replacing ?? NSRange(location: NSNotFound, length: 0)

        if text.isEmpty {
            client.setMarkedText(
                "", selectionRange: NSRange(location: 0, length: 0),
                replacementRange: replacementRange)
            return
        }
        let attributed = NSAttributedString(
            string: text,
            attributes: [
                .underlineStyle: NSUnderlineStyle.single.rawValue
            ])
        client.setMarkedText(
            attributed,
            selectionRange: NSRange(location: text.utf16.count, length: 0),
            replacementRange: replacementRange)
    }

    private func insert(_ text: String, client sender: Any!) {
        guard let client = sender as? IMKTextInput else { return }
        if !markedText.isEmpty {
            client.setMarkedText(
                "", selectionRange: NSRange(location: 0, length: 0),
                replacementRange: NSRange(location: NSNotFound, length: 0))
            markedText = ""
        }
        guard !text.isEmpty else { return }
        client.insertText(text, replacementRange: NSRange(location: NSNotFound, length: 0))
    }

    /// What the core would commit if the word ended here.
    ///
    /// `markedText` is what the composition *renders as*, which for an English
    /// word is the Vietnamese misreading of it — "test" shows as "tét". Handing
    /// that to the client is MAC-04: focus loss and Tab wrote "tét" into the
    /// document where the core would have restored "test". The addon resolves it
    /// the same way, by sending `HC_KEY_ENTER` before committing
    /// (`commitActivePreedit`, `hcime.cpp:786-810`), which runs English restore
    /// and auto-restore over the raw keystrokes.
    ///
    /// Falls back to the rendered preedit if the core declines to resolve it:
    /// text the user typed is never dropped on the floor.
    private func resolvedCommitText() -> String {
        guard !markedText.isEmpty else { return "" }
        let result = session.handleKey(kind: .enter, text: nil, settings: settings)
        guard result.handled, result.errorCode >= 0, !result.text.isEmpty else { return markedText }
        return result.text
    }

    /// Ends the current composition. `commit: true` keeps whatever is in the
    /// preedit, resolved as the core would resolve it; `commit: false` discards
    /// it.
    private func clearComposition(commit: Bool, client sender: Any!) {
        // Resolve before the session is reset — afterwards there is no raw
        // buffer left to restore from.
        let pending = commit ? resolvedCommitText() : ""
        pendingCommit = nil
        markedText = ""
        candidates = []
        hideCandidates()
        session.reset()
        guard let client = sender as? IMKTextInput else { return }
        client.setMarkedText(
            "", selectionRange: NSRange(location: 0, length: 0),
            replacementRange: NSRange(location: NSNotFound, length: 0))
        if commit, !pending.isEmpty {
            client.insertText(pending, replacementRange: NSRange(location: NSNotFound, length: 0))
        }
    }

    // MARK: - Candidates

    private func updateCandidates(_ newCandidates: [HCCandidate]) {
        candidates = newCandidates
        guard session.mode.isHanNom else {
            hideCandidates()
            return
        }
        guard let panel = HCIMEApplication.candidatesPanel else { return }
        if candidates.isEmpty {
            panel.hidePanel()
        } else {
            // Digits select a candidate in Telex and VIQR, where the core hands
            // them back unhandled. In Hán Nôm VNI they are tone and shape
            // triggers and belong to the core, so the panel is given no
            // selection keys at all rather than being trusted not to eat them.
            panel.setPanelSelectionKeys(
                session.mode == .hanNomVni ? [] : HCIMEApplication.digitSelectionKeyCodes)
            panel.updatePanel()
            panel.showPanel()
        }
    }

    private func hideCandidates() {
        HCIMEApplication.candidatesPanel?.hidePanel()
    }

    /// Steers the candidate panel with an arrow, page or Tab key. Returns false
    /// when there is no panel to steer, in which case the key is the
    /// application's.
    ///
    /// Before this existed the panel was created, shown and hidden but never
    /// sent a single key event: arrows fell through to the application and moved
    /// the caret out from under a live preedit, and Hán Nôm VNI — where digit
    /// selection is deliberately unavailable — had no keyboard selection at all
    /// (MAC-11). The addon routes the same key set at `hcime.cpp:347-405`.
    private func movePanelSelection(_ move: HCPanelMove) -> Bool {
        guard session.mode.isHanNom, !candidates.isEmpty,
              let panel = HCIMEApplication.candidatesPanel, panel.isPanelVisible
        else { return false }
        return panel.movePanelSelection(move)
    }

    /// How a candidate is shown in the panel. A glyph no installed font can
    /// draw appears as a box; naming its codepoint keeps it identifiable
    /// instead of leaving the user with an unexplained square.
    ///
    /// The font is passed in rather than rebuilt here: this runs once per
    /// candidate on the keystroke path, and constructing the cascade dominated
    /// the cost.
    private func displayString(for candidate: HCCandidate, using font: NSFont) -> String {
        guard !HanNomFont.canDisplay(candidate.text, using: font) else {
            return candidate.text
        }
        return "\(candidate.text)  \(HanNomFont.codepoints(of: candidate.text))"
    }

    /// Supplies the candidate strings to the IMK panel.
    override func candidates(_ sender: Any!) -> [Any]! {
        let panelFont = HanNomFont.font(ofSize: HCIMEInputController.candidatePointSize)
        return candidates.map { displayString(for: $0, using: panelFont) }
    }

    override func candidateSelected(_ candidateString: NSAttributedString!) {
        let panelFont = HanNomFont.font(ofSize: HCIMEInputController.candidatePointSize)
        guard let shown = candidateString?.string,
              let index = candidates.firstIndex(where: {
                  displayString(for: $0, using: panelFont) == shown
              })
        else { return }
        let result = session.selectHanNomCandidate(absoluteIndex: index)
        guard result.handled else { return }
        insert(result.text, client: client())
        candidates = []
        hideCandidates()
    }

    // MARK: - Menu

    override func menu() -> NSMenu! {
        let menu = NSMenu(title: "HC_IME")

        func toggle(_ title: String, _ selector: Selector, _ on: Bool) {
            let item = NSMenuItem(title: title, action: selector, keyEquivalent: "")
            item.state = on ? .on : .off
            item.target = self
            menu.addItem(item)
        }

        let header = NSMenuItem(title: "Mode: \(session.mode.displayName)", action: nil, keyEquivalent: "")
        header.isEnabled = false
        menu.addItem(header)

        // The input menu is the only surface an installed input method has for
        // telling the user why candidates render as boxes.
        if let warning = HanNomFont.coverageWarning() {
            let item = NSMenuItem(title: "⚠︎ " + warning, action: nil, keyEquivalent: "")
            item.isEnabled = false
            menu.addItem(item)
            let install = NSMenuItem(
                title: "Download Hán Nôm font (Nom Na Tong)…",
                action: #selector(downloadNomFont), keyEquivalent: "")
            install.target = self
            menu.addItem(install)
        }
        menu.addItem(.separator())

        toggle("Spell check", #selector(toggleSpellCheck), settings.spellCheck)
        toggle("Auto-restore invalid sequences", #selector(toggleAutoRestore), settings.autoRestore)
        toggle("Quick consonants", #selector(toggleQuickConsonants), settings.quickConsonants)
        toggle("ESC restores raw keystrokes", #selector(toggleEscRestoreRaw), settings.escRestoreRaw)
        toggle("Legacy tone placement", #selector(toggleLegacyTone), settings.legacyTone)

        menu.addItem(.separator())
        let english = NSMenuItem(
            title: "English protection: \(["Off", "Soft", "Hard"][Int(settings.englishProtection)])",
            action: #selector(cycleEnglishProtection), keyEquivalent: "")
        english.target = self
        menu.addItem(english)

        menu.addItem(.separator())
        let reset = NSMenuItem(
            title: "Reset Hán Nôm learning", action: #selector(resetLearning), keyEquivalent: "")
        reset.target = self
        menu.addItem(reset)

        return menu
    }

    @objc private func toggleSpellCheck() { settings.spellCheck.toggle(); settings.save() }
    @objc private func toggleAutoRestore() { settings.autoRestore.toggle(); settings.save() }
    @objc private func toggleQuickConsonants() { settings.quickConsonants.toggle(); settings.save() }
    @objc private func toggleEscRestoreRaw() { settings.escRestoreRaw.toggle(); settings.save() }
    @objc private func toggleLegacyTone() { settings.legacyTone.toggle(); settings.save() }

    @objc private func cycleEnglishProtection() {
        settings.englishProtection = (settings.englishProtection + 1) % 3
        settings.save()
    }

    @objc private func resetLearning() { session.resetLearning() }

    /// macOS carries "Nom Na Tong" as an optional font asset. Fetching it is
    /// what actually removes the boxes from the candidate panel.
    @objc private func downloadNomFont() {
        HanNomFont.downloadNomFont { installed in
            NSLog("HC_IME: Hán Nôm font download finished, installed=%@",
                  installed ? "yes" : "no")
            HCIMEApplication.candidatesPanel?.setPanelAttributes(HCIMEApplication.panelAttributes())
        }
    }

    // MARK: - Testing

    /// Test seam for `run-tester.sh --selftest`, which drives this controller
    /// headlessly against a mock `IMKTextInput`. It pins the engine
    /// configuration so the assertions cannot be shifted by whatever the
    /// developer running the suite happens to have in their preferences or in
    /// `~/Library/Application Support/hcime/macros.txt`.
    ///
    /// A nil `mode` keeps whatever mode the controller resolved for itself,
    /// which is what the MAC-05 tests are about; everything else pins one.
    /// The mode the engine is actually in, for tests about mode resolution.
    var testingMode: HCInputMode { session.mode }

    func configureForTesting(mode: HCInputMode?, settings: HCSettings) {
        self.settings = settings
        if let mode { session.setMode(mode, legacyTone: settings.legacyTone) }
        // Hán Nôm ranking is learned per user. A suite that learned as it ran
        // would rewrite the developer's history file and would rank the
        // candidates differently on its second run than on its first.
        session.configureHanNom(learning: false)
        session.clearMacros()
        session.reset()
        markedText = ""
        candidates = []
        pendingCommit = nil
    }

    // MARK: - Macros

    /// Macros live in `~/Library/Application Support/hcime/macros.txt`, one
    /// `key=replacement` pair per line, matching the Linux macro file format.
    private func loadMacros() {
        let path = FileManager.default
            .urls(for: .applicationSupportDirectory, in: .userDomainMask).first?
            .appendingPathComponent("hcime/macros.txt")
        guard let path, let contents = try? String(contentsOf: path, encoding: .utf8) else { return }
        session.clearMacros()
        for line in contents.split(separator: "\n", omittingEmptySubsequences: true) {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            guard !trimmed.isEmpty, !trimmed.hasPrefix("#"),
                  let separator = trimmed.firstIndex(of: "=")
            else { continue }
            let key = String(trimmed[trimmed.startIndex..<separator]).trimmingCharacters(in: .whitespaces)
            let value = String(trimmed[trimmed.index(after: separator)...])
                .trimmingCharacters(in: .whitespaces)
            guard !key.isEmpty, !value.isEmpty else { continue }
            session.addMacro(key: key, value: value)
        }
    }
}
