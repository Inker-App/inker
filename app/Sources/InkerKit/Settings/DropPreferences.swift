import Foundation
import InkerCore

/// The first-drop dialog's memory. It asks on every drop until the user picks
/// Move with "Always move, don't ask again" checked.
public struct DropPreferences {
    public static let alwaysMoveKey = "sh.inker.drop.alwaysMove"
    private let store: KeyValueStore

    public init(store: KeyValueStore) {
        self.store = store
    }

    /// Non-nil when the dialog should be skipped.
    public var rememberedMode: IngestMode? {
        store.bool(forKey: DropPreferences.alwaysMoveKey) ? .move : nil
    }

    public mutating func record(_ mode: IngestMode, always: Bool) {
        if mode == .move && always {
            store.set(true, forKey: DropPreferences.alwaysMoveKey)
        }
    }
}
