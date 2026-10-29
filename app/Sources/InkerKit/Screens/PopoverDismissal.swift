/// When the popover closes on its own: Escape, or a mouse-down that lands
/// outside it. StatusItemController watches the events; this decides. A
/// pinned popover (folder chooser up, drag in flight) ignores both.
public enum PopoverDismissal {
    public enum Event: Equatable {
        case escape
        case mouseDown(insidePopover: Bool)

        /// The Escape key by its virtual key code; nil for any other key.
        public init?(keyCode: UInt16) {
            guard keyCode == 53 else { return nil }
            self = .escape
        }
    }

    public static func closes(on event: Event, pinned: Bool) -> Bool {
        if pinned { return false }
        switch event {
        case .escape: return true
        case .mouseDown(let inside): return !inside
        }
    }
}
