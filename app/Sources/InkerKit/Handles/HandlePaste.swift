import Foundation
import InkerCore

/// What Copy puts on the clipboard. With the note on (the default), the
/// handle is followed by a short parenthetical for the chat, because Claude
/// Desktop may not have INKER's tools in view when a bare handle arrives and
/// then goes looking for a 🐙 folder on disk (stack-decision.md section 10).
/// The wording and the default are Ben's to change.
public enum HandlePaste {
    public static let note = "(INKER handle: read it with the INKER tool, not from disk)"

    /// The handle the Settings example line uses. Synthetic.
    public static let exampleCodename = "w2-acme-2025"

    /// `🐙/x (INKER handle: …)` with the note, `🐙/x` without. One line, no
    /// trailing newline, the handle first.
    public static func text(handle: String, withNote: Bool) -> String {
        withNote ? handle + " " + note : handle
    }

    /// The muted example under the Settings toggle: what a paste looks like.
    public static func example(style: HandleStyle, withNote: Bool) -> String {
        text(handle: HandleFormatter.prefix(style) + exampleCodename, withNote: withNote)
    }

    /// `text` without INKER's own note, when it ends with it; nil otherwise.
    static func strippingNote(_ text: String) -> String? {
        let suffix = " " + note
        guard text.hasSuffix(suffix) else { return nil }
        return String(text.dropLast(suffix.count))
    }
}
