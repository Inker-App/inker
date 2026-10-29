import InkerCore

/// What to do with a drop before it is filed.
public enum DropDecision: Equatable, Sendable {
    case ingest(IngestMode)
    /// Show the move-or-copy card in the popover.
    case ask

    public static func resolve(needsMoveDecision: Bool, remembered: IngestMode?) -> DropDecision {
        guard needsMoveDecision else { return .ingest(.move) }
        if let remembered { return .ingest(remembered) }
        return .ask
    }
}
