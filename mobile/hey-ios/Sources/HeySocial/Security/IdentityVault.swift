import Foundation
import Security
import LocalAuthentication
import os.log

// Optional identity vault — the iOS counterpart of Android's IdentityVault.
//
// Encrypts the Hey BIP39 seed at rest under a HARDWARE key in the Secure Enclave that
// is BIOMETRY-GATED: the key can only be used after a Face ID / Touch ID / passcode
// check, so the seed is never plaintext at rest and unlock = your biometric. Off by
// default — always-on background delivery stays the default; with the vault ON,
// full message decryption resumes only after you unlock (the carrier still meshes +
// buffers sealed messages headless in between; see the Android headless-vault model).
//
// Safety, matching Android: the access control uses `.userPresence` (biometric OR the
// device passcode), and — critically — is NOT `.biometryCurrentSet`, so enrolling a
// new face/finger does NOT invalidate the key and lock you out. The caller
// round-trip-verifies a seal (seal → unseal == plaintext) before deleting the
// plaintext seed.
//
// Wrap uses the Enclave PUBLIC key (no prompt). Unseal uses the PRIVATE key, whose ACL
// forces the biometric prompt. Recovery still works from the one BIP39 phrase even if
// the key is lost — re-entering the phrase re-provisions everything.
enum IdentityVault {
    private static let log = Logger(subsystem: "os.elastos.hey", category: "identity-vault")
    private static let group = "os.elastos.hey.messaging"
    private static let sealAccount = "identity-seed-sealed-v1"
    private static let keyTag = "os.elastos.hey.identity.kek".data(using: .utf8)!
    private static let onKey = "identity_vault_on"
    private static let eciesAlgo: SecKeyAlgorithm = .eciesEncryptionCofactorVariableIVX963SHA256AESGCM
    private static var defaults: UserDefaults { UserDefaults(suiteName: AppPaths.appGroup) ?? .standard }

    /// Can we hardware-seal here? Needs a Secure Enclave AND an enrolled biometric/passcode.
    static func available() -> Bool { secureEnclaveAvailable() && AppLock.available() }

    static var isOn: Bool {
        get { defaults.bool(forKey: onKey) }
        set { defaults.set(newValue, forKey: onKey) }
    }
    static func hasSealed() -> Bool { readBlob() != nil }

    /// Seal the seed. Call AFTER a successful AppLock prompt. Wrapping uses the public
    /// key (no second prompt). Returns false on failure (caller must keep plaintext).
    static func seal(_ seed: String) -> Bool {
        guard let key = loadKey() ?? makeKey(),
              let pub = SecKeyCopyPublicKey(key),
              SecKeyIsAlgorithmSupported(pub, .encrypt, eciesAlgo) else { return false }
        var err: Unmanaged<CFError>?
        let ct = SecKeyCreateEncryptedData(pub, eciesAlgo, Data(seed.utf8) as CFData, &err)
        err?.release()
        guard let blob = ct as Data? else { log.error("IdentityVault: seal encrypt failed"); return false }
        writeBlob(blob)
        return true
    }

    /// Decrypt the sealed seed — triggers Face ID / Touch ID (the key's ACL requires it).
    /// `reason` is shown in the system prompt. Returns nil on cancel/failure.
    static func unseal(reason: String = "Unlock your Hey identity") -> String? {
        guard let blob = readBlob(), let key = loadKey(prompt: reason),
              SecKeyIsAlgorithmSupported(key, .decrypt, eciesAlgo) else { return nil }
        var err: Unmanaged<CFError>?
        let pt = SecKeyCreateDecryptedData(key, eciesAlgo, blob as CFData, &err)
        err?.release()
        guard let data = pt as Data? else { return nil }
        return String(data: data, encoding: .utf8)
    }

    static func clear() {
        deleteKey()
        let q: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: "hey",
            kSecAttrAccount as String: sealAccount,
            kSecAttrAccessGroup as String: group,
        ]
        SecItemDelete(q as CFDictionary)
        isOn = false
    }

    // MARK: - Biometry-gated Secure Enclave key

    private static func secureEnclaveAvailable() -> Bool {
        guard let ac = SecAccessControlCreateWithFlags(nil, kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly, [.privateKeyUsage], nil) else { return false }
        let attrs: [String: Any] = [
            kSecAttrKeyType as String: kSecAttrKeyTypeECSECPrimeRandom,
            kSecAttrKeySizeInBits as String: 256,
            kSecAttrTokenID as String: kSecAttrTokenIDSecureEnclave,
            kSecPrivateKeyAttrs as String: [kSecAttrIsPermanent as String: false, kSecAttrAccessControl as String: ac],
        ]
        var err: Unmanaged<CFError>?
        let k = SecKeyCreateRandomKey(attrs as CFDictionary, &err)
        err?.release()
        return k != nil
    }

    private static func loadKey(prompt: String? = nil) -> SecKey? {
        var q: [String: Any] = [
            kSecClass as String: kSecClassKey,
            kSecAttrApplicationTag as String: keyTag,
            kSecAttrKeyType as String: kSecAttrKeyTypeECSECPrimeRandom,
            kSecReturnRef as String: true,
        ]
        if let prompt { q[kSecUseOperationPrompt as String] = prompt }
        var item: CFTypeRef?
        guard SecItemCopyMatching(q as CFDictionary, &item) == errSecSuccess else { return nil }
        return (item as! SecKey?)
    }

    /// `.userPresence` = biometric OR passcode, and NOT invalidated on biometric
    /// re-enrollment (unlike `.biometryCurrentSet`). `whenUnlocked` so it's usable only
    /// while the device is unlocked, but survives across enrollments.
    private static func makeKey() -> SecKey? {
        guard let ac = SecAccessControlCreateWithFlags(nil, kSecAttrAccessibleWhenUnlockedThisDeviceOnly, [.privateKeyUsage, .userPresence], nil) else { return nil }
        let attrs: [String: Any] = [
            kSecAttrKeyType as String: kSecAttrKeyTypeECSECPrimeRandom,
            kSecAttrKeySizeInBits as String: 256,
            kSecAttrTokenID as String: kSecAttrTokenIDSecureEnclave,
            kSecPrivateKeyAttrs as String: [
                kSecAttrIsPermanent as String: true,
                kSecAttrApplicationTag as String: keyTag,
                kSecAttrAccessControl as String: ac,
            ],
        ]
        var err: Unmanaged<CFError>?
        let k = SecKeyCreateRandomKey(attrs as CFDictionary, &err)
        // Consume the +1 (Create-rule) error exactly once. takeRetainedValue() balances
        // the retain, so we must NOT also call release() (that would over-release).
        if k == nil { log.error("IdentityVault: keygen failed: \(String(describing: err?.takeRetainedValue()))") }
        else { err?.release() }
        return k
    }

    private static func deleteKey() {
        let q: [String: Any] = [
            kSecClass as String: kSecClassKey,
            kSecAttrApplicationTag as String: keyTag,
            kSecAttrKeyType as String: kSecAttrKeyTypeECSECPrimeRandom,
        ]
        SecItemDelete(q as CFDictionary)
    }

    // MARK: - Sealed-blob Keychain storage

    private static func readBlob() -> Data? {
        let q: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: "hey",
            kSecAttrAccount as String: sealAccount,
            kSecAttrAccessGroup as String: group,
            kSecReturnData as String: true,
        ]
        var item: CFTypeRef?
        guard SecItemCopyMatching(q as CFDictionary, &item) == errSecSuccess, let d = item as? Data else { return nil }
        return d
    }

    private static func writeBlob(_ data: Data) {
        let q: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: "hey",
            kSecAttrAccount as String: sealAccount,
            kSecAttrAccessGroup as String: group,
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
            kSecValueData as String: data,
        ]
        SecItemDelete(q as CFDictionary)
        SecItemAdd(q as CFDictionary, nil)
    }
}
