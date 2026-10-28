import Foundation

/// `~`-abbreviated paths, the way the boards write them.
public enum PathDisplay {
    public static func abbreviate(_ url: URL, home: URL) -> String {
        let path = url.standardizedFileURL.path
        let h = home.standardizedFileURL.path
        if path == h { return "~" }
        let prefix = h.hasSuffix("/") ? h : h + "/"
        if path.hasPrefix(prefix) {
            return "~/" + path.dropFirst(prefix.count)
        }
        return path
    }

    public static func expand(_ path: String, home: URL) -> URL {
        if path == "~" { return home }
        if path.hasPrefix("~/") {
            return home.appendingPathComponent(String(path.dropFirst(2)))
        }
        return URL(fileURLWithPath: path)
    }

    /// True when `url` is `ancestor` or inside it (component-wise, not a string prefix).
    public static func isInside(_ url: URL, _ ancestor: URL) -> Bool {
        let a = ancestor.standardizedFileURL.pathComponents
        let p = url.standardizedFileURL.pathComponents
        return p.count >= a.count && Array(p.prefix(a.count)) == a
    }
}
