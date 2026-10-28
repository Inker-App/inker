import Foundation

/// Text for the first-drop dialog (AppDropDialog). The board's title is
/// "Move these 4 files out of ~/Downloads?"; the singular and no-common-folder
/// variants are ours and need Ben's sign-off.
public enum DropDialogCopy {
    public static func title(for batch: DropBatch) -> String {
        let what: String
        if batch.count == 1 {
            what = batch.isSingleFolder ? "this folder" : "this file"
        } else {
            what = "these \(batch.count) files"
        }
        if let source = batch.sourceDisplay {
            return "Move \(what) out of \(source)?"
        }
        return "Move \(what)?"
    }

    public static let body = "Agents can usually read Downloads. Moving leaves nothing behind for them to look at."
    public static let alwaysMove = "Always move, don't ask again"
    public static let copyInstead = "Copy instead"
    public static let move = "Move"
}
