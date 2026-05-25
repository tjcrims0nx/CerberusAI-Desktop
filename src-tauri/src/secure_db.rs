//! Secure SQLite Storage for CerberusAI
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

const SERVICE_NAME: &str = "cerberusai_desktop";
const ACCOUNT_NAME: &str = "master_key";

pub struct SecureDb {
    conn: Mutex<Connection>,
    cipher: Aes256Gcm,
}

impl SecureDb {
    pub fn new(app_dir: PathBuf) -> Result<Self, anyhow::Error> {
        // 1. Get or create master key
        let entry = Entry::new(SERVICE_NAME, ACCOUNT_NAME)?;
        let key_bytes = match entry.get_password() {
            Ok(hex_key) => hex::decode(hex_key)?,
            Err(_) => {
                // Generate new key
                let key = Aes256Gcm::generate_key(OsRng);
                let hex_key = hex::encode(key);
                entry.set_password(&hex_key)?;
                key.to_vec()
            }
        };

        let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
        let cipher = Aes256Gcm::new(key);

        // 2. Open SQLite DB
        let db_path = app_dir.join("cerberus.db");
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

        if let Some(data) = payload {
            if data.len() < 12 {
                return Err(anyhow::anyhow!("Invalid encrypted payload length"));
            }
            let (nonce_bytes, ciphertext) = data.split_at(12);
            let nonce = Nonce::from_slice(nonce_bytes);

            let decrypted = self.cipher.decrypt(nonce, ciphertext)
                .map_err(|e| anyhow::anyhow!("Decryption failed: {:?}", e))?;

            Ok(Some(String::from_utf8(decrypted)?))
        } else {
            Ok(None)
        }
    }

    pub fn delete_kv(&self, key: &str) -> Result<(), anyhow::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM kv_store WHERE key = ?1", params![key])?;
        Ok(())
    }
}
