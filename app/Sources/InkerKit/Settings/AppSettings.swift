import Foundation
import InkerCore

/// Stored settings: the private folder and the clipboard handle style.
public struct AppSettings {
    public static let privateFolderKey = "sh.inker.privateFolder"
    public static let handleStyleKey = "sh.inker.handleStyle"
    private let store: KeyValueStore

    public init(store: KeyValueStore) {
        self.store = store
    }

    public var privateFolderPath: String? {
        get { store.string(forKey: AppSettings.privateFolderKey) }
        set { store.setString(newValue, forKey: AppSettings.privateFolderKey) }
    }

    public var handleStyle: HandleStyle {
        get { HandleFormatter.style(named: store.string(forKey: AppSettings.handleStyleKey)) }
        set { store.setString(HandleFormatter.name(of: newValue), forKey: AppSettings.handleStyleKey) }
    }
}
