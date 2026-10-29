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

    public static let body = "Moving it leaves nothing in Downloads to drag into a chat by mistake."
    public static let alwaysMove = "Always move, don't ask again"
    public static let copyInstead = "Copy instead"
    /// Not on the board: the card needs a way out now that it is not a window.
    public static let cancel = "Cancel"
    public static let move = "Move"
}
