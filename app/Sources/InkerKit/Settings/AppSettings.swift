import Foundation
import InkerCore

/// Stored settings: the private folder, the clipboard handle style, whether
/// rows show the payer, whether the clipboard chip is offered and whether
/// Copy adds a note for the chat.
public struct AppSettings {
    public static let privateFolderKey = "sh.inker.privateFolder"
    public static let handleStyleKey = "sh.inker.handleStyle"
    /// Both default on, so the stored flag is the opt-out.
    public static let payerHiddenKey = "sh.inker.payerHidden"
    public static let clipboardChipOffKey = "sh.inker.clipboardChipOff"
    /// On by default too: the stored flag is the opt-out.
    public static let copyHandleNoteOffKey = "sh.inker.copyHandleNoteOff"
    public static let showPayerLabel = "Show payer names in the list"
    public static let clipboardChipLabel = "Offer to find a handle from the clipboard"
    public static let copyHandleNoteLabel = "Add a note for the chat when copying a handle"
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

    /// "W-2 · Acme Robotics · 6 inked" vs "W-2 · 6 inked".
    public var showsPayer: Bool {
        get { !store.bool(forKey: AppSettings.payerHiddenKey) }
        set { store.set(!newValue, forKey: AppSettings.payerHiddenKey) }
    }

    /// The clipboard chip when the popover opens with a known handle copied.
    public var offersClipboardHandle: Bool {
        get { !store.bool(forKey: AppSettings.clipboardChipOffKey) }
        set { store.set(!newValue, forKey: AppSettings.clipboardChipOffKey) }
    }

    /// Copy adds `HandlePaste.note` after the handle, for chat apps that do
    /// not have INKER's tools in view when the paste arrives. `nonmutating`:
    /// it writes straight to the shared store, so Settings can flip it
    /// through `CoreStore.settings` without a CoreStore setter.
    public var copiesHandleNote: Bool {
        get { !store.bool(forKey: AppSettings.copyHandleNoteOffKey) }
        nonmutating set { store.set(!newValue, forKey: AppSettings.copyHandleNoteOffKey) }
    }
}
