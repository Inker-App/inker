import AppKit
import InkerCore
import InkerKit
import os
import SwiftUI

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate, CorePresenter {
    private var store: CoreStore?
    private var statusController: StatusItemController?
    private var clock: RenderClock { RenderClock.shared }
    private let log = Logger(subsystem: "sh.inker.app", category: "app")

    func applicationDidFinishLaunching(_ notification: Notification) {
        FontRegistry.registerBundledFonts()
        let env = ProcessInfo.processInfo.environment
        let store = CoreStore(handle: makeHandle(), forceOnboarding: env["INKER_ONBOARDING"] == "1")
        store.presenter = self
        self.store = store
        let controller = StatusItemController(store: store, clock: clock)
        statusController = controller
        // Onboarding is a screen in the popover: open it by itself at first launch.
        // VERIFY: the status item's button may not be placed in the menu bar
        // until the next run-loop turn; showing on the next turn anchors the
        // popover under it rather than at the screen corner.
        if store.flow.needsAttentionAtLaunch {
            DispatchQueue.main.async {
                MainActor.assumeIsolated { controller.showPopover() }
            }
        }
    }

    /// Stop the MCP socket and the ingest worker. Drops not yet processed stay
    /// queued and resume at the next launch.
    func applicationWillTerminate(_ notification: Notification) {
        store?.handle.close()
    }

    /// `INKER_PREVIEW=empty|processing|attention|ready` freezes the popover in
    /// one board. Otherwise `open`: the real core, with the Keychain, PDFKit
    /// and Vision as its providers, serving MCP on `<data dir>/inker.sock`.
    private func makeHandle() -> CoreHandle {
        let env = ProcessInfo.processInfo.environment
        switch env["INKER_PREVIEW"] {
        case "empty": return CoreHandle.preview(scenario: .empty)
        case "processing": return CoreHandle.preview(scenario: .processing)
        case "attention": return CoreHandle.preview(scenario: .attention)
        case "ready": return CoreHandle.preview(scenario: .ready)
        default: break
        }
        let home = FileManager.default.homeDirectoryForCurrentUser
        let settings = AppSettings(store: UserDefaults.standard)
        let privateFolder = PrivateFolderSection.folder(stored: settings.privateFolderPath, home: home).path
        // Must match `inker_ipc::paths::data_dir()` on macOS, which is where
        // the `inker mcp` shim looks for the socket. The core creates it 0700.
        let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first?
            .appendingPathComponent("sh.inker", isDirectory: true)
        do {
            return try CoreHandle.open(privateFolder: privateFolder, dataDir: support?.path ?? "",
                                       providers: MacProviders())
        } catch {
            log.error("core open failed, using an empty preview: \(String(describing: error), privacy: .public)")
            return CoreHandle.preview(scenario: .empty)
        }
    }

    // MARK: CorePresenter

    func presentPopover() {
        statusController?.showPopover()
    }

    func setPopoverPinned(_ pinned: Bool) {
        statusController?.setPinned(pinned)
    }
}
