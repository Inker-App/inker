import Foundation

/// The clipboard chip: "Your clipboard holds 🐙/w2-2025-2 · Find it". Only
/// a handle on its own (or with INKER's Copy note), with a prefix, counts; the core then confirms the
/// app holds it (`locate`). The pasteboard is read when the popover opens
/// and at no other time.
public enum ClipboardHandle {
    public static let chipButton = "Find it"

    /// The text before the mono handle.
    public static let chipText = "Your clipboard holds "

    /// The trimmed handle when `text` is exactly `🐙/<codename>` or
    /// `ink/<codename>`, alone or followed by INKER's own Copy note
    /// (`HandlePaste.note`); nil otherwise.
    public static func parse(_ text: String?) -> String? {
        guard let text else { return nil }
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        let t = HandlePaste.strippingNote(trimmed) ?? trimmed
        for prefix in [HandleFormatter.octopusPrefix, HandleFormatter.asciiPrefix] where t.hasPrefix(prefix) {
            let codename = String(t.dropFirst(prefix.count))
            return HandleFormatter.isValidCodename(codename) ? t : nil
        }
        return nil
    }
}
