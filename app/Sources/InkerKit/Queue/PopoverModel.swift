import Foundation

/// Which of the four boards the popover shows.
public enum PopoverMode: Equatable, Sendable {
    case empty
    case processing
    case attention
    case ready
}

/// Everything the popover draws, derived from backend records.
public struct PopoverModel: Equatable, Sendable {
    public var mode: PopoverMode
    /// Docs that need the user: the header badge and the menu bar title.
    public var badgeCount: Int
    public var groups: [GroupModel]
    public var footer: FooterModel
    /// Empty board subtitle, before the mono `🐙/` ("They move into ~/Private. You get a ").
    public var emptySubtitlePrefix: String
    /// "🐙/" or "ink/".
    public var handlePrefix: String
}

/// A dropped folder (or a single dropped file).
public struct GroupModel: Equatable, Sendable, Identifiable {
    public var id: String
    /// What Copy puts on the clipboard: "🐙/taxes-2025".
    public var handle: String
    /// "4 files · 1 ready · 3 inking".
    public var summaryLine: String
    public var isFolder: Bool
    /// Folders and ready single files draw the handle header. A single file
    /// still in flight is just its row, so it is never listed twice.
    public var showsHeader: Bool
    /// Decision 5.5: a folder copies once at least one child is ready.
    public var copyEnabled: Bool
    public var rows: [RowModel]
    /// The held document, shown as the attention card.
    public var attention: AttentionModel?
}

public struct RowModel: Equatable, Sendable, Identifiable {
    public enum Kind: Equatable, Sendable {
        case ready
        case processing(InkLoaderMode)
        case failed
    }

    public var id: String
    public var kind: Kind
    /// Ready: the handle ("🐙/w2-acme-2025"). Processing: the file name.
    public var title: String
    /// "W-2 · 6 inked", "reading", "ready".
    public var detail: String
    /// The handle to copy.
    public var handle: String
    public var copyEnabled: Bool
}

/// The attention card (AppPopAttention). Never holds the value.
public struct AttentionModel: Equatable, Sendable {
    public var docID: String
    public var questionID: String
    public var fileName: String
    /// The handle the file will get once answered, in muted mono under the name.
    public var handle: String
    /// For "Open file", which reveals the original in Finder (decision 5.9).
    public var filePath: String
    public var location: String
    public var prompt: String
    public var contextBefore: String
    public var valueLength: Int
    public var footnote: String
    /// The page (from 0) and the byte span in its text, for the page crop.
    /// The crop needs the file; without a span the card keeps the octopus.
    public var pageIndex: Int
    public var span: Range<Int>?
}

public enum FooterLeft: Equatable, Sendable {
    case text(String)
    /// "Claude Code read 🐙/w2-acme-2025, saw only ink · 2m"
    case lastRead(agent: String, handle: String, ago: String)
}

public struct FooterModel: Equatable, Sendable {
    public var left: FooterLeft
    /// "30 files · MCP running"; just "MCP running" with nothing filed.
    public var right: String
    /// The 24x24 peeker eye on the Ready board.
    public var showsPeeker: Bool
}
