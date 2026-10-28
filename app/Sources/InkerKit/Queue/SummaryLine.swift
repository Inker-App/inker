import Foundation
import InkerCore

/// "4 files · 1 ready · 3 inking", "4 files · 3 ready · 1 needs you",
/// "4 files · all ready · 23 inked".
public enum SummaryLine {
    public static func text(_ s: FolderSummary) -> String {
        var parts = [s.total == 1 ? "1 file" : "\(s.total) files"]
        if s.total > 0 && s.ready == s.total {
            parts.append("all ready")
            parts.append("\(s.inked) inked")
            return parts.joined(separator: " · ")
        }
        parts.append("\(s.ready) ready")
        if s.inking > 0 { parts.append("\(s.inking) inking") }
        if s.needsAttention > 0 { parts.append("\(s.needsAttention) needs you") }
        if s.failed > 0 { parts.append("\(s.failed) failed") }
        return parts.joined(separator: " · ")
    }

    /// Row subtitle: "W-2 · 6 inked".
    public static func docDetail(kind: String, inked: UInt32) -> String {
        "\(kind) · \(inked) inked"
    }
}
