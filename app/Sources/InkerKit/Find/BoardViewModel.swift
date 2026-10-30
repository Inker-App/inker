import Foundation
import InkerCore

/// Which Find hit a row carries, and the row menu. The ready board at any
/// size (AppFindScale): year groups, folder sub-groups and tag chips. Pure:
/// no AppKit, no FFI calls.

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
    /// "W-2 · Bluefin Analytics · 6 inked", or the status note while in flight.
    public var detail: String
    public var state: RowState
    public var copyEnabled: Bool
    public var path: String
    /// Inside an open folder: 12 pt further in.
    public var indented: Bool
    public var menu: RowMenuModel
}

/// A dropped folder: its name and count over the folder handle.
public struct FolderRowModel: Equatable, Sendable, Identifiable {
    public var id: String
    public var name: String
    /// "· 9 files"
    public var countLine: String
    public var handle: String
    /// "all ready · 41 inked"
    public var summary: String
    public var copyEnabled: Bool
    public var isOpen: Bool
    public var children: [DocRowModel]
    public var path: String
    public var menu: RowMenuModel
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
}

/// `2025 · 14 FILES` with its chevron.
public struct YearGroupModel: Equatable, Sendable, Identifiable {
    /// "2025" or "undated".
    public var id: String
    public var label: String
    public var isOpen: Bool
    public var rows: [BoardRow]
}

/// `All`, `Taxes · 24`.
public struct ChipModel: Equatable, Sendable, Identifiable {
    public var id: String
    public var title: String
    /// nil for `All`.
    public var tag: String?
    public var isActive: Bool
}

public struct BoardModel: Equatable, Sendable {
    public var groups: [YearGroupModel]
    public var chips: [ChipModel]
    /// The Find field only appears above eight files.
    public var showsFind: Bool
    public var total: Int

    public static let empty = BoardModel(groups: [], chips: [], showsFind: false, total: 0)
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

    public static func yearID(_ year: UInt16?) -> String {
        year.map { String($0) } ?? "undated"
    }

    public static func build(board: Board, style: HandleStyle, showPayer: Bool, activeTag: String?,
                             isYearOpen: (String) -> Bool, isFolderOpen: (String) -> Bool) -> BoardModel {
        let groups = board.groups.map { g -> YearGroupModel in
            let id = yearID(g.year)
            let head = g.year.map { String($0) } ?? "UNDATED"
            let files = g.files == 1 ? "1 FILE" : "\(g.files) FILES"
            let rows = g.entries.map { entry -> BoardRow in
                switch entry {
                case .doc(let item):
                    return .doc(docRow(item, style: style, showPayer: showPayer, indented: false))
                case .folder(let item):
                    return .folder(folderRow(item, style: style, showPayer: showPayer, isOpen: isFolderOpen(item.id)))
                }
            }
            return YearGroupModel(id: id, label: "\(head) · \(files)", isOpen: isYearOpen(id), rows: rows)
        }
        var chips: [ChipModel] = []
        if !board.tags.isEmpty {
            chips.append(ChipModel(id: "all", title: "All", tag: nil, isActive: activeTag == nil))
            for t in board.tags {
                chips.append(ChipModel(id: t.tag, title: "\(t.tag) · \(t.count)", tag: t.tag, isActive: activeTag == t.tag))
            }
        }
        return BoardModel(groups: groups, chips: chips, showsFind: Int(board.total) > findThreshold, total: Int(board.total))
    }

    public static func docRow(_ d: DocItem, style: HandleStyle, showPayer: Bool, indented: Bool) -> DocRowModel {
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
                           copyEnabled: d.state == .ready, path: d.path, indented: indented, menu: menu)
    }

    /// "W-2 · Acme Robotics · 6 inked"; the payer only when shown and known.
    public static func readyDetail(_ d: DocItem, showPayer: Bool) -> String {
        var parts = [d.kindLabel]
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

    static func folderRow(_ f: FolderItem, style: HandleStyle, showPayer: Bool, isOpen: Bool) -> FolderRowModel {
        let handle = HandleFormatter.render(codename: f.codename, style: style) ?? f.codename
        let children = f.children.map { docRow($0, style: style, showPayer: showPayer, indented: true) }
        let count = f.summary.total == 1 ? "· 1 file" : "· \(f.summary.total) files"
        return FolderRowModel(id: f.id, name: f.name, countLine: count, handle: handle,
                              summary: SummaryLine.folderDetail(f.summary), copyEnabled: f.summary.ready > 0,
                              isOpen: isOpen, children: children, path: f.path,
                              menu: RowMenuModel(codename: f.codename, isFolder: true))
    }
}
