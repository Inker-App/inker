import AppKit

/// Entry point. A menu bar app: no Dock icon (LSUIElement in Info.plist, and
/// the accessory policy for `swift run`, which has no Info.plist).
///
/// `INKER --snapshots <dir>` renders every popover screen to PNGs in `<dir>`
/// and exits instead of running the app (Snapshots.swift).
@main
enum InkerMain {
    /// NSApplication.delegate is weak; keep the delegate alive here.
    @MainActor private static var delegate: AppDelegate?

    @MainActor
    static func main() {
        let args = CommandLine.arguments
        if let flag = args.firstIndex(of: "--snapshots") {
            guard flag + 1 < args.count else {
                FileHandle.standardError.write(Data("usage: INKER --snapshots <dir>\n".utf8))
                exit(2)
            }
            exit(Snapshots.run(directory: URL(fileURLWithPath: args[flag + 1], isDirectory: true)))
        }
        let app = NSApplication.shared
        let appDelegate = AppDelegate()
        delegate = appDelegate
        app.delegate = appDelegate
        app.setActivationPolicy(.accessory)
        app.run()
    }
}
