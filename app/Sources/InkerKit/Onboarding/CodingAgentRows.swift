import Foundation
import InkerCore

/// A row in the settings section for coding agents, per decision 5.3. None of
/// these is a "deny rule": Codex has none.
public struct CodingAgentRow: Equatable, Sendable, Identifiable {
    public let agent: AgentKind
    public let title: String
    public let detail: String

    public var id: String { title }
    public var label: String { "\(title) · \(detail)" }
    public var accessibilityLabel: String { "Keep \(title) out with a \(detail)" }
}

public enum CodingAgentRows {
    /// Placeholder copy for Ben: the section is for people who also run a
    /// coding agent on this Mac. Cursor is supported by the CLI only.
    public static let sectionTitle = "Also use a coding agent?"

    public static let keepOut: [CodingAgentRow] = [
        CodingAgentRow(agent: .claudeCode, title: "Claude Code", detail: "permission rule, sandbox and hook"),
        CodingAgentRow(agent: .codex, title: "Codex", detail: "permission profile (replaces your sandbox mode)"),
    ]

    /// Replaces the board's "A deny rule is a polite request." Placeholder copy for Ben.
    public static let footnote = "These settings are a polite request. The folder being out of reach does the real work."

    public static func doneLabel(changed: Bool) -> String {
        changed ? "Kept out" : "Already kept out"
    }
}
