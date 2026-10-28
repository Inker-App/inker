import Foundation

/// Why a private-folder choice is risky (decision 5.15).
public enum FolderWarning: Equatable, Sendable {
    /// Under ~/Desktop or ~/Documents, which iCloud may sync.
    case iCloudSynced(String)
    /// Inside an agent's own config folder (.claude, .cursor, .codex).
    case agentWorkspace(String)
    /// Inside a git repository, which is usually somebody's agent workspace.
    case gitRepository(String)

    /// Short warning under the folder field. Copy is a placeholder for Ben.
    public var message: String {
        switch self {
        case .iCloudSynced(let place):
            return "\(place) can sync to iCloud. Pick a folder outside it."
        case .agentWorkspace(let name):
            return "This is inside \(name), where an agent keeps its own files."
        case .gitRepository(let root):
            return "This is inside the repository at \(root). Agents working there can read it."
        }
    }
}
