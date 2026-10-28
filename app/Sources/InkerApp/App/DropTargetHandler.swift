import AppKit

/// What both status-item drop paths do with a drag. Owned by the controller.
@MainActor
protocol DropTargetHandler: AnyObject {
    func dragEntered() -> NSDragOperation
    func dragExited()
    func performDrop(urls: [URL]) -> Bool
}

enum DraggedFiles {
    /// File URLs on a dragging pasteboard.
    static func urls(from info: NSDraggingInfo) -> [URL] {
        let options: [NSPasteboard.ReadingOptionKey: Any] = [.urlReadingFileURLsOnly: true]
        return info.draggingPasteboard.readObjects(forClasses: [NSURL.self], options: options) as? [URL] ?? []
    }

    static func hasFiles(_ info: NSDraggingInfo) -> Bool {
        info.draggingPasteboard.canReadObject(forClasses: [NSURL.self], options: [.urlReadingFileURLsOnly: true])
    }
}
