//! Secure SQLite Storage for HELIX
//!
//! Uses standard rusqlite, but encrypts sensitive columns (like API keys
//! and chat content) using AES-256-GCM. The encryption key is stored securely
//! in the OS keyring (Windows Credential Manager / macOS Keychain).

use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use keyring::Entry;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::PathBuf;
use std::sync::Mutex;

pub const SERVICE_NAME: &str = "helix_desktop";
pub const ACCOUNT_NAME: &str = "master_key";

/// The database's filename inside the app directory.
pub const DB_FILE: &str = "helix.db";

/// Settings keys that carry the old brand, and what they are called now.
///
/// The frontend reads the new names; without this the user's chat history and
/// model choice are still in the database but nothing looks for them, which
/// presents as a first-run empty app. See [`crate::migrate`] for the rest of
/// the rename.
const RENAMED_KV_KEYS: [(&str, &str); 2] = [
    ("cerberus.chats.v1", "helix.chats.v1"),
    ("cerberus.model.v1", "helix.model.v1"),
];

pub struct SecureDb {
    conn: Mutex<Connection>,
    cipher: Aes256Gcm,
}

/// Tauri state wrapper that is registered even when storage failed to open.
///
/// If `SecureDb::new` fails, the handle is absent but the reason is kept, and
/// every `db_*` command answers with that reason. Skipping `app.manage` instead
/// leaves Tauri to reject the call with "state not found", which names neither
/// the subsystem nor the cause — that is how a broken credential store stayed
/// invisible for an entire release.
pub struct SecureDbState(Result<SecureDb, String>);

impl SecureDbState {
    pub fn new(db: Result<SecureDb, anyhow::Error>) -> Self {
        Self(db.map_err(|e| format!("{e:#}")))
    }

    /// The storage handle, or the startup error explaining why there isn't one.
    pub fn get(&self) -> Result<&SecureDb, String> {
        self.0.as_ref().map_err(Clone::clone)
    }
}

impl SecureDb {
    pub fn new(app_dir: PathBuf) -> Result<Self, anyhow::Error> {
        // 1. Get or create master key.
        //
        // Only a genuinely absent entry may mint a new key. Every other error —
        // a locked keychain, a denied credential prompt, a transient backend
        // failure — is propagated. Treating those as "no key yet" is what makes
        // this unrecoverable: a fresh key is generated, the old ciphertext in
        // helix.db stays behind encrypted under a key nobody has any more,
        // and every later read fails forever. Failing to start is recoverable;
        // silently orphaning the user's data is not.
        let entry = Entry::new(SERVICE_NAME, ACCOUNT_NAME)?;
        let key_bytes = match entry.get_password() {
            Ok(hex_key) => hex::decode(hex_key)?,
            Err(keyring::Error::NoEntry) => {
                let key = Aes256Gcm::generate_key(OsRng);
                let hex_key = hex::encode(key);
                entry.set_password(&hex_key)?;

                // Read back before trusting it. `set_password` returning Ok is
                // not proof of durability: with no platform backend feature
                // enabled, keyring silently substitutes an in-memory mock store
                // that accepts writes and returns NoEntry on the next launch.
                // That shipped, and every key written this way was lost at exit.
                match entry.get_password() {
                    Ok(stored) if stored == hex_key => key.to_vec(),
                    Ok(_) => anyhow::bail!(
                        "Credential store returned a different master key than was just written; \
                         refusing to continue with storage that would lose data."
                    ),
                    Err(e) => anyhow::bail!(
                        "Master key did not persist to the OS credential store ({e}). \
                         Encrypted storage would be lost on exit."
                    ),
                }
            }
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "Could not read the master key from the OS credential store: {e}"
                ))
            }
        };

        // `from_slice` panics on a wrong length, and a stored key can be short
        // if an older build wrote a truncated value.
        if key_bytes.len() != 32 {
            anyhow::bail!(
                "Master key in the OS credential store is {} bytes, expected 32.",
                key_bytes.len()
            );
        }
        let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
        let cipher = Aes256Gcm::new(key);

        // 2. Open SQLite DB
        let db_path = app_dir.join(DB_FILE);
        let conn = Connection::open(db_path)?;

        // 3. Setup tables
        conn.execute(
            "CREATE TABLE IF NOT EXISTS kv_store (
                key TEXT PRIMARY KEY,
                encrypted_value BLOB NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS chats (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                encrypted_messages BLOB NOT NULL
            )",
            [],
        )?;

        // 4. Carry pre-rename settings over to their new keys.
        rename_legacy_kv_keys(&conn)?;

        Ok(Self {
            conn: Mutex::new(conn),
            cipher,
        })
    }

    pub fn set_kv(&self, key: &str, value: &str) -> Result<(), anyhow::Error> {
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng); // 96-bits
        let encrypted = self.cipher.encrypt(&nonce, value.as_bytes())
            .map_err(|e| anyhow::anyhow!("Encryption failed: {:?}", e))?;

        // Prepend nonce to encrypted data
        let mut payload = nonce.to_vec();
        payload.extend_from_slice(&encrypted);

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO kv_store (key, encrypted_value) VALUES (?1, ?2)",
            params![key, payload],
        )?;
        Ok(())
    }

    pub fn get_kv(&self, key: &str) -> Result<Option<String>, anyhow::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT encrypted_value FROM kv_store WHERE key = ?1")?;

        let payload: Option<Vec<u8>> = stmt.query_row(params![key], |row| row.get(0))
            .optional()?;

        let Some(data) = payload else { return Ok(None) };

        // AES-GCM is authenticated, so a decryption failure is never benign: the
        // row was truncated, corrupted on disk, or written under a different key.
        // Report it. An earlier version of this code deleted such rows to get
        // past the credential-store bug — that bug is fixed at the source now
        // (see `Cargo.toml`, the keyring backend features), and deleting on a
        // read is destroying user data to silence a symptom.
        if data.len() < 12 {
            anyhow::bail!("Stored value for '{key}' is truncated ({} bytes).", data.len());
        }
        let (nonce_bytes, ciphertext) = data.split_at(12);
        let plaintext = self
            .cipher
            .decrypt(Nonce::from_slice(nonce_bytes), ciphertext)
            .map_err(|_| {
                anyhow::anyhow!(
                    "Could not decrypt stored value for '{key}'. The data is corrupt, \
                     or it was encrypted with a different master key."
                )
            })?;

        Ok(Some(String::from_utf8(plaintext)?))
    }

    pub fn delete_kv(&self, key: &str) -> Result<(), anyhow::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM kv_store WHERE key = ?1", params![key])?;
        Ok(())
    }
}

/// Point pre-rename settings rows at their new keys.
///
/// Only the key column changes; the values stay encrypted under the same master
/// key, so nothing is decrypted or re-encrypted here. The `NOT EXISTS` guard
/// makes this a no-op once done, and means a value already written under the
/// new name is never clobbered by a stale one.
fn rename_legacy_kv_keys(conn: &Connection) -> Result<(), anyhow::Error> {
    for (legacy, current) in RENAMED_KV_KEYS {
        conn.execute(
            "UPDATE kv_store SET key = ?1 WHERE key = ?2
             AND NOT EXISTS (SELECT 1 FROM kv_store WHERE key = ?1)",
            params![current, legacy],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A failed open must reach the caller as the actual reason. Before this,
    /// `app.manage` was skipped on failure and Tauri answered every `db_*` call
    /// with "state not found", naming neither the subsystem nor the cause.
    #[test]
    fn failed_open_reports_the_underlying_cause() {
        let state = SecureDbState::new(Err(anyhow::anyhow!("credential store is locked")));
        // `err()` rather than `expect_err`: the latter needs `SecureDb: Debug`,
        // and deriving that would put the cipher into debug output.
        let err = state.get().err().expect("must not report a usable handle");
        assert!(
            err.contains("credential store is locked"),
            "error should carry the cause, got: {err}"
        );
    }

    /// An in-memory `kv_store` seeded with `rows`, for exercising the rename
    /// without a credential store — `SecureDb::new` would need a real master
    /// key, and a test has no business writing to the user's keychain.
    fn kv_store(rows: &[(&str, &str)]) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE kv_store (key TEXT PRIMARY KEY, encrypted_value BLOB NOT NULL)",
            [],
        )
        .unwrap();
        for (key, value) in rows {
            conn.execute(
                "INSERT INTO kv_store (key, encrypted_value) VALUES (?1, ?2)",
                params![key, value.as_bytes()],
            )
            .unwrap();
        }
        conn
    }

    fn value_at(conn: &Connection, key: &str) -> Option<String> {
        conn.query_row(
            "SELECT encrypted_value FROM kv_store WHERE key = ?1",
            params![key],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()
        .unwrap()
        .map(|v| String::from_utf8(v).unwrap())
    }

    /// Chat history and the model choice survive the rebrand. The frontend
    /// reads the new keys, so without this the rows are still in the database
    /// and the app looks freshly installed.
    #[test]
    fn settings_follow_the_rename() {
        let conn = kv_store(&[
            ("cerberus.chats.v1", "history"),
            ("cerberus.model.v1", "a-model"),
            ("mcp-plugins", "untouched"),
        ]);

        rename_legacy_kv_keys(&conn).unwrap();

        assert_eq!(value_at(&conn, "helix.chats.v1").as_deref(), Some("history"));
        assert_eq!(value_at(&conn, "helix.model.v1").as_deref(), Some("a-model"));
        assert_eq!(value_at(&conn, "cerberus.chats.v1"), None);
        assert_eq!(
            value_at(&conn, "mcp-plugins").as_deref(),
            Some("untouched"),
            "keys outside the rename table must be left alone"
        );
    }

    /// Runs on every open, so it has to be safe to repeat — and a row already
    /// written under the new key is the live one. Overwriting it with a stale
    /// pre-rename row would roll the user's chats back.
    #[test]
    fn rename_never_overwrites_a_newer_value() {
        let conn = kv_store(&[
            ("cerberus.chats.v1", "stale"),
            ("helix.chats.v1", "current"),
        ]);

        rename_legacy_kv_keys(&conn).unwrap();
        rename_legacy_kv_keys(&conn).unwrap();

        assert_eq!(value_at(&conn, "helix.chats.v1").as_deref(), Some("current"));
        assert_eq!(
            value_at(&conn, "cerberus.chats.v1").as_deref(),
            Some("stale"),
            "the superseded row is left in place rather than deleted"
        );
    }
}
