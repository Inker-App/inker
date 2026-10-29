import Foundation
import InkerCore
import Testing
@testable import InkerKit

/// Onboarding step 2, "Your chat asks INKER": the chat apps, Claude first.
struct ChatRowsTests {
    @Test func rowsLeadWithClaudeAndNeverNameACodingAgent() {
        let rows = ChatRows.connect
        #expect(rows.map(\.title) == ["Claude", "ChatGPT", "Something else"])
        #expect(rows.map(\.action) == [.connect(.claudeDesktop), .comingSoon, .copyConfig])
        for r in rows {
            for banned in ["Claude Code", "Cursor", "Codex"] {
                #expect(!r.label.contains(banned), "\(r.label)")
            }
        }
    }

    @Test func connectedLabelSaysToRestartTheApp() {
        #expect(ChatRows.connectedLabel(for: .claudeDesktop) == "Connected · restart Claude")
        #expect(ChatRows.failedLabel == "Couldn't write the config")
    }

    @Test func genericConfigPointsAtTheBundledCLI() throws {
        let bin = "/Applications/INKER.app/Contents/MacOS/inker-cli"
        let json = AgentConfigSnippet.mcpServers(binary: bin)
        let obj = try JSONSerialization.jsonObject(with: Data(json.utf8)) as? [String: Any]
        let servers = obj?["mcpServers"] as? [String: Any]
        let inker = servers?["inker"] as? [String: Any]
        #expect(inker?["command"] as? String == bin)
        #expect(inker?["args"] as? [String] == ["mcp"])
    }

    @Test func cliPathSitsNextToTheAppExecutable() {
        let exe = URL(fileURLWithPath: "/Applications/INKER.app/Contents/MacOS/INKER")
        #expect(AgentConfigSnippet.cliPath(executable: exe) == "/Applications/INKER.app/Contents/MacOS/inker-cli")
        #expect(AgentConfigSnippet.cliPath(executable: nil) == "/Applications/INKER.app/Contents/MacOS/inker-cli")
    }
}
