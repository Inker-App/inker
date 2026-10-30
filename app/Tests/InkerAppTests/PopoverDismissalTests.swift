import Testing
@testable import InkerKit

/// Escape closes the popover; a click anywhere else leaves it up, so the
/// user can click around Finder and drag files in. A pinned popover (the
/// folder chooser is up) ignores Escape too.
struct PopoverDismissalTests {
    @Test func escapeCloses() {
        #expect(PopoverDismissal.closes(on: .escape, pinned: false))
    }

    @Test func aClickAnywhereLeavesItUp() {
        #expect(!PopoverDismissal.closes(on: .mouseDown(insidePopover: false), pinned: false), "clicking around Finder")
        #expect(!PopoverDismissal.closes(on: .mouseDown(insidePopover: true), pinned: false))
    }

    @Test func aPinnedPopoverIgnoresEscape() {
        #expect(!PopoverDismissal.closes(on: .escape, pinned: true))
    }

    @Test func escapeClosesAnOpenRowMenuBeforeThePopover() {
        #expect(PopoverDismissal.outcome(on: .escape, pinned: false, menuOpen: true) == .closeMenu)
        #expect(PopoverDismissal.outcome(on: .escape, pinned: false, menuOpen: false) == .closePopover)
        #expect(PopoverDismissal.outcome(on: .mouseDown(insidePopover: false), pinned: false, menuOpen: true) == .nothing)
        #expect(PopoverDismissal.outcome(on: .escape, pinned: true, menuOpen: true) == .nothing)
    }

    @Test func otherKeysAreNotDismissals() {
        #expect(PopoverDismissal.Event(keyCode: 53) == .escape)
        #expect(PopoverDismissal.Event(keyCode: 36) == nil, "return")
        #expect(PopoverDismissal.Event(keyCode: 0) == nil, "a")
    }
}
