import Foundation

/// The slice of UserDefaults the app uses, so preferences are testable.
public protocol KeyValueStore: AnyObject {
    func bool(forKey key: String) -> Bool
    func set(_ value: Bool, forKey key: String)
    func string(forKey key: String) -> String?
    func setString(_ value: String?, forKey key: String)
}

extension UserDefaults: KeyValueStore {
    public func setString(_ value: String?, forKey key: String) {
        set(value, forKey: key)
    }
}
