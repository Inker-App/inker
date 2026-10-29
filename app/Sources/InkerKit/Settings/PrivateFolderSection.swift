import Foundation

/// The "Private folder" block in Settings: where the folder is, and a button
/// that opens it in Finder. Resolution is the one rule the whole app uses:
/// the stored path if there is one, else `~/Private`.
public enum PrivateFolderSection {
    public static let title = "Private folder"
    public static let openButton = "Open in Finder"

    public static func folder(stored: String?, home: URL) -> URL {
        guard let stored, !stored.isEmpty else { return FolderPickerWarnings.defaultFolder(home: home) }
        return PathDisplay.expand(stored, home: home)
    }

    /// `~/Private`, as on the onboarding step and the empty board.
    public static func display(stored: String?, home: URL) -> String {
        PathDisplay.abbreviate(folder(stored: stored, home: home), home: home)
    }
}
