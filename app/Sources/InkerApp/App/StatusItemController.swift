import AppKit
import Combine
import InkerKit
import SwiftUI

/// The menu bar item: the mark, the needs-you count, the drop target, and
/// the popover. AppKit on purpose (stack-decision 3.7: MenuBarExtra cannot
/// take drops). The popover is the whole app: boards, onboarding and
/// settings are screens inside it (CoreStore.flow).
@MainActor
final class StatusItemController: NSObject, NSPopoverDelegate, DropTargetHandler {
    /// Which drop path to use. false: a StatusItemDropView over the button.
    /// true: the button's window via ButtonWindowDropTarget. Flip this if the
    /// overlay view never sees drags on some macOS version (see 3.7).
    static let useButtonDropTarget = false

    private let statusItem: NSStatusItem
    private let popover = NSPopover()
    private let store: CoreStore
    private let clock: RenderClock
    private var dropView: StatusItemDropView?
    private var windowDropTarget: ButtonWindowDropTarget?
    private var cancellables = Set<AnyCancellable>()
    /// A transient popover closes on the mouse-down that lands on our icon,
    /// before our own click handler runs; without this the click reopens it.
    private var lastClosed = Date.distantPast
    /// Escape, watched while the popover is up.
    private var keyMonitor: Any?

    init(store: CoreStore, clock: RenderClock) {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        self.store = store
        self.clock = clock
        super.init()
        configureButton()
        configurePopover()
        store.$model
            .map(\.badgeCount)
            .removeDuplicates()
            .receive(on: DispatchQueue.main)
            .sink { [weak self] count in
                MainActor.assumeIsolated { self?.setBadge(count) }
            }
            .store(in: &cancellables)
    }

    private func configureButton() {
        guard let button = statusItem.button else { return }
        button.image = MarkIcon.make()
        button.imagePosition = .imageLeft
        button.title = ""
        button.target = self
        button.action = #selector(buttonClicked(_:))
        button.setAccessibilityLabel("INKER")

        if StatusItemController.useButtonDropTarget {
            let target = ButtonWindowDropTarget()
            target.handler = self
            target.attach(to: button)
            windowDropTarget = target
        } else {
            let view = StatusItemDropView(frame: button.bounds)
            view.autoresizingMask = [.width, .height]
            view.handler = self
            view.onClick = { [weak self] in self?.togglePopover() }
            button.addSubview(view)
            dropView = view
        }
    }

    private func configurePopover() {
        popover.contentSize = NSSize(width: 360, height: 500)
        // Never transient: the popover stays up while the user clicks around
        // Finder and drags files in. The icon and Escape close it.
        popover.behavior = .applicationDefined
        popover.animates = true
        popover.appearance = NSAppearance(named: .darkAqua)
        popover.delegate = self
        let root = PopoverRootView()
            .environmentObject(store)
            .environmentObject(clock)
        popover.contentViewController = NSHostingController(rootView: root)
    }

    /// Badge: the needs-you count as the button title, empty when zero.
    private func setBadge(_ count: Int) {
        guard let button = statusItem.button else { return }
        button.imagePosition = .imageLeft
        button.title = count > 0 ? "\(count)" : ""
    }

    @objc private func buttonClicked(_ sender: Any?) {
        togglePopover()
    }

    func togglePopover() {
        if popover.isShown {
            popover.performClose(nil)
        } else if Date().timeIntervalSince(lastClosed) > 0.25 {
            showPopover()
        }
    }

    func showPopover() {
        guard let button = statusItem.button, !popover.isShown else { return }
        popover.show(relativeTo: button.bounds, of: button, preferredEdge: .minY)
        NSApp.activate()
    }

    /// While pinned (the folder chooser's panel is up) Escape is the
    /// panel's, not ours.
    func setPinned(_ pinned: Bool) {
        isPinned = pinned
    }

    // MARK: NSPopoverDelegate

    nonisolated func popoverDidShow(_ notification: Notification) {
        MainActor.assumeIsolated {
            clock.surfaceAppeared()
            store.refresh()
            store.popoverOpened()
            installDismissalMonitors()
        }
    }

    nonisolated func popoverDidClose(_ notification: Notification) {
        MainActor.assumeIsolated {
            removeDismissalMonitors()
            clock.surfaceDisappeared()
            lastClosed = Date()
            store.isDropTargeted = false
            store.popoverClosed()
        }
    }

    // MARK: Escape and outside clicks

    private var isPinned = false

    private func installDismissalMonitors() {
        removeDismissalMonitors()
        // Keys typed into the popover's own window. A row menu or the folder
        // chooser has its own window and keeps its Escape.
        keyMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self, event.window === popoverWindow,
                  let kind = PopoverDismissal.Event(keyCode: event.keyCode) else { return event }
            return dismiss(on: kind) ? nil : event
        }
    }

    private var popoverWindow: NSWindow? { popover.contentViewController?.view.window }

    private func removeDismissalMonitors() {
        if let keyMonitor { NSEvent.removeMonitor(keyMonitor) }
        keyMonitor = nil
    }

    /// True when the event closed the popover, or the row menu inside it.
    private func dismiss(on event: PopoverDismissal.Event) -> Bool {
        guard popover.isShown else { return false }
        switch PopoverDismissal.outcome(on: event, pinned: isPinned, menuOpen: store.openMenuID != nil) {
        case .nothing:
            return false
        case .closeMenu:
            store.closeMenu()
            return true
        case .closePopover:
            popover.performClose(nil)
            return true
        }
    }

    // MARK: DropTargetHandler

    func dragEntered() -> NSDragOperation {
        store.isDropTargeted = true
        showPopover()
        return .copy
    }

    func dragExited() {
        store.isDropTargeted = false
    }

    func performDrop(urls: [URL]) -> Bool {
        store.isDropTargeted = false
        guard !urls.isEmpty else { return false }
        store.receive(urls: urls)
        return true
    }
}
