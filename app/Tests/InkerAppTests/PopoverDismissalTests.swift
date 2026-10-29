import Testing
@testable import InkerKit

/// Escape and a click outside the popover close it. A pinned popover (the
/// folder chooser is up, or a drag is in flight) ignores both.
struct PopoverDismissalTests {
    @Test func escapeCloses() {
        #expect(PopoverDismissal.closes(on: .escape, pinned: false))
    }

    @Test func aClickOutsideCloses() {
        #expect(PopoverDismissal.closes(on: .mouseDown(insidePopover: false), pinned: false))
    }

    @Test func aClickInsideStays() {
        #expect(!PopoverDismissal.closes(on: .mouseDown(insidePopover: true), pinned: false))
    }

    @Test func aPinnedPopoverIgnoresBoth() {
        #expect(!PopoverDismissal.closes(on: .escape, pinned: true))
        #expect(!PopoverDismissal.closes(on: .mouseDown(insidePopover: false), pinned: true))
    }

    @Test func otherKeysAreNotDismissals() {
        #expect(PopoverDismissal.Event(keyCode: 53) == .escape)
        #expect(PopoverDismissal.Event(keyCode: 36) == nil, "return")
        #expect(PopoverDismissal.Event(keyCode: 0) == nil, "a")
    }
}
