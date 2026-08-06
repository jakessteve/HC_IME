import Foundation

/// Vietnamese composition method plus optional Hán Nôm translation target.
/// Mirrors `HC_InputMode` in the shared C ABI.
enum HCInputMode: Int32 {
    case telex = 0
    case vni = 1
    case viqr = 2
    case hanNomTelex = 3
    case hanNomVni = 4
    case hanNomViqr = 5

    var isHanNom: Bool { rawValue >= HCInputMode.hanNomTelex.rawValue }

    /// The `composition_method` half of the v4 request (Telex/VNI/VIQR).
    var compositionMethod: Int32 {
        isHanNom ? rawValue - HCInputMode.hanNomTelex.rawValue : rawValue
    }

    var translationTarget: UInt8 {
        isHanNom ? UInt8(TRANSLATION_TARGET_HAN_NOM) : UInt8(TRANSLATION_TARGET_VIETNAMESE)
    }

    var displayName: String {
        switch self {
        case .telex: return "Telex"
        case .vni: return "VNI"
        case .viqr: return "VIQR"
        case .hanNomTelex: return "Hán Nôm (Telex)"
        case .hanNomVni: return "Hán Nôm (VNI)"
        case .hanNomViqr: return "Hán Nôm (VIQR)"
        }
    }

    /// Maps a `tsInputModeListKey` identifier from Info.plist to a mode.
    static func from(inputSourceID: String) -> HCInputMode? {
        switch inputSourceID {
        case let id where id.hasSuffix(".Telex"): return .telex
        case let id where id.hasSuffix(".VNI"): return .vni
        case let id where id.hasSuffix(".VIQR"): return .viqr
        case let id where id.hasSuffix(".HanNomTelex"): return .hanNomTelex
        case let id where id.hasSuffix(".HanNomVNI"): return .hanNomVni
        case let id where id.hasSuffix(".HanNomVIQR"): return .hanNomViqr
        default: return nil
        }
    }
}

enum HCKeyKind: Int32 {
    case printable = 0
    case backspace = 1
    case enter = 2
    case space = 3
    case boundary = 4
    case escape = 5
    case other = 6
    case undo = 7
}

enum HCStatus: Int32 {
    case inProgress = 0
    case commit = 1
    case englishFallback = 2
    case reconversionActive = 3
    case escRestoredRaw = 4
}

/// User-tunable engine behaviour. These map 1:1 onto the per-request flags the
/// core expects; the core is stateless with respect to them, so they are read
/// fresh on every keystroke exactly as the Fcitx5 addon does.
struct HCSettings {
    var legacyTone = false
    var spellCheck = true
    var autoRestore = true
    var quickConsonants = false
    /// 0 = Off, 1 = Soft, 2 = Hard.
    var englishProtection: UInt8 = 0
    var macroInEnglish = false
    var escRestoreRaw = false

    private static let defaultsKey = "dev.hcime.settings"

    static func load() -> HCSettings {
        var settings = HCSettings()
        guard let raw = UserDefaults.standard.dictionary(forKey: defaultsKey) else { return settings }
        settings.legacyTone = raw["legacyTone"] as? Bool ?? settings.legacyTone
        settings.spellCheck = raw["spellCheck"] as? Bool ?? settings.spellCheck
        settings.autoRestore = raw["autoRestore"] as? Bool ?? settings.autoRestore
        settings.quickConsonants = raw["quickConsonants"] as? Bool ?? settings.quickConsonants
        settings.englishProtection = (raw["englishProtection"] as? Int).map { UInt8(clamping: $0) }
            ?? settings.englishProtection
        settings.macroInEnglish = raw["macroInEnglish"] as? Bool ?? settings.macroInEnglish
        settings.escRestoreRaw = raw["escRestoreRaw"] as? Bool ?? settings.escRestoreRaw
        return settings
    }

    func save() {
        UserDefaults.standard.set([
            "legacyTone": legacyTone,
            "spellCheck": spellCheck,
            "autoRestore": autoRestore,
            "quickConsonants": quickConsonants,
            "englishProtection": Int(englishProtection),
            "macroInEnglish": macroInEnglish,
            "escRestoreRaw": escRestoreRaw,
        ], forKey: HCSettings.defaultsKey)
    }
}

struct HCCandidate {
    let text: String
    let reading: String
    let kind: UInt8
}

struct HCKeyResult {
    var handled = false
    var status: HCStatus = .inProgress
    var errorCode: Int32 = 0
    var spellCheckStatus: Int32 = 0
    var text = ""
    var candidates: [HCCandidate] = []
    var totalCandidateCount = 0
}

/// Owning Swift wrapper over one `hc_session_*` handle.
///
/// Every string the core returns is copied out before this wrapper returns, so
/// callers never hold a borrowed pointer past the next FFI call — the core
/// documents those buffers as valid only until the next call on the session.
final class HCSession {
    private var handle: UnsafeMutableRawPointer?
    private(set) var mode: HCInputMode

    init(mode: HCInputMode, legacyTone: Bool) {
        self.mode = mode
        self.handle = hc_session_new(mode.rawValue, legacyTone ? 1 : 0)
    }

    deinit {
        if let handle {
            hc_session_flush_hannom_learning(handle)
            hc_session_free(handle)
        }
    }

    var isValid: Bool { handle != nil }

    func reset() {
        guard let handle else { return }
        hc_session_reset(handle)
    }

    func flushLearning() {
        guard let handle else { return }
        hc_session_flush_hannom_learning(handle)
    }

    func resetLearning() {
        guard let handle else { return }
        hc_session_reset_hannom_learning(handle)
    }

    func setMode(_ newMode: HCInputMode, legacyTone: Bool) {
        guard newMode != mode else { return }
        if let handle {
            hc_session_flush_hannom_learning(handle)
            hc_session_free(handle)
        }
        mode = newMode
        handle = hc_session_new(newMode.rawValue, legacyTone ? 1 : 0)
        configureHanNom()
    }

    /// Hán Nôm phrase prediction and local ranking are on by default; the
    /// history path is left null so the core resolves it through `platform.rs`
    /// (`~/Library/Application Support/hcime` on macOS).
    func configureHanNom(phrasePrediction: Bool = true, learning: Bool = true) {
        guard let handle else { return }
        var options = HC_HanNomOptionsV2(
            phrase_prediction: phrasePrediction ? 1 : 0,
            learning_enabled: learning ? 1 : 0,
            history_path: nil,
            user_phrase_path: nil)
        hc_session_set_hannom_options_v2(handle, &options)
    }

    func addMacro(key: String, value: String) {
        guard let handle else { return }
        key.withCString { keyPtr in
            value.withCString { valuePtr in
                hc_session_add_macro(handle, keyPtr, valuePtr)
            }
        }
    }

    func clearMacros() {
        guard let handle else { return }
        hc_session_clear_macros(handle)
    }

    func handleKey(kind: HCKeyKind, text: String?, settings: HCSettings) -> HCKeyResult {
        guard let handle else { return HCKeyResult() }

        var result = HC_KeyResultV2()
        let handled: Int32

        // `withCString` keeps the buffer alive only for the closure body, which
        // is exactly the window the core needs it for.
        if let text, !text.isEmpty {
            handled = text.withCString { textPtr in
                var request = makeRequest(kind: kind, text: textPtr, settings: settings)
                return hc_session_handle_key_v4(handle, &request, &result)
            }
        } else {
            var request = makeRequest(kind: kind, text: nil, settings: settings)
            handled = hc_session_handle_key_v4(handle, &request, &result)
        }

        return decode(result: result, handled: handled)
    }

    /// Commits the candidate at an absolute index in the core's ranked list.
    /// Hán Nôm Telex/VIQR digit selection routes here because the core
    /// deliberately does not treat digits as selectors — it returns them
    /// unhandled so the frontend owns selection.
    func selectHanNomCandidate(absoluteIndex: Int) -> HCKeyResult {
        guard let handle else { return HCKeyResult() }
        var nomResult = HC_HanNomResultV3()
        let handled = hc_session_select_hannom_candidate_v3(
            handle, UInt16(clamping: absoluteIndex), &nomResult)
        guard handled != 0 else { return HCKeyResult() }

        var result = HCKeyResult()
        result.handled = true
        result.status = HCStatus(rawValue: nomResult.status_flag) ?? .inProgress
        result.errorCode = nomResult.error_code
        result.text = HCSession.string(from: nomResult.reading, length: Int(nomResult.reading_len))
        return result
    }

    private func makeRequest(
        kind: HCKeyKind, text: UnsafePointer<CChar>?, settings: HCSettings
    ) -> HC_KeyRequestV2 {
        HC_KeyRequestV2(
            kind: kind.rawValue,
            text: text,
            composition_method: mode.compositionMethod,
            translation_target: mode.translationTarget,
            legacy_tone: settings.legacyTone ? 1 : 0,
            spell_check: settings.spellCheck ? 1 : 0,
            auto_restore: settings.autoRestore ? 1 : 0,
            quick_consonants: settings.quickConsonants ? 1 : 0,
            english_protection: settings.englishProtection,
            macro_in_english: settings.macroInEnglish ? 1 : 0,
            esc_restore_raw: settings.escRestoreRaw ? 1 : 0)
    }

    private func decode(result: HC_KeyResultV2, handled: Int32) -> HCKeyResult {
        var decoded = HCKeyResult()
        decoded.handled = handled != 0 || result.handled != 0
        decoded.status = HCStatus(rawValue: result.status_flag) ?? .inProgress
        decoded.errorCode = result.error_code
        decoded.spellCheckStatus = result.spell_check_status
        decoded.totalCandidateCount = Int(result.total_candidate_count)

        if let composition = result.composition_string, result.composition_len > 0 {
            decoded.text = composition.withMemoryRebound(
                to: UInt8.self, capacity: result.composition_len
            ) { bytes in
                HCSession.string(from: bytes, length: result.composition_len)
            }
        }

        if let candidates = result.candidates, result.candidate_count > 0 {
            decoded.candidates = (0..<Int(result.candidate_count)).compactMap { index in
                let candidate = candidates[index]
                let text = HCSession.string(from: candidate.text, length: Int(candidate.text_len))
                guard !text.isEmpty else { return nil }
                let reading = HCSession.string(
                    from: candidate.reading, length: Int(candidate.reading_len))
                return HCCandidate(text: text, reading: reading, kind: candidate.kind)
            }
        }

        return decoded
    }

    /// Copies a borrowed UTF-8 buffer out of core-owned memory.
    private static func string(from pointer: UnsafePointer<UInt8>?, length: Int) -> String {
        guard let pointer, length > 0 else { return "" }
        return String(decoding: UnsafeBufferPointer(start: pointer, count: length), as: UTF8.self)
    }
}
