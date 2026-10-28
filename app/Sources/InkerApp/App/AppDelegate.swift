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
    private var dropDialog: DropDialogWindowController?
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

    /// `INKER_PREVIEW=empty|processing|attention|ready` freezes the popover in
    /// one board. Otherwise `open`, which today is the live preview backend.
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
        let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first?
            .appendingPathComponent("INKER", isDirectory: true)
        if let support {
            try? FileManager.default.createDirectory(at: support, withIntermediateDirectories: true)
        }
        do {
            return try CoreHandle.open(privateFolder: privateFolder, dataDir: support?.path ?? "")
        } catch {
            log.error("core open failed, using an empty preview: \(String(describing: error), privacy: .public)")
            return CoreHandle.preview(scenario: .empty)
        }
    }

    // MARK: Settings menu (not on the boards; placeholder until Ben designs it)

    private func showSettingsMenu() {
        guard let store else { return }
        let menu = NSMenu()
        let ascii = NSMenuItem(title: "Copy handles as ink/", action: #selector(toggleHandleStyle(_:)), keyEquivalent: "")
        ascii.target = self
        ascii.state = store.settings.handleStyle == .ascii ? .on : .off
        menu.addItem(ascii)
        let setup = NSMenuItem(title: "Set up agents…", action: #selector(openOnboarding(_:)), keyEquivalent: "")
        setup.target = self
        menu.addItem(setup)
        menu.addItem(.separator())
        let quit = NSMenuItem(title: "Quit INKER", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        menu.addItem(quit)
        _ = menu.popUp(positioning: nil, at: NSEvent.mouseLocation, in: nil)
    }

    @objc private func toggleHandleStyle(_ sender: Any?) {
        guard let store else { return }
        store.setHandleStyle(store.settings.handleStyle == .ascii ? .octopus : .ascii)
    }

    @objc private func openOnboarding(_ sender: Any?) {
        presentOnboarding()
    }

    // MARK: CorePresenter

    func presentOnboarding() {
        guard let store else { return }
        if let onboarding {
            onboarding.show()
            return
        }
        let controller = OnboardingWindowController(store: store, clock: clock) { [weak self] in
            var state = OnboardingState(store: UserDefaults.standard)
            state.markComplete()
            // Release after the button action that got us here has unwound.
            DispatchQueue.main.async { MainActor.assumeIsolated { self?.onboarding = nil } }
        }
        onboarding = controller
        controller.show()
    }

    func presentDropDialog(for batch: DropBatch, completion: @escaping (IngestMode?, Bool) -> Void) {
        dropDialog?.close()
        let controller = DropDialogWindowController(batch: batch, clock: clock) { [weak self] mode, always in
            completion(mode, always)
            // Release after the window's own callbacks have unwound.
            DispatchQueue.main.async { MainActor.assumeIsolated { self?.dropDialog = nil } }
        }
        dropDialog = controller
        controller.show()
    }
}
