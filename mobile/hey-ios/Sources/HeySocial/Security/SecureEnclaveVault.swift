import Foundation
import Security
import LocalAuthentication
import os.log

// Storage data-encryption key (DEK) — the iOS counterpart of Android's StorageVault.
//
// The runtime seals every persisted file (the BIP39 seed/identity, the Double-Ratchet
// PRIVATE keys, conversation plaintext, contacts, pinned peer keys) under a 32-byte
// DEK. We generate that DEK once, WRAP it with a hardware key held in the SECURE
// ENCLAVE (the iPhone/iPad security chip — the KEK), and persist only the wrapped DEK
// in the Keychain. At startup the unwrapped DEK is handed to Rust via
// `hey_set_storage_key` BEFORE the runtime touches disk, so nothing at rest is ever
// plaintext (ios.rs §lifecycle).
//
// WHY the Secure Enclave: it does P-256 ECC only and its private key is
// NON-EXPORTABLE — it never leaves the chip. We use it as a KEK and wrap the AES DEK
// via ECIES (ECDH→HKDF→AES-GCM, all in-Enclave). This is the exact StrongBox/TEE model
// from Android: hardware non-exportability is the at-rest protection — the wrapped DEK
// is useless without THIS device's Enclave, which defeats the real threats (an iTunes/
// iCloud-style backup extraction, a forensic image, another process reading the
// container). `…ThisDeviceOnly` accessibility keeps it off backups entirely.
//
// The KEK is deliberately NOT biometry-gated (`.privateKeyUsage` only): the
// Notification Service Extension must unwrap the DEK to decrypt a pushed message with
// the user absent. Hardware non-exportability — not a biometric — is what protects it.
// The separate, biometric-gated AppLock + IdentityVault are the user-presence gate for
// opening the app, revealing the phrase, and spends.
//
// FALLBACK: the iOS Simulator (and very old devices) have no usable Secure Enclave. We
// detect that, fall back to a software DEK stored directly in the Keychain
// (`…AfterFirstUnlockThisDeviceOnly`, the device's hardware-encrypted keystore), and
// log LOUDLY that hardware wrapping is off — we never silently pretend. An existing
// software DEK is transparently migrated (wrapped) the first time the Enclave is present.
enum SecureEnclaveVault {
    private static let log = Logger(subsystem: "os.elastos.hey", category: "vault")
    private static let group = "os.elastos.hey.messaging"   // shared Keychain access group (app + NSE)
    private static let wrappedAccount = "at-rest-dek-wrapped-v1"  // SE-wrapped DEK blob
    private static let plainAccount = "at-rest-dek-v1"            // software DEK (fallback / legacy)
    private static let kekTag = "os.elastos.hey.dek.kek".data(using: .utf8)!
    private static let eciesAlgo: SecKeyAlgorithm = .eciesEncryptionCofactorVariableIVX963SHA256AESGCM

    /// The 32-byte storage DEK as Base64, creating + hardware-wrapping it on first call.
    /// Pass to `hey_set_storage_key` BEFORE `hey_start`. Returns nil only if no key store
    /// at all is usable (extremely rare) — the runtime then stays plaintext and says so.
    static func storageKeyBase64() -> String? {
        // 1) Already wrapped under the Enclave? Unwrap and return.
        if let blob = readKeychain(wrappedAccount), let kek = loadKEK() {
            if let dek = unwrap(blob, with: kek) { return dek.base64EncodedString() }
            log.error("SecureEnclaveVault: wrapped DEK present but unwrap failed — keystore may be corrupt")
        }

        // 2) No Enclave on this device/sim → software DEK in the Keychain (logged).
        guard secureEnclaveAvailable() else {
            log.error("SecureEnclaveVault: Secure Enclave UNAVAILABLE — at-rest DEK is software-Keychain-protected (no hardware wrap). Expected on the Simulator.")
            return softwareDEKBase64()
        }

        // 3) Enclave present. Reuse a legacy software DEK's bytes if one exists (migrate),
        //    else mint a fresh 32-byte DEK. Then wrap under a (new or existing) Enclave KEK.
        let dek: Data
        if let legacy = readKeychain(plainAccount), let d = Data(base64Encoded: legacy), d.count == 32 {
            dek = d
            log.info("SecureEnclaveVault: migrating software DEK under the Secure Enclave")
        } else if let fresh = randomBytes(32) {
            dek = fresh
        } else {
            return nil
        }
        guard let kek = loadKEK() ?? makeKEK(), let blob = wrap(dek, with: kek) else {
            log.error("SecureEnclaveVault: Enclave wrap failed — falling back to software DEK")
            return softwareDEKBase64(seed: dek)
        }
        writeKeychain(wrappedAccount, blob)
        deleteKeychain(plainAccount)   // remove the now-migrated plaintext copy
        return dek.base64EncodedString()
    }

    /// True once a wrapped (or software) DEK exists — storage is encrypted at rest.
    static func isActive() -> Bool { readKeychain(wrappedAccount) != nil || readKeychain(plainAccount) != nil }

    /// True when the DEK is protected by the Secure Enclave (vs the software fallback).
    static func isHardwareBacked() -> Bool { readKeychain(wrappedAccount) != nil }

    // MARK: - Secure Enclave KEK

    private static func secureEnclaveAvailable() -> Bool {
        // The honest probe: actually attempt an ephemeral Enclave keygen. The Simulator
        // returns errSecUnimplemented / -25293 here; real hardware succeeds.
        guard let ac = SecAccessControlCreateWithFlags(nil, kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly, [.privateKeyUsage], nil) else { return false }
        let attrs: [String: Any] = [
            kSecAttrKeyType as String: kSecAttrKeyTypeECSECPrimeRandom,
            kSecAttrKeySizeInBits as String: 256,
            kSecAttrTokenID as String: kSecAttrTokenIDSecureEnclave,
            kSecPrivateKeyAttrs as String: [
                kSecAttrIsPermanent as String: false,        // ephemeral probe — not stored
                kSecAttrAccessControl as String: ac,
            ],
        ]
        var err: Unmanaged<CFError>?
        let key = SecKeyCreateRandomKey(attrs as CFDictionary, &err)
        err?.release()
        return key != nil
    }

    /// Load the persistent Enclave KEK private key handle, if it exists.
    private static func loadKEK() -> SecKey? {
        let q: [String: Any] = [
            kSecClass as String: kSecClassKey,
            kSecAttrApplicationTag as String: kekTag,
            kSecAttrKeyType as String: kSecAttrKeyTypeECSECPrimeRandom,
            kSecReturnRef as String: true,
        ]
        var item: CFTypeRef?
        guard SecItemCopyMatching(q as CFDictionary, &item) == errSecSuccess else { return nil }
        return (item as! SecKey?)
    }

    /// Create + persist the Enclave KEK. `.privateKeyUsage` only — no biometric, so the
    /// NSE can unwrap headless; `…AfterFirstUnlockThisDeviceOnly` keeps it on-device.
    private static func makeKEK() -> SecKey? {
        guard let ac = SecAccessControlCreateWithFlags(nil, kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly, [.privateKeyUsage], nil) else { return nil }
        let attrs: [String: Any] = [
            kSecAttrKeyType as String: kSecAttrKeyTypeECSECPrimeRandom,
            kSecAttrKeySizeInBits as String: 256,
            kSecAttrTokenID as String: kSecAttrTokenIDSecureEnclave,
            kSecPrivateKeyAttrs as String: [
                kSecAttrIsPermanent as String: true,
                kSecAttrApplicationTag as String: kekTag,
                kSecAttrAccessControl as String: ac,
            ],
        ]
        var err: Unmanaged<CFError>?
        let key = SecKeyCreateRandomKey(attrs as CFDictionary, &err)
        // Consume the +1 (Create-rule) error exactly once. takeRetainedValue() balances
        // the retain, so we must NOT also call release() (that would over-release).
        if key == nil { log.error("SecureEnclaveVault: KEK keygen failed: \(String(describing: err?.takeRetainedValue()))") }
        else { err?.release() }
        return key
    }

    private static func wrap(_ data: Data, with kek: SecKey) -> Data? {
        guard let pub = SecKeyCopyPublicKey(kek),
              SecKeyIsAlgorithmSupported(pub, .encrypt, eciesAlgo) else { return nil }
        var err: Unmanaged<CFError>?
        let ct = SecKeyCreateEncryptedData(pub, eciesAlgo, data as CFData, &err)
        err?.release()
        return ct as Data?
    }

    private static func unwrap(_ blob: Data, with kek: SecKey) -> Data? {
        guard SecKeyIsAlgorithmSupported(kek, .decrypt, eciesAlgo) else { return nil }
        var err: Unmanaged<CFError>?
        let pt = SecKeyCreateDecryptedData(kek, eciesAlgo, blob as CFData, &err)
        err?.release()
        return pt as Data?
    }

    // MARK: - Software fallback DEK (no Enclave)

    private static func softwareDEKBase64(seed: Data? = nil) -> String? {
        if let existing = readKeychain(plainAccount), let d = Data(base64Encoded: existing), d.count == 32 {
            return d.base64EncodedString()
        }
        guard let dek = seed ?? randomBytes(32), dek.count == 32 else { return nil }
        writeKeychain(plainAccount, Data(dek.base64EncodedString().utf8))
        return dek.base64EncodedString()
    }

    // MARK: - Keychain primitives

    private static func randomBytes(_ n: Int) -> Data? {
        var b = [UInt8](repeating: 0, count: n)
        return SecRandomCopyBytes(kSecRandomDefault, n, &b) == errSecSuccess ? Data(b) : nil
    }

    private static func readKeychain(_ account: String) -> Data? {
        let q: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: "hey",
            kSecAttrAccount as String: account,
            kSecAttrAccessGroup as String: group,
            kSecReturnData as String: true,
        ]
        var item: CFTypeRef?
        guard SecItemCopyMatching(q as CFDictionary, &item) == errSecSuccess, let d = item as? Data else { return nil }
        // wrapped blob is raw bytes; software DEK is a base64 string stored as utf8 — both Data.
        return d
    }

    private static func writeKeychain(_ account: String, _ data: Data) {
        let q: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: "hey",
            kSecAttrAccount as String: account,
            kSecAttrAccessGroup as String: group,
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
            kSecValueData as String: data,
        ]
        SecItemDelete(q as CFDictionary)
        SecItemAdd(q as CFDictionary, nil)
    }

    private static func deleteKeychain(_ account: String) {
        let q: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: "hey",
            kSecAttrAccount as String: account,
            kSecAttrAccessGroup as String: group,
        ]
        SecItemDelete(q as CFDictionary)
    }
}

/// Back-compat alias — earlier scaffold code referenced `KeychainVault`.
typealias KeychainVault = SecureEnclaveVault
