import Cocoa
import InputMethodKit

/// The part of `IMKCandidates` this controller drives.
///
/// `IMKCandidates` cannot exist without an `IMKServer`, so in any process that
/// is not the input-method server — including the self-test — the panel is nil
/// and every line that touches it is unreachable. Key routing into the panel
/// (MAC-11) is exactly such a line, so the controller talks to this protocol and
/// the self-test substitutes a stand-in. What the stand-in cannot model is
/// IMK's own interpretation of the keys it is handed; that is noted where the
/// tests are.
protocol HCCandidatePanel: AnyObject {
    var isPanelVisible: Bool { get }
    func showPanel()
    func hidePanel()
    /// Re-reads the candidate list, by calling back into `candidates(_:)`.
    func updatePanel()
    func setPanelAttributes(_ attributes: [AnyHashable: Any])
    /// Key codes that pick a candidate directly, or `[]` for none.
    func setPanelSelectionKeys(_ keyCodes: [Int])
    /// Moves the highlighted candidate. Returns false when the panel declined.
    func movePanelSelection(_ move: HCPanelMove) -> Bool
    /// Row index of the highlighted candidate, or nil when nothing is
    /// highlighted.
    var highlightedCandidateIndex: Int? { get }
}

extension IMKCandidates: HCCandidatePanel {
    var isPanelVisible: Bool { isVisible() }
    func showPanel() { show() }
    func hidePanel() { hide() }
    func updatePanel() { update() }
    func setPanelAttributes(_ attributes: [AnyHashable: Any]) { setAttributes(attributes) }

    func setPanelSelectionKeys(_ keyCodes: [Int]) {
        setSelectionKeys(keyCodes.map { NSNumber(value: $0) })
    }

    /// `interpretKeyEvents` is the only documented way into an
    /// `IMKCandidates`'s own selection machinery, and it takes events. The event
    /// is synthesized from the *meaning* of the key rather than forwarded raw so
    /// that Tab, Right and Down all move one candidate forward the way the
    /// Fcitx5 addon defines them (`hcime.cpp:352-385`), instead of depending on
    /// how this panel type happens to interpret each key.
    func movePanelSelection(_ move: HCPanelMove) -> Bool {
        guard let event = move.canonicalEvent else { return false }
        interpretKeyEvents([event])
        return true
    }

    var highlightedCandidateIndex: Int? {
        let identifier = selectedCandidate()
        guard identifier != NSNotFound else { return nil }
        let line = lineNumberForCandidate(withIdentifier: identifier)
        guard line != NSNotFound, line >= 0 else { return nil }
        return line
    }
}

/// Process-wide IMK plumbing. The server name must match
/// `InputMethodConnectionName` in Info.plist or macOS will not connect to us.
///
/// This lives outside `main.swift` so `HCIMEInputController` can be compiled
/// into a binary that is not the input-method server — the self-test harness in
/// `Tester/` drives the real controller, and a file with top-level code can only
/// be the entry point of one executable.
enum HCIMEApplication {
    static var server: IMKServer?
    static var candidatesPanel: HCCandidatePanel?

    /// Virtual key codes for the digits 1–9, which are what a candidate panel
    /// uses as selection keys.
    static let digitSelectionKeyCodes = [18, 19, 20, 21, 23, 22, 26, 28, 25]

    /// One definition of the panel's attributes, so refreshing the font cannot
    /// silently drop the event-routing flag or vice versa.
    ///
    /// `IMKCandidatesSendServerKeyEventFirst` is true because this controller
    /// decides what the panel sees: in Hán Nôm VNI the digits are tone and shape
    /// triggers that must reach the core, and a panel taking key events first
    /// would eat them.
    static func panelAttributes() -> [AnyHashable: Any] {
        [
            NSAttributedString.Key.font.rawValue:
                HanNomFont.font(ofSize: HCIMEInputController.candidatePointSize),
            IMKCandidatesSendServerKeyEventFirst: NSNumber(value: true),
        ]
    }
}
