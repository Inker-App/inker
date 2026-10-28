import Foundation

/// Turns whatever was dropped (on the status item or on the popover) into the
/// list to ingest: file URLs only, standardised, de-duplicated, no hidden files
/// (.DS_Store), no missing files, and nothing that sits inside a folder that
/// was dropped too.
public enum DropCollector {
    public static func collect(_ dropped: [URL], home: URL, privateFolder: URL,
                               exists: (URL) -> Bool, isDirectory: (URL) -> Bool = { _ in false }) -> DropBatch {
        var seen = Set<String>()
        var kept: [URL] = []
        for raw in dropped where raw.isFileURL {
            let url = raw.standardizedFileURL
            if url.lastPathComponent.hasPrefix(".") { continue }
            if !exists(url) { continue }
            if seen.insert(url.path).inserted { kept.append(url) }
        }
        let top = kept.filter { candidate in
            !kept.contains { other in other != candidate && PathDisplay.isInside(candidate, other) }
        }
        let parents = Set(top.map { $0.deletingLastPathComponent().path })
        var source: String?
        if parents.count == 1, let first = top.first {
            source = PathDisplay.abbreviate(first.deletingLastPathComponent(), home: home)
        }
        let needsMove = top.contains { !PathDisplay.isInside($0, privateFolder) }
        let singleFolder = top.count == 1 && isDirectory(top[0])
        return DropBatch(urls: top, sourceDisplay: source, needsMoveDecision: needsMove, isSingleFolder: singleFolder)
    }
}
