import AppKit

/// The alternative drop path: register the status button (and its window)
/// for file URLs and take the drag through the window's delegate.
///
/// VERIFY: NSWindow forwards NSDraggingDestination messages to its delegate
/// when the window is registered for the dragged types; this is the widely
/// quoted recipe for drops on NSStatusItem. Taking over the status window's
/// delegate is a private-ish arrangement; prefer StatusItemDropView if it works.
final class ButtonWindowDropTarget: NSObject, NSWindowDelegate, NSDraggingDestination {
    weak var handler: DropTargetHandler?

    func attach(to button: NSStatusBarButton) {
        button.registerForDraggedTypes([.fileURL])
        button.window?.registerForDraggedTypes([.fileURL])
        button.window?.delegate = self
    }

    func draggingEntered(_ sender: NSDraggingInfo) -> NSDragOperation {
        guard DraggedFiles.hasFiles(sender) else { return [] }
        return MainActor.assumeIsolated { handler?.dragEntered() ?? [] }
    }

    func draggingExited(_ sender: NSDraggingInfo?) {
        MainActor.assumeIsolated { handler?.dragExited() }
    }

    func performDragOperation(_ sender: NSDraggingInfo) -> Bool {
        let urls = DraggedFiles.urls(from: sender)
        return MainActor.assumeIsolated { handler?.performDrop(urls: urls) ?? false }
    }
}
