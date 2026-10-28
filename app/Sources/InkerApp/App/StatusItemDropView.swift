import AppKit

/// A transparent view laid over `statusItem.button` that takes file drops
/// (the pattern prof18/slashgrab ships). It also takes clicks, because a view
/// that returns nil from hitTest is skipped when AppKit looks for a drag
/// destination; it forwards the click so the popover still toggles.
///
/// VERIFY on macOS 14, 15 and 26 (stack-decision 3.7): one report says the
/// subview never sees drags while the button's window does. If so, set
/// `StatusItemController.useButtonDropTarget = true`.
final class StatusItemDropView: NSView {
    weak var handler: DropTargetHandler?
    var onClick: (() -> Void)?

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        registerForDraggedTypes([.fileURL])
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        registerForDraggedTypes([.fileURL])
    }

    override func mouseDown(with event: NSEvent) {
        onClick?()
    }

    // Right-clicks at the top edge no longer fire on macOS 26 (3.7); treat any
    // right-click we do get like a left click.
    override func rightMouseDown(with event: NSEvent) {
        onClick?()
    }

    override func draggingEntered(_ sender: NSDraggingInfo) -> NSDragOperation {
        guard DraggedFiles.hasFiles(sender) else { return [] }
        return handler?.dragEntered() ?? []
    }

    override func draggingUpdated(_ sender: NSDraggingInfo) -> NSDragOperation {
        DraggedFiles.hasFiles(sender) ? .copy : []
    }

    override func draggingExited(_ sender: NSDraggingInfo?) {
        handler?.dragExited()
    }

    override func prepareForDragOperation(_ sender: NSDraggingInfo) -> Bool {
        DraggedFiles.hasFiles(sender)
    }

    override func performDragOperation(_ sender: NSDraggingInfo) -> Bool {
        handler?.performDrop(urls: DraggedFiles.urls(from: sender)) ?? false
    }
}
