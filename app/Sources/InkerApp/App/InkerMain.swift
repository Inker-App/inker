import AppKit

/// Entry point. A menu bar app: no Dock icon (LSUIElement in Info.plist, and
/// the accessory policy for `swift run`, which has no Info.plist).
@main
enum InkerMain {
    /// NSApplication.delegate is weak; keep the delegate alive here.
    @MainActor private static var delegate: AppDelegate?

    @MainActor
    static func main() {
        let app = NSApplication.shared
        let appDelegate = AppDelegate()
        delegate = appDelegate
        app.delegate = appDelegate
        app.setActivationPolicy(.accessory)
        app.run()
    }
}
