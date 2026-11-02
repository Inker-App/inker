import Foundation

/// Which folders on the board are open, remembered across launches by their
/// path inside the private folder. Folders start collapsed; the root (the
/// empty path) is always open.
public struct FolderMemory {
    public static let key = "sh.inker.board.folders"
    private let store: KeyValueStore

    public init(store: KeyValueStore) {
        self.store = store
    }

    /// A JSON array of the open paths: a folder name can hold any character.
    private var open: Set<String> {
        guard let text = store.string(forKey: FolderMemory.key),
              let paths = try? JSONDecoder().decode([String].self, from: Data(text.utf8)) else {
            return []
        }
        return Set(paths)
    }

    public func isOpen(_ path: String) -> Bool {
        path.isEmpty || open.contains(path)
    }

    public mutating func set(_ path: String, open isOpen: Bool) {
        var paths = open
        if isOpen { paths.insert(path) } else { paths.remove(path) }
        guard let data = try? JSONEncoder().encode(paths.sorted()) else { return }
        store.setString(String(decoding: data, as: UTF8.self), forKey: FolderMemory.key)
    }
}
