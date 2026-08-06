import Carbon
import Cocoa
import InputMethodKit

// `HCIMEApplication` lives in HCIMEApplication.swift: the controller needs it,
// and the controller is compiled into the self-test binary too, which has its
// own `main.swift`.

// `HCIME --register` asks the Text Input Sources database to pick this bundle
// up immediately, so installing does not require a logout. Activation in
// System Settings is still the user's call.
if CommandLine.arguments.contains("--register") {
    let url = Bundle.main.bundleURL as CFURL
    let status = TISRegisterInputSource(url)
    if status == noErr {
        print("Registered \(Bundle.main.bundleURL.path)")
        exit(0)
    }
    // paramErr (-50) is the usual response when the bundle is already known.
    print("TISRegisterInputSource returned \(status) (already registered, or a logout is needed)")
    exit(status == paramErr ? 0 : 1)
}

let bundle = Bundle.main
guard
    let connectionName = bundle.infoDictionary?["InputMethodConnectionName"] as? String,
    let bundleIdentifier = bundle.bundleIdentifier
else {
    NSLog("HC_IME: Info.plist is missing InputMethodConnectionName or a bundle identifier")
    exit(1)
}

HCIMEApplication.server = IMKServer(name: connectionName, bundleIdentifier: bundleIdentifier)
guard let server = HCIMEApplication.server else {
    NSLog("HC_IME: failed to create IMKServer for connection '\(connectionName)'")
    exit(1)
}

// A single shared candidate panel for Hán Nôm. `.singleColumn` keeps the CJK
// glyphs legible; the panel is only shown while Hán Nôm candidates exist.
if let panel = IMKCandidates(server: server, panelType: kIMKSingleColumnScrollingCandidatePanel) {
    // Prefer an installed Nôm font over the system default. CoreText falls back
    // system-wide either way, so this decides which glyph *form* is drawn, not
    // whether the glyph appears at all — nothing renders codepoints no
    // installed font contains. The same dictionary also routes key events
    // through the controller first; see `HCIMEApplication.panelAttributes`.
    panel.setAttributes(HCIMEApplication.panelAttributes())
    HCIMEApplication.candidatesPanel = panel
    if let warning = HanNomFont.coverageWarning() {
        NSLog("HC_IME: %@", warning)
    } else {
        NSLog("HC_IME: Hán Nôm fonts in use: %@",
              HanNomFont.activeFamilies.prefix(3).joined(separator: ", "))
    }
} else {
    // Vietnamese modes never need the panel, so a failure here is not fatal.
    NSLog("HC_IME: could not create the Hán Nôm candidate panel")
}

NSLog("HC_IME: input method server started (connection: \(connectionName))")
NSApplication.shared.run()
