import Foundation
import InkerCore

/// A row in "Keep these agents out" (AppOnboard1), per decision 5.3. None of
/// these is a "deny rule": Cursor and Codex have none.
public struct AgentRow: Equatable, Sendable, Identifiable {
    public let agent: AgentKind
    public let title: String
    public let detail: String

    public var id: String { title }
    public var label: String { "\(title) · \(detail)" }
    public var accessibilityLabel: String { "Keep \(title) out with a \(detail)" }
}

public enum AgentRows {
    public static let keepOut: [AgentRow] = [
        AgentRow(agent: .claudeCode, title: "Claude Code", detail: "permission rule, sandbox and hook"),
        AgentRow(agent: .codex, title: "Codex", detail: "permission profile (replaces your sandbox mode)"),
        AgentRow(agent: .cursor, title: "Cursor", detail: "read hook"),
    ]

    /// Replaces the board's "A deny rule is a polite request." Placeholder copy for Ben.
    public static let footnote = "These settings are a polite request. The folder being out of reach does the real work."
}
