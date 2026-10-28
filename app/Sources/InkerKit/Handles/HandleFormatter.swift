import Foundation
import InkerCore

/// Codename to clipboard handle. Mirrors `inker_core::Handle::render` so the
/// queue can label rows without a call per row; the clipboard uses the same.
public enum HandleFormatter {
    public static let octopusPrefix = "🐙/"
    public static let asciiPrefix = "ink/"

    /// Same rule as `Codename::parse`: 1 to 64 of a-z, 0-9 and single hyphens,
    /// no hyphen at either end.
    public static func isValidCodename(_ s: String) -> Bool {
        let bytes = Array(s.utf8)
        guard (1...64).contains(bytes.count) else { return false }
        for b in bytes {
            let ok = (b >= 0x61 && b <= 0x7A) || (b >= 0x30 && b <= 0x39) || b == 0x2D
            if !ok { return false }
        }
        if bytes.first == 0x2D || bytes.last == 0x2D { return false }
        return !s.contains("--")
    }

    public static func prefix(_ style: HandleStyle) -> String {
        switch style {
        case .octopus: return octopusPrefix
        case .ascii: return asciiPrefix
        }
    }

    public static func render(codename: String, style: HandleStyle) -> String? {
        guard isValidCodename(codename) else { return nil }
        return prefix(style) + codename
    }

    /// For settings storage.
    public static func name(of style: HandleStyle) -> String {
        switch style {
        case .octopus: return "octopus"
        case .ascii: return "ascii"
        }
    }

    public static func style(named name: String?) -> HandleStyle {
        name == "ascii" ? .ascii : .octopus
    }
}
