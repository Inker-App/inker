import Foundation

/// The cleaned-up result of a drop.
public struct DropBatch: Equatable, Sendable {
    /// Top-level items to ingest, in drop order.
    public let urls: [URL]
    /// The shared parent folder as the user reads it ("~/Downloads"), if all
    /// items share one.
    public let sourceDisplay: String?
    /// False when every item is already inside the private folder (decision
    /// 5.10: indexed in place, nothing to move).
    public let needsMoveDecision: Bool
    /// Whether the single item (if one) is a folder; for the dialog copy.
    public let isSingleFolder: Bool

    public var count: Int { urls.count }
    public var isEmpty: Bool { urls.isEmpty }
}
