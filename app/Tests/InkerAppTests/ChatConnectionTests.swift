import Testing
@testable import InkerKit

/// "Claude connected" means a live session, not a config entry.
struct ChatConnectionTests {
    @Test func aLiveSessionIsConnectedWhateverTheConfigSays() {
        #expect(ChatConnection.state(registered: true, clients: ["Claude"]) == .connected)
        #expect(ChatConnection.state(registered: false, clients: ["Claude", "Claude Code"]) == .connected)
    }

    @Test func aConfigEntryWithoutASessionIsOnlySetUp() {
        #expect(ChatConnection.state(registered: true, clients: []) == .setUp)
        #expect(ChatConnection.state(registered: true, clients: ["Claude Code"]) == .setUp, "a coding agent is not the Claude app")
        #expect(ChatConnection.state(registered: false, clients: []) == .notSetUp)
    }

    @Test func theFooterAndTheSettingsRowSayIt() {
        #expect(ChatConnection.footerText(clients: ["Claude"]) == "Claude connected")
        #expect(ChatConnection.footerText(clients: []) == "Claude not connected")
        #expect(ChatConnection.rowLabel(.notSetUp) == nil, "the row shows Connect")
        #expect(ChatConnection.rowLabel(.setUp) == "Set up · restart Claude")
        #expect(ChatConnection.rowLabel(.connected) == "Connected")
    }
}
