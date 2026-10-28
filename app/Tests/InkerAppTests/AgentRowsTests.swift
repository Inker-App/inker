import Foundation
import InkerCore
import Testing
@testable import InkerKit

/// "Keep these agents out" (decision 5.3 / section 3.8).
struct AgentRowsTests {
    @Test func rowsReadPerDecision5_3() {
        let rows = AgentRows.keepOut
        #expect(rows.map(\.title) == ["Claude Code", "Codex", "Cursor"])
        #expect(rows.map(\.detail) == ["permission rule, sandbox and hook",
                                        "permission profile (replaces your sandbox mode)",
                                        "read hook"])
        #expect(rows.map(\.label) == ["Claude Code · permission rule, sandbox and hook",
                                       "Codex · permission profile (replaces your sandbox mode)",
                                       "Cursor · read hook"])
        #expect(rows.map(\.agent) == [.claudeCode, .codex, .cursor])
    }

    @Test func noRowCallsItADenyRule() {
        for r in AgentRows.keepOut {
            #expect(!r.label.lowercased().contains("deny rule"))
            #expect(!r.accessibilityLabel.lowercased().contains("deny rule"))
        }
        #expect(!AgentRows.footnote.lowercased().contains("deny rule"))
    }
}
