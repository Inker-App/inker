import Foundation

/// Finds SwiftPM's resource bundle for this target.
///
/// SwiftPM's generated `Bundle.module` looks next to the executable and at the
/// .app root, and traps if it finds neither. `make app` puts the bundle in
/// Contents/Resources (the .app root would break code signing), so look there
/// first and never trap.
enum ResourceBundle {
    /// `<package>_<target>.bundle`.
    static let name = "INKER_InkerApp.bundle"

    static let bundle: Bundle? = {
        let main = Bundle.main
        let candidates: [URL?] = [
            main.resourceURL?.appendingPathComponent(name),
            main.bundleURL.appendingPathComponent(name),
            main.executableURL?.deletingLastPathComponent().appendingPathComponent(name),
        ]
        for case let url? in candidates {
            if let b = Bundle(url: url) { return b }
        }
        return nil
    }()

    static func url(forResource name: String, withExtension ext: String, subdirectory: String) -> URL? {
        bundle?.url(forResource: name, withExtension: ext, subdirectory: subdirectory)
    }
}
