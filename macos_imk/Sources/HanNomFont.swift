import AppKit
import CoreText

/// Font selection and glyph-coverage reporting for Hán Nôm candidates.
///
/// Hán Nôm draws heavily on CJK Unified Ideographs Extension B and beyond
/// (U+20000 and up). macOS ships no font covering those planes, so a candidate
/// list rendered in the system font shows boxes — with nothing on screen to
/// explain why. This type does two things about that:
///
///   1. Builds a cascade over any Hán Nôm-capable font the user has installed.
///      This controls *which* font wins for a glyph several fonts cover — Nôm
///      forms differ from Han forms, so preferring NomNaTong over Songti is a
///      fidelity fix. It does not create fallback: CoreText already falls back
///      system-wide, so a font outside this list still renders.
///   2. Reports which candidates cannot be drawn, so the UI can say so instead
///      of showing silent tofu.
enum HanNomFont {

    /// Ordered by how well each covers Nôm specifically, not by popularity.
    /// The first installed family becomes the base font; the rest form the
    /// cascade list.
    static let preferredFamilies = [
        "NomNaTong", "Nom Na Tong", "NomNaTongLight",
        "HanaMinB", "HanaMinA", "Hanamin B", "Hanamin A",
        "Hanom PV", "HAN NOM B", "HAN NOM A",
        "Jigmo3", "Jigmo2", "Jigmo",
        "Noto Serif CJK SC", "Noto Sans CJK SC", "Noto Serif TC", "Noto Sans TC",
        "Songti SC", "STSong", "Hiragino Sans", "PingFang SC",
    ]

    /// Real Nôm glyphs taken from the bundled dictionary, not arbitrary points
    /// in the extended planes. Probing U+20000 or U+2A700 measures whether a
    /// font covers rare Han ideographs nobody types — a Nôm font can legitimately
    /// omit them, which produced a coverage warning while every actual candidate
    /// rendered perfectly.
    private static let coverageProbes: [Unicode.Scalar] = [
        Unicode.Scalar(0x20129)!,  // 𠄩 hai
        Unicode.Scalar(0x232C0)!,  // 𣋀 sao
        Unicode.Scalar(0x2BCF5)!,  // 𫳵 sao
    ]

    private static func scanInstalledFamilies() -> [String] {
        let available = Set(NSFontManager.shared.availableFontFamilies)
        let availableFonts = Set(NSFontManager.shared.availableFonts)
        return preferredFamilies.filter { available.contains($0) || availableFonts.contains($0) }
    }

    /// Cached because enumerating families is not free (~80 ms cold), but
    /// refreshable: the coverage warning tells the user to install a font, so
    /// the answer has to be allowed to change without restarting.
    ///
    /// Unsynchronised on purpose — every caller is InputMethodKit's main
    /// thread. Reaching this from another thread needs a lock first.
    private static var cachedFamilies: [String] = scanInstalledFamilies()

    private static var installedFamilies: [String] { cachedFamilies }

    /// Re-scans installed fonts. Call when the process regains focus, so a
    /// font installed mid-session takes effect.
    static func refresh() {
        cachedFamilies = scanInstalledFamilies()
    }

    /// A font that falls back through every installed Hán Nôm-capable family
    /// before giving up. Falls back to the system font when none is installed —
    /// the text still will not render, but nothing else breaks.
    static func font(ofSize size: CGFloat) -> NSFont {
        let base: NSFont =
            installedFamilies.first.flatMap { NSFont(name: $0, size: size) }
            ?? NSFont.systemFont(ofSize: size)

        let cascade = installedFamilies.dropFirst().map {
            NSFontDescriptor(name: $0, size: size)
        }
        guard !cascade.isEmpty else { return base }

        let descriptor = base.fontDescriptor.addingAttributes([.cascadeList: cascade])
        return NSFont(descriptor: descriptor, size: size) ?? base
    }

    /// Whether every scalar in `text` can actually be drawn.
    ///
    /// `CTFontCreateForString` performs the same system-wide fallback the text
    /// renderer does, so a negative here means no installed font on the machine
    /// can draw it — not merely that the requested font lacks it.
    ///
    /// Deliberately NOT implemented with `CTFontGetGlyphsForCharacters`. That
    /// API is UTF-16 based: a scalar above U+FFFF is a surrogate pair, and it
    /// returns the real glyph for the lead unit and a filler `0` for the trail
    /// unit — `U+20129` yields `true` with glyphs `[14419, 0]`. Treating any
    /// zero as "missing" therefore rejects every supplementary-plane character
    /// no matter which font is used, which is precisely the range Hán Nôm
    /// occupies. Coverage is read from the resolved font's character set
    /// instead.
    static func canDisplay(_ text: String, using font: NSFont) -> Bool {
        for scalar in text.unicodeScalars {
            let piece = String(scalar) as NSString
            let resolved = CTFontCreateForString(
                font, piece, CFRange(location: 0, length: piece.length))
            let name = CTFontCopyPostScriptName(resolved) as String
            // CoreText hands back LastResort when nothing covers the scalar.
            if name == "LastResort" || name.hasPrefix(".LastResort") { return false }
            if !(CTFontCopyCharacterSet(resolved) as CharacterSet).contains(scalar) {
                return false
            }
        }
        return true
    }

    /// Asks macOS to fetch a Hán Nôm font from its optional font catalog.
    ///
    /// "Nom Na Tong" ships as a downloadable asset on macOS — present in the
    /// catalog but not installed by default, which is why Hán Nôm candidates
    /// render as boxes on a stock system. This is the same mechanism Font
    /// Book's download uses; it needs a network connection.
    ///
    /// `completion` runs on the main queue with whether the font is now
    /// available. The font cache is refreshed before it fires.
    static func downloadNomFont(
        family: String = "Nom Na Tong",
        progress: ((Double) -> Void)? = nil,
        completion: @escaping (Bool) -> Void
    ) {
        let descriptor = CTFontDescriptorCreateWithAttributes(
            [kCTFontFamilyNameAttribute: family] as CFDictionary)

        CTFontDescriptorMatchFontDescriptorsWithProgressHandler(
            [descriptor] as CFArray, nil
        ) { state, details in
            switch state {
            case .downloading:
                if let percent = (details as NSDictionary)[
                    kCTFontDescriptorMatchingPercentage as String] as? NSNumber
                {
                    DispatchQueue.main.async { progress?(percent.doubleValue) }
                }
            case .didFinish, .didFailWithError:
                DispatchQueue.main.async {
                    refresh()
                    completion(NSFontManager.shared.availableFontFamilies.contains(family))
                }
            default:
                break
            }
            return true
        }
    }

    /// Whether the optional Hán Nôm font is already present.
    static var nomFontInstalled: Bool {
        NSFontManager.shared.availableFontFamilies.contains("Nom Na Tong")
    }

    /// Nil when the machine can render the extended planes; otherwise a
    /// one-line explanation naming fonts that would fix it.
    static func coverageWarning(size: CGFloat = 18) -> String? {
        let probeFont = font(ofSize: size)
        let missing = coverageProbes.filter { !canDisplay(String($0), using: probeFont) }
        guard !missing.isEmpty else { return nil }
        return "No installed font covers CJK Extension B+ — rare Hán Nôm glyphs "
            + "show as boxes. macOS offers \"Nom Na Tong\" as a downloadable font; "
            + "use the Install button, or Font Book › All Fonts › Nom Na Tong."
    }

    /// Human-readable list of the Hán Nôm fonts actually in use, for
    /// diagnostics.
    static var activeFamilies: [String] { installedFamilies }

    static func codepoints(of text: String) -> String {
        text.unicodeScalars
            .map { String(format: "U+%04X", $0.value) }
            .joined(separator: " ")
    }
}
