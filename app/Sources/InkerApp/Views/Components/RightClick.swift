import AppKit
import SwiftUI

/// An invisible layer that sees right-clicks and control-clicks and lets
/// every other event fall through to the SwiftUI views beneath it.
struct RightClickCatcher: NSViewRepresentable {
    let action: () -> Void

    func makeNSView(context: Context) -> CatcherView {
        let view = CatcherView()
        view.action = action
        return view
    }

    func updateNSView(_ view: CatcherView, context: Context) {
        view.action = action
    }

    final class CatcherView: NSView {
        var action: () -> Void = {}

        /// Only a secondary click is ours; everything else hits what is below.
        override func hitTest(_ point: NSPoint) -> NSView? {
            guard let event = NSApp.currentEvent, CatcherView.isSecondary(event) else { return nil }
            return super.hitTest(point)
        }

        override func rightMouseDown(with event: NSEvent) {
            action()
        }

        override func mouseDown(with event: NSEvent) {
            if CatcherView.isSecondary(event) { action() } else { super.mouseDown(with: event) }
        }

        static func isSecondary(_ event: NSEvent) -> Bool {
            switch event.type {
            case .rightMouseDown, .rightMouseUp:
                return true
            case .leftMouseDown, .leftMouseUp:
                return event.modifierFlags.contains(.control)
            default:
                return false
            }
        }
    }
}
