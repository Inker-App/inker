//! The vault: real value <-> stable token, encrypted at rest.
//!
//! File format of `<data_dir>/vault.bin`: the 5-byte header `INKV\x01`, a
//! 12-byte random nonce, then AES-256-GCM ciphertext (with the header as
//! associated data) of a JSON document holding the per-kind counters and the
//! entries. The whole file is rewritten atomically whenever a new value is
//! minted.
//!
//! Real values only ever exist in memory here and in the ciphertext. Nothing in
//! this module logs or formats a value; `Debug` prints counts only.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::ops::Range;
use std::sync::{Arc, Mutex, MutexGuard};

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};

use crate::fsutil::atomic_write;
use crate::ports::KeyStore;
use crate::{Error, Result, Token, TokenKind};

const HEADER: &[u8; 5] = b"INKV\x01";
const SEAL_HEADER: &[u8; 5] = b"INKS\x01";
const NONCE_LEN: usize = 12;

/// In-memory key store for tests and the headless CLI. Clones share the key.
#[derive(Clone)]
pub struct MemoryKeyStore {
    key: Arc<[u8; 32]>,
}

impl fmt::Debug for MemoryKeyStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MemoryKeyStore(..)")
    }
}

impl MemoryKeyStore {
    /// A fresh random key.
    pub fn new() -> Result<Self> {
        let mut key = [0u8; 32];
        getrandom::fill(&mut key).map_err(|e| Error::Vault(format!("random key: {e}")))?;
        Ok(Self { key: Arc::new(key) })
    }

    /// A fixed key (tests).
    pub fn with_key(key: [u8; 32]) -> Self {
        Self { key: Arc::new(key) }
    }
}

impl KeyStore for MemoryKeyStore {
    fn load_or_create(&self) -> Result<[u8; 32]> {
        Ok(*self.key)
    }
}

#[derive(Serialize, Deserialize, Default)]
struct VaultFile {
    version: u32,
    /// Keyed by bracket label, so two custom kinds with the same label share numbering.
    counters: BTreeMap<String, u32>,
    entries: Vec<StoredEntry>,
}

#[derive(Serialize, Deserialize)]
struct StoredEntry {
    kind: TokenKind,
    index: u32,
    /// Normalised lookup key.
    key: String,
    /// The value as first seen; this is what rehydration writes back.
    value: String,
}

#[derive(Default)]
struct State {
    file: VaultFile,
    /// (label, normalised value) -> token.
    forward: HashMap<(String, String), Token>,
    /// (label, index) -> position in `file.entries`.
    reverse: HashMap<(String, u32), usize>,
}

impl State {
    fn from_file(file: VaultFile) -> Self {
        let mut s = State {
            file,
            ..Default::default()
        };
        for (i, e) in s.file.entries.iter().enumerate() {
            let label = e.kind.label();
            let token = Token {
                kind: e.kind.clone(),
                index: e.index,
            };
            s.forward.insert((label.clone(), e.key.clone()), token);
            s.reverse.insert((label, e.index), i);
        }
        s
    }
}

/// The value-to-token map. Thread-safe; share it behind `&`.
pub struct Vault {
    path: Utf8PathBuf,
    cipher: Aes256Gcm,
    state: Mutex<State>,
}

impl fmt::Debug for Vault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Vault")
            .field("path", &self.path)
            .field("entries", &self.len())
            .finish_non_exhaustive()
    }
}

impl Vault {
    /// Open (or create on first mint) the vault at `path`, decrypting with the
    /// key from `keystore`.
    pub fn open(path: &Utf8Path, keystore: &dyn KeyStore) -> Result<Self> {
        let key = keystore.load_or_create()?;
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
        let file = if path.exists() {
            let bytes = std::fs::read(path)?;
            let plain = decrypt(&cipher, HEADER, &bytes).map_err(|_| {
                Error::Vault("could not decrypt the vault (wrong key or damaged file)".into())
            })?;
            let file: VaultFile = serde_json::from_slice(&plain)
                .map_err(|_| Error::Vault("vault contents are malformed".into()))?;
            file
        } else {
            VaultFile {
                version: 1,
                ..Default::default()
            }
        };
        let state = State::from_file(file);
        tracing::debug!(entries = state.file.entries.len(), "vault opened");
        Ok(Self {
            path: path.to_owned(),
            cipher,
            state: Mutex::new(state),
        })
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        // A poisoned lock still holds consistent data: every mutation below
        // completes before anything that can panic.
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Token for `value`, minting the next index of `kind` on first sight.
    /// Idempotent: the same (normalised) value always returns the same token.
    pub fn mint(&self, kind: &TokenKind, value: &str) -> Result<Token> {
        let key = normalize(kind, value);
        if key.is_empty() {
            return Err(Error::Vault(format!("empty {} value", kind.label())));
        }
        let label = kind.label();
        let mut st = self.lock();
        if let Some(t) = st.forward.get(&(label.clone(), key.clone())) {
            return Ok(t.clone());
        }
        let index = st.file.counters.get(&label).copied().unwrap_or(0) + 1;
        let token = Token {
            kind: kind.clone(),
            index,
        };
        st.file.counters.insert(label.clone(), index);
        st.file.entries.push(StoredEntry {
            kind: kind.clone(),
            index,
            key: key.clone(),
            value: value.trim().to_string(),
        });
        let pos = st.file.entries.len() - 1;
        st.forward.insert((label.clone(), key), token.clone());
        st.reverse.insert((label, index), pos);
        if let Err(e) = self.persist(&st.file) {
            // Roll back so memory never holds a token the disk does not.
            st.file.entries.pop();
            st.file.counters.insert(token.kind.label(), index - 1);
            st.forward.retain(|_, t| t != &token);
            st.reverse.remove(&(token.kind.label(), index));
            return Err(e);
        }
        tracing::debug!(token = %token, "minted");
        Ok(token)
    }

    /// Token for `value` if it has been minted, without minting.
    pub fn lookup(&self, kind: &TokenKind, value: &str) -> Option<Token> {
        let key = normalize(kind, value);
        self.lock().forward.get(&(kind.label(), key)).cloned()
    }

    /// Replace every known `[KIND_N]` token in `text` with its real value.
    /// Unknown tokens are left as they are.
    pub fn rehydrate(&self, text: &str) -> String {
        let st = self.lock();
        let mut out = String::with_capacity(text.len());
        let mut last = 0;
        for (range, label, index) in find_tokens(text) {
            if let Some(&pos) = st.reverse.get(&(label, index)) {
                out.push_str(&text[last..range.start]);
                out.push_str(&st.file.entries[pos].value);
                last = range.end;
            }
        }
        out.push_str(&text[last..]);
        out
    }

    /// Number of distinct values held.
    pub fn len(&self) -> usize {
        self.lock().file.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Encrypt an arbitrary blob with the vault key (used by the index for the
    /// raw extracted text of held documents).
    pub fn seal(&self, plain: &[u8]) -> Result<Vec<u8>> {
        encrypt(&self.cipher, SEAL_HEADER, plain)
    }

    /// Decrypt a blob produced by [`Vault::seal`].
    pub fn unseal(&self, blob: &[u8]) -> Result<Vec<u8>> {
        decrypt(&self.cipher, SEAL_HEADER, blob)
    }

    fn persist(&self, file: &VaultFile) -> Result<()> {
        let plain = serde_json::to_vec(file)?;
        let blob = encrypt(&self.cipher, HEADER, &plain)?;
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        atomic_write(&self.path, &blob)
    }
}

fn encrypt(cipher: &Aes256Gcm, header: &[u8; 5], plain: &[u8]) -> Result<Vec<u8>> {
    let mut nonce = [0u8; NONCE_LEN];
    getrandom::fill(&mut nonce).map_err(|e| Error::Vault(format!("random nonce: {e}")))?;
    let ct = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plain,
                aad: header,
            },
        )
        .map_err(|_| Error::Vault("encryption failed".into()))?;
    let mut out = Vec::with_capacity(header.len() + NONCE_LEN + ct.len());
    out.extend_from_slice(header);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    Ok(out)
}

fn decrypt(cipher: &Aes256Gcm, header: &[u8; 5], blob: &[u8]) -> Result<Vec<u8>> {
    let min = header.len() + NONCE_LEN;
    if blob.len() < min || &blob[..header.len()] != header {
        return Err(Error::Vault("not an INKER encrypted blob".into()));
    }
    let nonce = Nonce::from_slice(&blob[header.len()..min]);
    cipher
        .decrypt(
            nonce,
            Payload {
                msg: &blob[min..],
                aad: header,
            },
        )
        .map_err(|_| Error::Vault("decryption failed".into()))
}

/// Canonical lookup key for a value of `kind`.
///
/// Numeric identifiers drop spaces, hyphens and other punctuation and are
/// upper-cased; phone numbers also drop a leading US `1`; names and other text
/// are case-folded with whitespace collapsed.
pub fn normalize(kind: &TokenKind, value: &str) -> String {
    match kind {
        TokenKind::Ssn
        | TokenKind::Itin
        | TokenKind::Ein
        | TokenKind::Account
        | TokenKind::Routing
        | TokenKind::Card
        | TokenKind::GovId => alnum_upper(value),
        TokenKind::Phone => {
            let d: String = value.chars().filter(|c| c.is_ascii_digit()).collect();
            match d.strip_prefix('1') {
                Some(rest) if d.len() == 11 => rest.to_string(),
                _ => d,
            }
        }
        TokenKind::Email => value.trim().to_lowercase(),
        TokenKind::Person | TokenKind::Org | TokenKind::Address | TokenKind::Custom(_) => value
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase(),
    }
}

fn alnum_upper(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_uppercase)
        .collect()
}

/// Every well-formed `[LABEL_N]` in `text`: byte range, label, index.
/// Labels are `[A-Z0-9_]+`; the index is the digits after the last `_`.
pub fn find_tokens(text: &str) -> Vec<(Range<usize>, String, u32)> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'[' {
            i += 1;
            continue;
        }
        let start = i;
        let mut j = i + 1;
        while j < bytes.len()
            && (bytes[j].is_ascii_uppercase() || bytes[j].is_ascii_digit() || bytes[j] == b'_')
        {
            j += 1;
        }
        if j < bytes.len() && bytes[j] == b']' {
            let inner = &text[start + 1..j];
            if let Some((label, idx)) = inner.rsplit_once('_')
                && !label.is_empty()
                && !idx.is_empty()
                && idx.bytes().all(|b| b.is_ascii_digit())
                && let Ok(n) = idx.parse::<u32>()
            {
                out.push((start..j + 1, label.to_string(), n));
                i = j + 1;
                continue;
            }
        }
        i = start + 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_tmp() -> (tempfile::TempDir, MemoryKeyStore, Vault) {
        let dir = tempfile::tempdir().unwrap();
        let ks = MemoryKeyStore::new().unwrap();
        let path = Utf8PathBuf::from_path_buf(dir.path().join("vault.bin")).unwrap();
        let v = Vault::open(&path, &ks).unwrap();
        (dir, ks, v)
    }

    #[test]
    fn mint_numbers_per_kind_and_is_idempotent() {
        let (_d, _ks, v) = open_tmp();
        let a = v.mint(&TokenKind::Ssn, "000-12-3456").unwrap();
        let b = v.mint(&TokenKind::Ssn, "666-12-3456").unwrap();
        let c = v.mint(&TokenKind::Ein, "00-0000001").unwrap();
        assert_eq!(a.to_string(), "[SSN_1]");
        assert_eq!(b.to_string(), "[SSN_2]");
        assert_eq!(c.to_string(), "[EIN_1]");
        assert_eq!(v.mint(&TokenKind::Ssn, "000-12-3456").unwrap(), a);
        assert_eq!(v.len(), 3);
        assert!(!v.is_empty());
    }

    #[test]
    fn normalisation_unifies_formats() {
        let (_d, _ks, v) = open_tmp();
        let a = v.mint(&TokenKind::Ssn, "000-12-3456").unwrap();
        assert_eq!(v.mint(&TokenKind::Ssn, "000123456").unwrap(), a);
        assert_eq!(v.mint(&TokenKind::Ssn, " 000 12 3456 ").unwrap(), a);
        let p = v.mint(&TokenKind::Person, "Jane  Q. Placeholder").unwrap();
        assert_eq!(
            v.mint(&TokenKind::Person, "jane q. placeholder").unwrap(),
            p
        );
        assert_eq!(v.lookup(&TokenKind::Person, "JANE Q. PLACEHOLDER"), Some(p));
        assert_eq!(v.lookup(&TokenKind::Person, "Someone Else"), None);
        // Same value, different kind: different token.
        let e = v.mint(&TokenKind::Account, "000123456").unwrap();
        assert_eq!(e.kind, TokenKind::Account);
    }

    #[test]
    fn normalize_rules() {
        assert_eq!(normalize(&TokenKind::Ein, "00-000 0001"), "000000001");
        assert_eq!(normalize(&TokenKind::Account, "ab-12 3"), "AB123");
        assert_eq!(
            normalize(&TokenKind::Phone, "+1 (555) 010-0000"),
            "5550100000"
        );
        assert_eq!(
            normalize(&TokenKind::Org, "  Acme   Widgets "),
            "acme widgets"
        );
        assert_eq!(
            normalize(&TokenKind::Email, " Jo@Example.TEST "),
            "jo@example.test"
        );
    }

    #[test]
    fn empty_value_is_refused() {
        let (_d, _ks, v) = open_tmp();
        assert!(v.mint(&TokenKind::Ssn, " - ").is_err());
    }

    #[test]
    fn rehydrate_replaces_known_tokens_only() {
        let (_d, _ks, v) = open_tmp();
        v.mint(&TokenKind::Ssn, "000-12-3456").unwrap();
        v.mint(&TokenKind::Custom("plan id".into()), "PX-1")
            .unwrap();
        let out =
            v.rehydrate("SSN [SSN_1], plan [PLAN_ID_1], unknown [SSN_9], [not a token], [SSN_x]");
        assert_eq!(
            out,
            "SSN 000-12-3456, plan PX-1, unknown [SSN_9], [not a token], [SSN_x]"
        );
    }

    #[test]
    fn find_tokens_parses_labels_with_underscores() {
        let t = find_tokens("a [PLAN_ID_12] b [SSN_1]");
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].1, "PLAN_ID");
        assert_eq!(t[0].2, 12);
        assert_eq!(&"a [PLAN_ID_12] b [SSN_1]"[t[1].0.clone()], "[SSN_1]");
    }

    #[test]
    fn persists_across_reopen_and_counters_continue() {
        let (d, ks, v) = open_tmp();
        let a = v.mint(&TokenKind::Ssn, "000-12-3456").unwrap();
        drop(v);
        let path = Utf8PathBuf::from_path_buf(d.path().join("vault.bin")).unwrap();
        let v2 = Vault::open(&path, &ks).unwrap();
        assert_eq!(v2.mint(&TokenKind::Ssn, "000123456").unwrap(), a);
        let b = v2.mint(&TokenKind::Ssn, "666-00-0001").unwrap();
        assert_eq!(b.index, 2);
        assert_eq!(v2.rehydrate("[SSN_1]"), "000-12-3456");
    }

    #[test]
    fn file_is_encrypted_and_wrong_key_fails() {
        let (d, _ks, v) = open_tmp();
        v.mint(&TokenKind::Person, "Testy McFakename").unwrap();
        let path = Utf8PathBuf::from_path_buf(d.path().join("vault.bin")).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let hay = String::from_utf8_lossy(&bytes);
        assert!(!hay.contains("McFakename"));
        let other = MemoryKeyStore::with_key([7u8; 32]);
        assert!(matches!(Vault::open(&path, &other), Err(Error::Vault(_))));
    }

    #[test]
    fn seal_roundtrip_and_tamper_detect() {
        let (_d, _ks, v) = open_tmp();
        let blob = v.seal(b"hello").unwrap();
        assert_eq!(v.unseal(&blob).unwrap(), b"hello");
        let mut bad = blob.clone();
        let last = bad.len() - 1;
        bad[last] ^= 1;
        assert!(v.unseal(&bad).is_err());
    }

    #[test]
    fn debug_does_not_print_values() {
        let (_d, _ks, v) = open_tmp();
        v.mint(&TokenKind::Person, "Testy McFakename").unwrap();
        assert!(!format!("{v:?}").contains("McFakename"));
    }

    #[cfg(unix)]
    #[test]
    fn failed_persist_rolls_back_the_mint() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let dir = d.path().join("ro");
        std::fs::create_dir(&dir).unwrap();
        let path = Utf8PathBuf::from_path_buf(dir.join("vault.bin")).unwrap();
        let v = Vault::open(&path, &MemoryKeyStore::new().unwrap()).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o500)).unwrap();
        let r = v.mint(&TokenKind::Ssn, "000-12-3456");
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(r.is_err());
        assert!(v.is_empty());
        assert_eq!(v.lookup(&TokenKind::Ssn, "000-12-3456"), None);
        // The counter was rolled back too: the next mint is still _1.
        assert_eq!(v.mint(&TokenKind::Ssn, "000-12-3456").unwrap().index, 1);
    }

    #[test]
    fn memory_keystore_is_stable() {
        let ks = MemoryKeyStore::new().unwrap();
        assert_eq!(
            ks.load_or_create().unwrap(),
            ks.clone().load_or_create().unwrap()
        );
    }
}
