import Foundation

/// The clipboard chip: "Your clipboard holds 🐙/w2-2025-2 · Find it". Only
/// a handle on its own, with a prefix, counts; the core then confirms the
/// app holds it (`locate`). The pasteboard is read when the popover opens
/// and at no other time.
public enum ClipboardHandle {
    public static let chipButton = "Find it"

    /// The text before the mono handle.
    public static let chipText = "Your clipboard holds "

    /// The trimmed handle when `text` is exactly `🐙/<codename>` or
    /// `ink/<codename>`; nil otherwise.
    public static func parse(_ text: String?) -> String? {
        guard let text else { return nil }
        let t = text.trimmingCharacters(in: .whitespacesAndNewlines)
        for prefix in [HandleFormatter.octopusPrefix, HandleFormatter.asciiPrefix] where t.hasPrefix(prefix) {
            let codename = String(t.dropFirst(prefix.count))
            return HandleFormatter.isValidCodename(codename) ? t : nil
        }
        return nil
    }
}
