import Foundation

/// The MCP entry any client can use, and where the CLI lives.
public enum AgentConfigSnippet {
    public static let defaultCLIPath = "/Applications/INKER.app/Contents/MacOS/inker-cli"

    /// The `inker-cli` next to the app executable. The release workflow puts
    /// it there; a `swift run` build falls back to the installed app's copy.
    public static func cliPath(executable: URL?) -> String {
        guard let executable else { return defaultCLIPath }
        return executable.deletingLastPathComponent().appendingPathComponent("inker-cli").path
    }

    /// A generic `mcpServers` block for "Something else" (Copy config).
    public static func mcpServers(binary: String) -> String {
        let quoted = binary
            .replacingOccurrences(of: "\\", with: "\\\\")
            .replacingOccurrences(of: "\"", with: "\\\"")
        return """
        {
          "mcpServers": {
            "inker": { "command": "\(quoted)", "args": ["mcp"] }
          }
        }
        """
    }
}
