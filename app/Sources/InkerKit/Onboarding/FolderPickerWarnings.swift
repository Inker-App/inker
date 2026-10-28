import Foundation

/// The checks behind the folder picker in AppOnboard1.
public enum FolderPickerWarnings {
    public static let agentFolders = [".claude", ".cursor", ".codex"]

    public static func defaultFolder(home: URL) -> URL {
        home.appendingPathComponent("Private")
    }

    /// `containsGitDirectory(dir)` answers whether `dir/.git` exists; it is
    /// asked for the folder and each ancestor.
    public static func warnings(for folder: URL, home: URL, containsGitDirectory: (URL) -> Bool) -> [FolderWarning] {
        let url = folder.standardizedFileURL
        var out: [FolderWarning] = []
        for name in ["Desktop", "Documents"] {
            let synced = home.appendingPathComponent(name)
            if PathDisplay.isInside(url, synced) {
                out.append(.iCloudSynced("~/" + name))
            }
        }
        if let agent = url.pathComponents.first(where: { agentFolders.contains($0) }) {
            out.append(.agentWorkspace(agent))
        }
        var dir = url
        while dir.pathComponents.count > 1 {
            if containsGitDirectory(dir) {
                out.append(.gitRepository(PathDisplay.abbreviate(dir, home: home)))
                break
            }
            dir = dir.deletingLastPathComponent().standardizedFileURL
        }
        return out
    }

    /// The real filesystem check, for the app.
    public static func gitCheck(_ dir: URL) -> Bool {
        FileManager.default.fileExists(atPath: dir.appendingPathComponent(".git").path)
    }
}
