import AppKit
import Combine
import SwiftUI

/// The menu bar item: the mark, the needs-you count, the drop target, and
/// the popover. AppKit on purpose (stack-decision 3.7: MenuBarExtra cannot
/// take drops).
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
    private let onSettings: () -> Void
    private var dropView: StatusItemDropView?
    private var windowDropTarget: ButtonWindowDropTarget?
    private var cancellables = Set<AnyCancellable>()
    private var revertBehavior: DispatchWorkItem?
    /// A transient popover closes on the mouse-down that lands on our icon,
    /// before our own click handler runs; without this the click reopens it.
    private var lastClosed = Date.distantPast

    init(store: CoreStore, clock: RenderClock, onSettings: @escaping () -> Void) {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        self.store = store
        self.clock = clock
        self.onSettings = onSettings
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
        let root = PopoverRootView(onSettings: { [weak self] in self?.onSettings() })
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

    private func showPopover() {
        guard let button = statusItem.button, !popover.isShown else { return }
        popover.show(relativeTo: button.bounds, of: button, preferredEdge: .minY)
        NSApp.activate()
    }

    // MARK: NSPopoverDelegate

    nonisolated func popoverDidShow(_ notification: Notification) {
        MainActor.assumeIsolated {
            clock.surfaceAppeared()
            store.refresh()
        }
    }

    nonisolated func popoverDidClose(_ notification: Notification) {
        MainActor.assumeIsolated {
            clock.surfaceDisappeared()
            lastClosed = Date()
            popover.behavior = .transient
            store.isDropTargeted = false
        }
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
