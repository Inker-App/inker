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
    private var revertBehavior: DispatchWorkItem?
    /// A transient popover closes on the mouse-down that lands on our icon,
    /// before our own click handler runs; without this the click reopens it.
    private var lastClosed = Date.distantPast
    /// Escape and outside clicks, watched while the popover is up. The
    /// transient behavior alone misses both when the app is not active,
    /// which a status bar app usually is not.
    private var keyMonitor: Any?
    private var globalMouseMonitor: Any?

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
        popover.behavior = .transient
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

    /// While pinned the popover ignores outside clicks, so the folder
    /// chooser's panel does not close it.
    func setPinned(_ pinned: Bool) {
        revertBehavior?.cancel()
        popover.behavior = pinned ? .applicationDefined : .transient
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
            popover.behavior = .transient
            store.isDropTargeted = false
            store.popoverClosed()
        }
    }

    // MARK: Escape and outside clicks

    private var isPinned: Bool { popover.behavior == .applicationDefined }

    private func installDismissalMonitors() {
        removeDismissalMonitors()
        // Keys typed into the popover's own window. A row menu or the folder
        // chooser has its own window and keeps its Escape.
        keyMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self, event.window === popoverWindow,
                  let kind = PopoverDismissal.Event(keyCode: event.keyCode) else { return event }
            return dismiss(on: kind) ? nil : event
        }
        // Clicks in other apps. Our own windows (the status item, the popover,
        // its menus) are not routed here; the transient behavior covers them.
        let mouse: NSEvent.EventTypeMask = [.leftMouseDown, .rightMouseDown, .otherMouseDown]
        globalMouseMonitor = NSEvent.addGlobalMonitorForEvents(matching: mouse) { [weak self] _ in
            MainActor.assumeIsolated { _ = self?.dismiss(on: .mouseDown(insidePopover: false)) }
        }
    }

    private var popoverWindow: NSWindow? { popover.contentViewController?.view.window }

    private func removeDismissalMonitors() {
        for monitor in [keyMonitor, globalMouseMonitor] {
            if let monitor { NSEvent.removeMonitor(monitor) }
        }
        keyMonitor = nil
        globalMouseMonitor = nil
    }

    /// True when the event closed the popover.
    private func dismiss(on event: PopoverDismissal.Event) -> Bool {
        guard popover.isShown, PopoverDismissal.closes(on: event, pinned: isPinned) else { return false }
        popover.performClose(nil)
        return true
    }

    // MARK: DropTargetHandler

    func dragEntered() -> NSDragOperation {
        revertBehavior?.cancel()
        // Stay open while the drag is in flight: transient popovers close when
        // the click that ends a drag lands elsewhere.
        popover.behavior = .applicationDefined
        store.isDropTargeted = true
        showPopover()
        return .copy
    }

    func dragExited() {
        store.isDropTargeted = false
        // The drag may be heading into the popover itself; give it a moment
        // before letting the popover close on outside clicks again.
        let work = DispatchWorkItem { [weak self] in
            MainActor.assumeIsolated { self?.popover.behavior = .transient }
        }
        revertBehavior = work
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.0, execute: work)
    }

    func performDrop(urls: [URL]) -> Bool {
        revertBehavior?.cancel()
        store.isDropTargeted = false
        popover.behavior = .transient
        guard !urls.isEmpty else { return false }
        store.receive(urls: urls)
        return true
    }
}
