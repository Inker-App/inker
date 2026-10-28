import CoreText
import Foundation
import os

/// Registers the bundled TTFs for this process (decision: bundle, not install).
enum FontRegistry {
    private static let log = Logger(subsystem: "sh.inker.app", category: "fonts")

    static func registerBundledFonts() {
        for name in ["Archivo-Variable", "JetBrainsMono-Variable"] {
            guard let url = ResourceBundle.url(forResource: name, withExtension: "ttf", subdirectory: "Fonts") else {
                log.error("missing bundled font \(name, privacy: .public)")
                continue
            }
            var error: Unmanaged<CFError>?
            if !CTFontManagerRegisterFontsForURL(url as CFURL, .process, &error) {
                // Already registered (e.g. a second launch path) is fine.
                let message = error?.takeRetainedValue().localizedDescription ?? "unknown"
                log.debug("font \(name, privacy: .public) not registered: \(message, privacy: .public)")
            }
        }
    }
}
