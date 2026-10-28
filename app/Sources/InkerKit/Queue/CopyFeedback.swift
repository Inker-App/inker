import Foundation

/// The Copy button reads "Copied" for 1.5 s after a press.
public struct CopyFeedback: Equatable, Sendable {
    public static let duration: TimeInterval = 1.5
    public private(set) var copiedID: String?
    public private(set) var copiedAt: Date?

    public init() {}

    public mutating func markCopied(_ id: String, at date: Date) {
        copiedID = id
        copiedAt = date
    }

    public func isShowingCopied(_ id: String, now: Date) -> Bool {
        guard copiedID == id, let at = copiedAt else { return false }
        return now.timeIntervalSince(at) < CopyFeedback.duration
    }

    public func label(for id: String, now: Date) -> String {
        isShowingCopied(id, now: now) ? "Copied" : "Copy"
    }
}
