import Foundation
import InkerCore

/// What the trailing control on a chat row does.
public enum ChatAction: Equatable, Sendable {
    /// Write the client's MCP config for real.
    case connect(AgentKind)
    /// The client cannot reach a local server yet.
    case comingSoon
    /// A generic `mcpServers` entry onto the clipboard.
    case copyConfig
}

/// A row in onboarding step 2, "Your chat asks INKER".
public struct ChatRow: Equatable, Sendable, Identifiable {
    public let title: String
    public let detail: String
    public let action: ChatAction

    public var id: String { title }
    public var label: String { "\(title) · \(detail)" }
}

public enum ChatRows {
    /// Chat apps first (stack-decision section 9). No coding agent here.
    public static let connect: [ChatRow] = [
        ChatRow(title: "Claude", detail: "the Claude app on this Mac", action: .connect(.claudeDesktop)),
        ChatRow(title: "ChatGPT", detail: "its desktop app can't reach local servers yet", action: .comingSoon),
        ChatRow(title: "Something else", detail: "any chat that speaks MCP", action: .copyConfig),
    ]

    public static let comingSoonLabel = "Coming soon"
    public static let failedLabel = "Couldn't write the config"

    /// The row's trailing text once the config is written. Chat apps read
    /// their MCP settings at launch, so the restart is part of the message.
    public static func connectedLabel(for agent: AgentKind) -> String {
        switch agent {
        case .claudeDesktop: return "Connected · restart Claude"
        case .claudeCode: return "Connected · restart Claude Code"
        case .codex: return "Connected · restart Codex"
        case .cursor: return "Connected · restart Cursor"
        }
    }
}
