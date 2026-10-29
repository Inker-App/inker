import Testing
@testable import InkerKit

/// A menu bar app is invisible to Cmd+Tab. While one of its windows is open
/// it joins the switcher, and leaves again when the last one closes.
struct WindowPresenceTests {
    @Test func joinsTheSwitcherForTheFirstWindowOnly() {
        var p = WindowPresence()
        #expect(p.policy == .accessory)
        #expect(p.opened("onboarding") == .regular)
        #expect(p.opened("settings") == nil, "already regular")
        #expect(p.opened("settings") == nil, "opening twice is harmless")
        #expect(p.policy == .regular)
    }

    @Test func leavesWhenTheLastWindowCloses() {
        var p = WindowPresence()
        _ = p.opened("onboarding")
        _ = p.opened("settings")
        #expect(p.closed("onboarding") == nil, "settings is still open")
        #expect(p.closed("settings") == .accessory)
        #expect(p.closed("settings") == nil, "closing twice is harmless")
        #expect(p.policy == .accessory)
    }
}
