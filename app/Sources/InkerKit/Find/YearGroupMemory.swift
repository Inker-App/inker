import Foundation

/// Which year groups are open, remembered across launches. The newest year
/// opens by default and older ones are collapsed until the user says so.
public struct YearGroupMemory {
    public static let key = "sh.inker.board.years"
    private let store: KeyValueStore

    public init(store: KeyValueStore) {
        self.store = store
    }

    /// "2025=1,2024=0"
    private var state: [String: Bool] {
        var out: [String: Bool] = [:]
        for pair in (store.string(forKey: YearGroupMemory.key) ?? "").split(separator: ",") {
            let kv = pair.split(separator: "=", maxSplits: 1)
            guard kv.count == 2 else { continue }
            out[String(kv[0])] = kv[1] == "1"
        }
        return out
    }

    public func isOpen(_ id: String, newest: String?) -> Bool {
        state[id] ?? (id == newest)
    }

    public mutating func set(_ id: String, open: Bool) {
        var s = state
        s[id] = open
        let text = s.keys.sorted().map { "\($0)=\(s[$0] == true ? 1 : 0)" }.joined(separator: ",")
        store.setString(text, forKey: YearGroupMemory.key)
    }
}
