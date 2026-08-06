import Foundation

// Drives the Rust core through the exact same bridging header, v4 FFI call and
// HCSession wrapper the IMK controller uses. If this passes, the macOS side of
// the ABI is sound; anything still broken is in the InputMethodKit glue, not
// the bridge.

struct Case {
    let mode: HCInputMode
    let keys: String
    let expected: String
    let note: String
}

/// Replays a keystroke sequence exactly as the controller would classify it.
func type(_ keys: String, mode: HCInputMode, settings: HCSettings) -> String {
    let session = HCSession(mode: mode, legacyTone: settings.legacyTone)
    session.configureHanNom()
    var committed = ""
    var preedit = ""

    for character in keys {
        let text = String(character)
        let kind: HCKeyKind
        if character == " " {
            kind = .space
        } else if ".,;:!?)]}/\\-_\"'".contains(character) {
            kind = .boundary
        } else {
            kind = .printable
        }

        let result = session.handleKey(kind: kind, text: text, settings: settings)
        guard result.handled else {
            // Unhandled keys reach the application verbatim.
            committed += preedit + text
            preedit = ""
            continue
        }
        switch result.status {
        case .inProgress, .reconversionActive:
            preedit = result.text
        case .commit, .englishFallback, .escRestoredRaw:
            committed += result.text
            preedit = ""
            if kind == .space || kind == .boundary { committed += text }
        }
    }
    return committed + preedit
}

let settings = HCSettings()

let cases: [Case] = [
    .init(mode: .telex, keys: "tieengs Vieejt", expected: "tiếng Việt", note: "Telex tone + circumflex"),
    .init(mode: .telex, keys: "ddaay", expected: "đây", note: "Telex đ digraph"),
    .init(mode: .telex, keys: "hocj", expected: "học", note: "Telex nặng on closed syllable"),
    .init(mode: .telex, keys: "hoocj", expected: "hộc", note: "Telex oo digraph keeps circumflex"),
    .init(mode: .telex, keys: "aaa", expected: "aa", note: "double-tap toggles circumflex off"),
    // English restore resolves at commit, not while composing: the preedit
    // legitimately shows "tét" until the boundary key arrives.
    .init(mode: .telex, keys: "test ", expected: "test ", note: "English word restored on commit"),
    .init(mode: .vni, keys: "tie61ng Vie65t", expected: "tiếng Việt", note: "VNI tone + circumflex"),
    .init(mode: .vni, keys: "a66", expected: "a6", note: "VNI digit toggle emits digit literally"),
    .init(mode: .viqr, keys: "tie^'ng Vie^.t", expected: "tiếng Việt", note: "VIQR tone + circumflex"),
]

var failures = 0
print("HC_IME macOS FFI probe\n")

for testCase in cases {
    let actual = type(testCase.keys, mode: testCase.mode, settings: settings)
    let ok = actual == testCase.expected
    if !ok { failures += 1 }
    let status = ok ? "PASS" : "FAIL"
    print("[\(status)] \(testCase.mode.displayName.padding(toLength: 16, withPad: " ", startingAt: 0)) "
        + "\"\(testCase.keys)\" -> \"\(actual)\""
        + (ok ? "" : "  (expected \"\(testCase.expected)\")"))
    print("        \(testCase.note)")
}

// Hán Nôm goes through the candidate path rather than a plain commit.
print("\nHán Nôm candidate lookup (Telex):")
let nomSession = HCSession(mode: .hanNomTelex, legacyTone: false)
nomSession.configureHanNom()
var nomResult = HCKeyResult()
for character in "vieejt" {
    nomResult = nomSession.handleKey(kind: .printable, text: String(character), settings: settings)
}
print("  reading    : \"\(nomResult.text)\"")
print("  candidates : \(nomResult.candidates.prefix(8).map(\.text).joined(separator: " "))")
print("  total      : \(nomResult.totalCandidateCount)")
if nomResult.candidates.isEmpty {
    failures += 1
    print("  [FAIL] no Hán Nôm candidates returned")
}

print("\n\(failures == 0 ? "All probes passed" : "\(failures) probe(s) failed")")
exit(failures == 0 ? 0 : 1)
