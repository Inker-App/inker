import Foundation
import InkerCore
import InkerKit
import Security

/// The vault key, kept as a generic password in the user's Keychain
/// (stack-decision 3.3). 32 random bytes from `SecRandomCopyBytes`, created
/// on first use. The core asks once, at open.
///
/// Errors carry an `OSStatus` only, never key material.
final class KeychainKeyStore: @unchecked Sendable {
    private let lock = NSLock()

    func loadOrCreate() throws -> Data {
        lock.lock()
        defer { lock.unlock() }
        if let existing = try read() {
            guard VaultKey.isValid(existing) else {
                throw ProviderError.Failed(reason: "the Keychain vault key has the wrong length")
            }
            return existing
        }
        var key = Data(count: VaultKey.length)
        let random = key.withUnsafeMutableBytes { buffer -> OSStatus in
            guard let base = buffer.baseAddress else { return errSecAllocate }
            return SecRandomCopyBytes(kSecRandomDefault, VaultKey.length, base)
        }
        guard random == errSecSuccess else {
            throw ProviderError.Failed(reason: "SecRandomCopyBytes failed: \(random)")
        }
        // VERIFY: on macOS this lands in the file-based login keychain, where
        // kSecAttrAccessible is accepted but not enforced. Enforcing it needs
        // kSecUseDataProtectionKeychain, which needs a keychain-access-groups
        // entitlement (and so a provisioning profile for Developer ID). Revisit
        // once the app is signed. Unsigned dev builds may also prompt for
        // Keychain access after every rebuild, since the code signature changes.
        let add: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: VaultKey.service,
            kSecAttrAccount as String: VaultKey.account,
            kSecAttrLabel as String: "INKER vault key",
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
            kSecAttrSynchronizable as String: false,
            kSecValueData as String: key,
        ]
        let status = SecItemAdd(add as CFDictionary, nil)
        if status == errSecDuplicateItem, let raced = try read(), VaultKey.isValid(raced) {
            return raced
        }
        guard status == errSecSuccess else {
            throw ProviderError.Failed(reason: "SecItemAdd failed: \(status)")
        }
        return key
    }

    private func read() throws -> Data? {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: VaultKey.service,
            kSecAttrAccount as String: VaultKey.account,
            kSecMatchLimit as String: kSecMatchLimitOne,
            kSecReturnData as String: true,
        ]
        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        switch status {
        case errSecSuccess:
            guard let data = item as? Data else {
                throw ProviderError.Failed(reason: "the Keychain returned no data for the vault key")
            }
            return data
        case errSecItemNotFound:
            return nil
        default:
            throw ProviderError.Failed(reason: "SecItemCopyMatching failed: \(status)")
        }
    }
}
