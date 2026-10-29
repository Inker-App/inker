import Foundation

/// Which activation policy the app should have. A menu bar app is an
/// accessory: no Dock tile, no Cmd+Tab entry. While one of its windows is
/// open it becomes regular, so the window can be found again.
public struct WindowPresence: Equatable, Sendable {
    public enum Policy: Equatable, Sendable {
        case accessory
        case regular
    }

    private var open: Set<String> = []

    public init() {}

    public var policy: Policy { open.isEmpty ? .accessory : .regular }

    /// Returns the new policy when it changes, else nil.
    public mutating func opened(_ id: String) -> Policy? {
        let before = policy
        open.insert(id)
        return policy == before ? nil : policy
    }

    /// Returns the new policy when it changes, else nil.
    public mutating func closed(_ id: String) -> Policy? {
        let before = policy
        open.remove(id)
        return policy == before ? nil : policy
    }
}
