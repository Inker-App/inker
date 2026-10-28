import AppKit
import SwiftUI

/// A dark, title-less window hosting SwiftUI, for onboarding and the drop
/// dialog. Registers with the RenderClock while visible. Not generic: @objc
/// delegate methods cannot live in a generic class.
@MainActor
final class InkPanel: NSObject, NSWindowDelegate {
    let window: NSWindow
    private let clock: RenderClock
    private var isVisible = false
    var onClose: (() -> Void)?

    init(size: NSSize, clock: RenderClock, floating: Bool, content: AnyView) {
        self.clock = clock
        window = NSWindow(contentRect: NSRect(origin: .zero, size: size),
                          styleMask: [.titled, .closable, .fullSizeContentView],
                          backing: .buffered, defer: false)
        super.init()
        window.titlebarAppearsTransparent = true
        window.titleVisibility = .hidden
        window.isMovableByWindowBackground = true
        window.backgroundColor = Palette.nsBackground
        window.appearance = NSAppearance(named: .darkAqua)
        window.isReleasedWhenClosed = false
        window.level = floating ? .floating : .normal
        window.contentView = NSHostingView(rootView: content)
        window.setContentSize(size)
        window.delegate = self
    }

    func show() {
        window.center()
        window.makeKeyAndOrderFront(nil)
        NSApp.activate()
        if !isVisible {
            isVisible = true
            clock.surfaceAppeared()
        }
    }

    func close() {
        window.close()
    }

    nonisolated func windowWillClose(_ notification: Notification) {
        MainActor.assumeIsolated {
            if isVisible {
                isVisible = false
                clock.surfaceDisappeared()
            }
            onClose?()
        }
    }
}
