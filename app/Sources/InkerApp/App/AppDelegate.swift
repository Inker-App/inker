import AppKit
import InkerCore
import InkerKit
import os
import SwiftUI

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate, CorePresenter {
    private var store: CoreStore?
    private var statusController: StatusItemController?
    private var onboarding: OnboardingWindowController?
    private var settingsWindow: SettingsWindowController?
    /// Joins Cmd+Tab while a window is open; menu bar only otherwise.
    private var presence = WindowPresence()
    private var clock: RenderClock { RenderClock.shared }
    private let log = Logger(subsystem: "sh.inker.app", category: "app")

    func applicationDidFinishLaunching(_ notification: Notification) {
        FontRegistry.registerBundledFonts()
        let store = CoreStore(handle: makeHandle())
        store.presenter = self
        self.store = store
        statusController = StatusItemController(store: store, clock: clock) { [weak self] in
            self?.showSettingsMenu()
        }
        let env = ProcessInfo.processInfo.environment
        if OnboardingState(store: UserDefaults.standard).needsOnboarding || env["INKER_ONBOARDING"] == "1" {
            presentOnboarding()
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
        let privateFolder = settings.privateFolderPath ?? FolderPickerWarnings.defaultFolder(home: home).path
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

    // MARK: Settings menu (not on the boards; placeholder until Ben designs it)

    private func showSettingsMenu() {
        let menu = NSMenu()
        let settings = NSMenuItem(title: "Settings…", action: #selector(openSettings(_:)), keyEquivalent: ",")
        settings.target = self
        menu.addItem(settings)
        let setup = NSMenuItem(title: "Set up your chat…", action: #selector(openOnboarding(_:)), keyEquivalent: "")
        setup.target = self
        menu.addItem(setup)
        menu.addItem(.separator())
        let quit = NSMenuItem(title: "Quit INKER", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        menu.addItem(quit)
        _ = menu.popUp(positioning: nil, at: NSEvent.mouseLocation, in: nil)
    }

    @objc private func openOnboarding(_ sender: Any?) {
        presentOnboarding()
    }

    @objc private func openSettings(_ sender: Any?) {
        presentSettings()
    }

    // MARK: Windows and Cmd+Tab

    private func windowOpened(_ id: String) {
        if let policy = presence.opened(id) { apply(policy) }
    }

    private func windowClosed(_ id: String) {
        if let policy = presence.closed(id) { apply(policy) }
    }

    private func apply(_ policy: WindowPresence.Policy) {
        // VERIFY: switching to .regular adds a Dock tile and a Cmd+Tab entry
        // at once; switching back removes them once no window is key.
        NSApp.setActivationPolicy(policy == .regular ? .regular : .accessory)
        if policy == .regular { NSApp.activate() }
    }

    func presentSettings() {
        guard let store else { return }
        if let settingsWindow {
            settingsWindow.show()
            return
        }
        let controller = SettingsWindowController(store: store, clock: clock) { [weak self] in
            self?.windowClosed("settings")
            DispatchQueue.main.async { MainActor.assumeIsolated { self?.settingsWindow = nil } }
        }
        settingsWindow = controller
        windowOpened("settings")
        controller.show()
    }

    // MARK: CorePresenter

    func presentPopover() {
        statusController?.showPopover()
    }

    func presentOnboarding() {
        guard let store else { return }
        if let onboarding {
            onboarding.show()
            return
        }
        let controller = OnboardingWindowController(store: store, clock: clock, onFinish: {
            var state = OnboardingState(store: UserDefaults.standard)
            state.markComplete()
        }, onClose: { [weak self] in
            self?.windowClosed("onboarding")
            // Release after the button action that got us here has unwound.
            DispatchQueue.main.async { MainActor.assumeIsolated { self?.onboarding = nil } }
        })
        onboarding = controller
        windowOpened("onboarding")
        controller.show()
    }
}
