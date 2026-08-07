//! One-time move of on-disk state from the old Cerberus identity to HELIX.
//!
//! The rename touches three things the user cannot afford to lose: the app
//! directory (models measured in gigabytes, the llama.cpp engine, installed
//! plugins), the encrypted database, and the AES master key in the OS
//! credential store that makes that database readable at all.
//!
//! The key is the dangerous one. `SecureDb::new` mints a fresh key when the
//! credential store reports `NoEntry`, which is correct on a first run and
//! catastrophic here: renaming the service without carrying the key across
//! looks exactly like a first run, and every existing row stays encrypted under
//! a key nobody has any more. So this runs *before* `SecureDb::new`, and any
//! failure is propagated rather than logged — an aborted launch is recoverable,
//! silently orphaned data is not.
//!
//! Every step is skipped when the destination already exists, so a normal
//! launch does a few `exists()` checks and returns.

use anyhow::Context;
use keyring::Entry;
use std::path::Path;

/// Credential-store service the master key used to live under.
const LEGACY_KEYRING_SERVICE: &str = "cerberusai_desktop";

/// The database's old filename inside the app directory.
const LEGACY_DB_FILE: &str = "cerberus.db";

/// Move everything the old install owned to its HELIX name.
///
/// `home` is the user's home directory; `app_dir` is the HELIX directory
/// inside it (passed in rather than recomputed so tests can point both at a
/// temp dir).
pub fn run(home: &Path, app_dir: &Path) -> Result<(), anyhow::Error> {
    migrate_app_dir(home, app_dir)?;
    migrate_db_files(app_dir)?;
    migrate_master_key()?;
    Ok(())
}

/// `~/.CerberusAI` → `~/.HELIX`.
///
/// A rename within the same volume is a directory-entry update, so this costs
/// the same whether the directory holds one file or 80 GB of models.
fn migrate_app_dir(home: &Path, app_dir: &Path) -> Result<(), anyhow::Error> {
    let legacy = home.join(crate::paths::LEGACY_APP_DIR_NAME);
    if app_dir.exists() || !legacy.exists() {
        return Ok(());
    }

    std::fs::rename(&legacy, app_dir).with_context(|| {
        format!(
            "Could not move the app directory from {} to {}",
            legacy.display(),
            app_dir.display()
        )
    })?;
    log::info!(
        "migrated app directory {} -> {}",
        legacy.display(),
        app_dir.display()
    );
    Ok(())
}

/// `cerberus.db` → `helix.db`, including the WAL sidecars.
///
/// `-wal` holds committed transactions not yet folded into the main file, so
/// leaving it behind under the old name discards the most recent writes — the
/// newest chats, exactly what the user would notice first. SQLite derives both
/// sidecar names from the database filename, so they have to travel with it.
fn migrate_db_files(app_dir: &Path) -> Result<(), anyhow::Error> {
    for suffix in ["", "-wal", "-shm"] {
        let legacy = app_dir.join(format!("{LEGACY_DB_FILE}{suffix}"));
        let current = app_dir.join(format!("{}{suffix}", crate::secure_db::DB_FILE));
        if current.exists() || !legacy.exists() {
            continue;
        }
        std::fs::rename(&legacy, &current)
            .with_context(|| format!("Could not rename {}", legacy.display()))?;
        log::info!("migrated database file {}", legacy.display());
    }
    Ok(())
}

/// Copy the AES master key to the new credential-store service.
///
/// The old entry is deliberately left in place. Deleting it would leave exactly
/// one copy of a value that cannot be regenerated, and the credential store is
/// the component this codebase has already had lose a write silently — if the
/// new entry later proves bad, the old one is the only way back.
fn migrate_master_key() -> Result<(), anyhow::Error> {
    let current = Entry::new(crate::secure_db::SERVICE_NAME, crate::secure_db::ACCOUNT_NAME)?;
    match current.get_password() {
        // Already migrated, or a genuine first run that will mint its own key.
        Ok(_) => return Ok(()),
        Err(keyring::Error::NoEntry) => {}
        Err(e) => {
            return Err(anyhow::anyhow!(
                "Could not read the master key from the OS credential store: {e}"
            ))
        }
    }

    let legacy = Entry::new(LEGACY_KEYRING_SERVICE, crate::secure_db::ACCOUNT_NAME)?;
    let hex_key = match legacy.get_password() {
        Ok(k) => k,
        // Nothing to carry across: a clean install. `SecureDb::new` generates a
        // key, which is the right outcome.
        Err(keyring::Error::NoEntry) => return Ok(()),
        Err(e) => {
            return Err(anyhow::anyhow!(
                "Found no HELIX master key and could not read the previous one to migrate it ({e}). \
                 Continuing would generate a new key and leave existing data undecryptable."
            ))
        }
    };

    current.set_password(&hex_key)?;

    // Read back before trusting it, for the same reason `SecureDb::new` does:
    // with no platform backend compiled in, keyring accepts writes into an
    // in-memory mock and reports NoEntry on the next launch. Here that would be
    // worse than at first run — the next launch would mint a fresh key over
    // data encrypted with this one.
    match current.get_password() {
        Ok(stored) if stored == hex_key => {
            log::info!("migrated master key to the HELIX credential-store entry");
            Ok(())
        }
        Ok(_) => anyhow::bail!(
            "Credential store returned a different master key than was just migrated; \
             refusing to continue with storage that would lose data."
        ),
        Err(e) => anyhow::bail!(
            "Migrated master key did not persist to the OS credential store ({e}). \
             Encrypted storage would be lost on exit."
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A throwaway home directory. Named per test rather than randomised, and
    /// cleared on the way in, so a crashed run leaves nothing that poisons the
    /// next one. Matches how the engine tests get their scratch space.
    fn scratch_home(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("helix-migrate-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    fn write(path: &Path, contents: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    /// The upgrade case: an install that predates the rename.
    #[test]
    fn moves_the_previous_install_across() {
        let home = scratch_home("upgrade");
        let legacy = home.join(crate::paths::LEGACY_APP_DIR_NAME);
        write(&legacy.join(LEGACY_DB_FILE), "db");
        write(&legacy.join(format!("{LEGACY_DB_FILE}-wal")), "wal");
        write(&legacy.join("models").join("a.gguf"), "weights");

        let app_dir = crate::paths::app_dir_in(&home);
        migrate_app_dir(&home, &app_dir).unwrap();
        migrate_db_files(&app_dir).unwrap();

        assert!(!legacy.exists(), "old directory should be gone, not copied");
        assert_eq!(
            std::fs::read_to_string(app_dir.join(crate::secure_db::DB_FILE)).unwrap(),
            "db"
        );
        assert_eq!(
            std::fs::read_to_string(app_dir.join(format!("{}-wal", crate::secure_db::DB_FILE)))
                .unwrap(),
            "wal",
            "the WAL holds the newest committed writes and must travel with the db"
        );
        assert_eq!(
            std::fs::read_to_string(app_dir.join("models").join("a.gguf")).unwrap(),
            "weights"
        );

        let _ = std::fs::remove_dir_all(&home);
    }

    /// Migration runs on every launch, so a second pass over an already-migrated
    /// install must not touch anything. Renaming onto an existing `~/.HELIX`
    /// would destroy whatever the user accumulated since the first migration.
    #[test]
    fn never_overwrites_an_existing_helix_install() {
        let home = scratch_home("idempotent");
        let legacy = home.join(crate::paths::LEGACY_APP_DIR_NAME);
        write(&legacy.join(LEGACY_DB_FILE), "stale");

        let app_dir = crate::paths::app_dir_in(&home);
        write(&app_dir.join(crate::secure_db::DB_FILE), "current");

        migrate_app_dir(&home, &app_dir).unwrap();
        migrate_db_files(&app_dir).unwrap();

        assert_eq!(
            std::fs::read_to_string(app_dir.join(crate::secure_db::DB_FILE)).unwrap(),
            "current",
            "an existing HELIX database must win over a leftover Cerberus one"
        );
        assert!(legacy.exists(), "the leftover is left alone, not silently deleted");

        let _ = std::fs::remove_dir_all(&home);
    }

    /// A clean install has nothing to migrate and must not error — that would
    /// block first launch for every new user.
    #[test]
    fn clean_install_is_a_no_op() {
        let home = scratch_home("clean");
        let app_dir = crate::paths::app_dir_in(&home);

        migrate_app_dir(&home, &app_dir).unwrap();
        migrate_db_files(&app_dir).unwrap();

        assert!(!app_dir.exists(), "nothing should be created out of thin air");

        let _ = std::fs::remove_dir_all(&home);
    }
}
