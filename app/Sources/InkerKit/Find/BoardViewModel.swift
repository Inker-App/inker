import Foundation
import InkerCore

/// The row menu, and the ready board as a tree of the private folder: folder
/// rows open or collapsed, files under their original names. Pure: no
/// AppKit, no FFI calls.

/// The `···` menu and the right-click menu, for a file or a folder.
public struct RowMenuModel: Equatable, Sendable {
    public var octopusHandle: String
    public var asciiHandle: String
    public var copyOctopus: String { "Copy \(octopusHandle)" }
    public var copyAscii: String { "Copy \(asciiHandle)" }
    public static let showInFinder = "Show in Finder"
    public static let inkAgain = "Ink again"
    public var showInFinder: String { RowMenuModel.showInFinder }
    public var inkAgain: String { RowMenuModel.inkAgain }
    /// "Forget this file" or "Forget this folder". After one click the
    /// panel shows `RowMenu.areYouSure` in red instead.
    public var forget: String

    public init(codename: String, isFolder: Bool) {
        octopusHandle = HandleFormatter.render(codename: codename, style: .octopus) ?? codename
        asciiHandle = HandleFormatter.render(codename: codename, style: .ascii) ?? codename
        forget = isFolder ? "Forget this folder" : "Forget this file"
    }
}

public enum RowState: Equatable, Sendable {
    case ready
    case processing(InkLoaderMode)
    case failed
}

/// A document row: the file name over the handle and its detail.
public struct DocRowModel: Equatable, Sendable, Identifiable {
    public var id: String
    public var name: String
    /// "🐙/w2-2025-2", in the clipboard style.
    public var handle: String
    /// "W-2 · 2025 · Bluefin Analytics · 6 inked", or the status note while in flight.
    public var detail: String
    public var state: RowState
    public var copyEnabled: Bool
    public var path: String
    /// How many folders down the tree: 0 at the root of the private folder.
    public var depth: Int
    public var menu: RowMenuModel
    /// 12 pt further in per level.
    public var indent: CGFloat { CGFloat(depth) * BoardViewModel.indentStep }
}

/// A folder: chevron, name and count over the folder handle and summary.
/// A folder that is only a path segment (not dropped as a folder) has no
/// handle, so its row is the first line alone, with no Copy and no menu.
public struct FolderRowModel: Equatable, Sendable, Identifiable {
    public var id: String
    public var name: String
    /// "· 9 files", everything under it.
    public var countLine: String
    public var handle: String?
    /// "all ready · 41 inked"
    public var summary: String
    public var copyEnabled: Bool
    public var isOpen: Bool
    /// Inside the private folder; what the open state is remembered by.
    public var relPath: String
    public var depth: Int
    public var path: String
    public var menu: RowMenuModel?
    public var indent: CGFloat { CGFloat(depth) * BoardViewModel.indentStep }
}

public enum BoardRow: Equatable, Sendable, Identifiable {
    case doc(DocRowModel)
    case folder(FolderRowModel)

    public var id: String {
        switch self {
        case .doc(let d): return d.id
        case .folder(let f): return f.id
        }
    }

    public var depth: Int {
        switch self {
        case .doc(let d): return d.depth
        case .folder(let f): return f.depth
        }
    }
}

/// The ready board: the rows of the private folder's tree that show, in
/// order, with the contents of collapsed folders left out.
public struct BoardModel: Equatable, Sendable {
    public var rows: [BoardRow]
    /// The Find field only appears above eight files.
    public var showsFind: Bool
    public var total: Int

    public static let empty = BoardModel(rows: [], showsFind: false, total: 0)
}

public extension BoardEntry {
    /// The document's or the folder's id.
    var entryID: String {
        switch self {
        case .doc(let item): return item.id
        case .folder(let item): return item.id
        }
    }
}

public enum BoardViewModel {
    /// Files above this count get the Find field.
    public static let findThreshold = 8
    /// How far in each level of the tree sits.
    public static let indentStep: CGFloat = 12

    /// The rows in tree order: each folder's sub-folders, then its files.
    /// `isFolderOpen` takes a folder's path inside the private folder.
    public static func build(board: Board, style: HandleStyle, showPayer: Bool,
                             isFolderOpen: (String) -> Bool) -> BoardModel {
        var rows: [BoardRow] = []
        appendContents(of: board.root, depth: 0, style: style, showPayer: showPayer, isFolderOpen: isFolderOpen, into: &rows)
        return BoardModel(rows: rows, showsFind: Int(board.total) > findThreshold, total: Int(board.total))
    }

    private static func appendContents(of folder: FolderItem, depth: Int, style: HandleStyle, showPayer: Bool,
                                       isFolderOpen: (String) -> Bool, into rows: inout [BoardRow]) {
        for sub in folder.folders {
            let open = isFolderOpen(sub.relPath)
            rows.append(.folder(folderRow(sub, style: style, depth: depth, isOpen: open)))
            if open {
                appendContents(of: sub, depth: depth + 1, style: style, showPayer: showPayer,
                               isFolderOpen: isFolderOpen, into: &rows)
            }
        }
        for file in folder.files {
            rows.append(.doc(docRow(file, style: style, showPayer: showPayer, depth: depth)))
        }
    }

    /// The paths of the folders to open so the row `id` shows: every folder
    /// above it and, for a folder, the folder itself. Nil when the board
    /// does not hold it.
    public static func pathsToOpen(for id: String, in board: Board) -> [String]? {
        pathsToOpen(for: id, in: board.root)
    }

    private static func pathsToOpen(for id: String, in folder: FolderItem) -> [String]? {
        if folder.files.contains(where: { $0.id == id }) { return [] }
        for sub in folder.folders {
            if sub.id == id { return [sub.relPath] }
            if let inner = pathsToOpen(for: id, in: sub) { return [sub.relPath] + inner }
        }
        return nil
    }

    public static func docRow(_ d: DocItem, style: HandleStyle, showPayer: Bool, depth: Int) -> DocRowModel {
        let handle = HandleFormatter.render(codename: d.codename, style: style) ?? d.codename
        let menu = RowMenuModel(codename: d.codename, isFolder: false)
        let state: RowState
        let detail: String
        switch d.state {
        case .ready:
            state = .ready
            detail = readyDetail(d, showPayer: showPayer)
        case .failed:
            state = .failed
            detail = d.statusNote ?? "failed"
        case .queued, .reading, .inking, .needsAttention:
            let loader = InkLoaderMode(d.loader) ?? (d.state == .queued ? .queued : .read)
            state = .processing(loader)
            detail = processingDetail(d)
        }
        return DocRowModel(id: d.id, name: d.originalName, handle: handle, detail: detail, state: state,
                           copyEnabled: d.state == .ready, path: d.path, depth: depth, menu: menu)
    }

    /// "W-2 · 2025 · Acme Robotics · 6 inked"; the year when known, the
    /// payer only when shown and known.
    public static func readyDetail(_ d: DocItem, showPayer: Bool) -> String {
        var parts = [d.kindLabel]
        if let year = d.year {
            parts.append(String(year))
        }
        if showPayer, let payer = d.payer, !payer.isEmpty {
            parts.append(payer)
        }
        parts.append("\(d.inked) inked")
        return parts.joined(separator: " · ")
    }

    /// "W-2 · inking" once classified, else just the note, so the row does
    /// not reflow when it finishes.
    static func processingDetail(_ d: DocItem) -> String {
        let note = d.statusNote ?? (d.state == .queued ? "queued" : "")
        if d.kindLabel.isEmpty || d.kindLabel == "document" {
            return note
        }
        return note.isEmpty ? d.kindLabel : "\(d.kindLabel) · \(note)"
    }

    static func folderRow(_ f: FolderItem, style: HandleStyle, depth: Int, isOpen: Bool) -> FolderRowModel {
        let handle = f.codename.map { HandleFormatter.render(codename: $0, style: style) ?? $0 }
        let count = f.summary.total == 1 ? "· 1 file" : "· \(f.summary.total) files"
        return FolderRowModel(id: f.id, name: f.name, countLine: count, handle: handle,
                              summary: SummaryLine.folderDetail(f.summary),
                              copyEnabled: handle != nil && f.summary.ready > 0, isOpen: isOpen,
                              relPath: f.relPath, depth: depth, path: f.path,
                              menu: f.codename.map { RowMenuModel(codename: $0, isFolder: true) })
    }
}
