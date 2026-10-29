import Foundation
import InkerCore

/// Find results (AppFindSearch): a flat list under "N OF M MATCH".
public struct FindRowModel: Equatable, Sendable, Identifiable {
    /// Where the matched text is underlined.
    public enum Highlight: Equatable, Sendable {
        case name
        case handle
        case detail
        case snippet
    }

    public var id: String
    public var row: DocRowModel
    public var matched: String
    public var highlight: Highlight
    /// A text hit's line, a folder hit's folder name or a tag hit's tag.
    public var snippet: String?
    /// True when the snippet is inked text (mono); false for a folder or tag.
    public var snippetIsInkedText: Bool
    public var exact: Bool
}

public struct FindResultsModel: Equatable, Sendable {
    public var header: String
    public var rows: [FindRowModel]
    public static let clear = "Clear"
    /// The one document whose handle the query was, exactly: the located row.
    public var exactMatchID: String?

    public static let none = FindResultsModel(header: "", rows: [], exactMatchID: nil)
}

public enum FindViewModel {
    public static func header(found: Int, total: Int) -> String {
        "\(found) OF \(total) \(found == 1 ? "MATCHES" : "MATCH")"
    }

    public static func build(matches: [FindMatch], total: Int, style: HandleStyle, showPayer: Bool) -> FindResultsModel {
        let rows = matches.map { m -> FindRowModel in
            let row = BoardViewModel.docRow(m.doc, style: style, showPayer: showPayer, indented: false)
            let highlight: FindRowModel.Highlight
            var snippet: String?
            var inked = false
            switch m.field {
            case .fileName:
                highlight = .name
            case .handle:
                highlight = .handle
            case .kind, .payer, .year:
                highlight = .detail
            case .text:
                highlight = .snippet
                snippet = m.snippet
                inked = true
            case .folder:
                highlight = .snippet
                snippet = m.doc.folderName
            case .tag:
                highlight = .snippet
                snippet = m.doc.tag
            }
            return FindRowModel(id: m.doc.id, row: row, matched: m.matched, highlight: highlight, snippet: snippet,
                                snippetIsInkedText: inked, exact: m.exactHandle)
        }
        let exact = rows.count == 1 && rows[0].exact ? rows[0].id : nil
        return FindResultsModel(header: header(found: rows.count, total: total), rows: rows, exactMatchID: exact)
    }
}
