/// Whether the Claude app is talking to INKER right now. "Connected" means
/// a live MCP session (Claude is running with INKER loaded), not merely
/// that setup wrote INKER into Claude's config (Ben, 2026-10-30).
public enum ChatConnection {
    /// The product name the MCP server reports for the Claude app.
    public static let claude = "Claude"

    public enum State: Equatable, Sendable {
        /// INKER is not in Claude's config.
        case notSetUp
        /// In the config, but no session: Claude is closed or not restarted.
        case setUp
        /// A live session.
        case connected
    }

    public static func state(registered: Bool, clients: [String]) -> State {
        if clients.contains(claude) { return .connected }
        return registered ? .setUp : .notSetUp
    }

    public static let connectedText = "Claude connected"
    public static let notConnectedText = "Claude not connected"

    /// The footer's right side, in place of "MCP running".
    public static func footerText(clients: [String]) -> String {
        clients.contains(claude) ? connectedText : notConnectedText
    }

    /// The Settings row's trailing text; nil shows the Connect button.
    public static func rowLabel(_ state: State) -> String? {
        switch state {
        case .notSetUp: return nil
        case .setUp: return "Set up · restart Claude"
        case .connected: return "Connected"
        }
    }
}
