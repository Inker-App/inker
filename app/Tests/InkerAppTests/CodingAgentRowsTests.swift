import Foundation
import InkerCore
import Testing
@testable import InkerKit

/// The settings section for people who also run a coding agent (decision
/// 5.3 / section 3.8). Not part of onboarding any more; Cursor is not offered.
struct CodingAgentRowsTests {
    @Test func rowsReadPerDecision5_3() {
        let rows = CodingAgentRows.keepOut
        #expect(rows.map(\.title) == ["Claude Code", "Codex"])
        #expect(rows.map(\.detail) == ["permission rule, sandbox and hook",
                                        "permission profile (replaces your sandbox mode)"])
        #expect(rows.map(\.agent) == [.claudeCode, .codex])
        #expect(CodingAgentRows.sectionTitle == "Also use a coding agent?")
    }

    @Test func noRowCallsItADenyRule() {
        for r in CodingAgentRows.keepOut {
            #expect(!r.label.lowercased().contains("deny rule"))
            #expect(!r.accessibilityLabel.lowercased().contains("deny rule"))
        }
        #expect(!CodingAgentRows.footnote.lowercased().contains("deny rule"))
    }

    @Test func doneLabelsReflectWhetherAnythingChanged() {
        #expect(CodingAgentRows.doneLabel(changed: true) == "Kept out")
        #expect(CodingAgentRows.doneLabel(changed: false) == "Already kept out")
    }
}
