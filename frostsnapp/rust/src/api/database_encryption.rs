use crate::api::bitcoin::BitcoinNetworkExt;
use crate::api::super_wallet::BitcoinNetwork;
use anyhow::Result;
use std::path::Path;
use tracing::{event, Level};

#[derive(Debug, PartialEq)]
pub enum DbEncryptionState {
    Fresh,                  // No database exists
    ExistingUnencrypted,    // Database exists and truly unencrypted (no SQLCipher)
    ExistingEncryptedEmpty, // Database exists, encrypted with empty password (treated as unencrypted for UX)
    ExistingEncrypted,      // Database exists and encrypted with real password
}

#[derive(Debug)]
pub enum DatabaseError {
    WrongPassword,
    Other(String),
}

impl std::fmt::Display for DatabaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DatabaseError::WrongPassword => write!(f, "Wrong password"),
            DatabaseError::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for DatabaseError {}

/// Get all database files that exist in the app directory
fn get_all_database_files(app_dir: &str) -> Vec<std::path::PathBuf> {
    let mut all_dbs = Vec::new();

    // Main database
    let main_db = Path::new(app_dir).join("frostsnap.sqlite");
    if main_db.exists() {
        all_dbs.push(main_db);
    }

    // BDK databases
    for network in [
        BitcoinNetwork::Bitcoin,
        BitcoinNetwork::Testnet,
        BitcoinNetwork::Signet,
        BitcoinNetwork::Regtest,
    ] {
        let bdk_file = network.bdk_file(app_dir);
        if bdk_file.exists() {
            all_dbs.push(bdk_file);
        }
    }

    all_dbs
}

/// Get all backup files that exist for our databases
fn get_all_backup_files(app_dir: &str) -> Vec<std::path::PathBuf> {
    get_all_database_files(app_dir)
        .into_iter()
        .map(|db_path| db_path.with_extension("backup"))
        .filter(|backup_path| backup_path.exists())
        .collect()
}

pub(crate) fn open_database(
    db_path: &Path,
    password: Option<&str>,
) -> Result<rusqlite::Connection, DatabaseError> {
    let conn = rusqlite::Connection::open(db_path)
        .map_err(|e| DatabaseError::Other(format!("Failed to open database: {}", e)))?;

    #[cfg(not(target_os = "android"))]
    {
        if let Some(password) = password {
            // For empty password, use x'' syntax to match what we use in REKEY
            let key_value = if password.is_empty() { "x''" } else { password };
            conn.pragma_update(None, "key", key_value)
                .map_err(|e| DatabaseError::Other(format!("Failed to set key: {}", e)))?;

            // Validate password
            match conn.pragma_query_value(None, "page_count", |row| row.get::<_, i32>(0)) {
                Ok(_) => {}
                Err(rusqlite::Error::SqliteFailure(err, _))
                    if err.code == rusqlite::ErrorCode::NotADatabase =>
                {
                    return Err(DatabaseError::WrongPassword);
                }
                Err(e) => return Err(DatabaseError::Other(format!("Database error: {}", e))),
            }
        }
    }

    #[cfg(target_os = "android")]
    let _ = password;

    Ok(conn)
}

/// Change database encryption: unencrypted → encrypted, encrypted → encrypted, or encrypted → unencrypted
/// Uses SQLCipher REKEY for encrypted databases, export method for unencrypted source
fn change_database_encryption(
    db_path: &Path,
    old_password: Option<&str>,
    new_password: &str,
) -> Result<(), DatabaseError> {
    // If source is encrypted, use REKEY (simplest and fastest)
    if let Some(old_pwd) = old_password {
        event!(
            Level::DEBUG,
            "Rekeying database with old_pwd={:?}, new_password={:?}",
            if old_pwd.is_empty() {
                "<empty>"
            } else {
                "<set>"
            },
            if new_password.is_empty() {
                "<empty>"
            } else {
                "<set>"
            }
        );
        let db = open_database(db_path, Some(old_pwd))?;

        // For empty password, use x'' syntax with pragma_update
        if new_password.is_empty() {
            // SQLCipher requires x'' for empty/null keys
            db.pragma_update(None, "rekey", "x''")
                .map_err(|e| DatabaseError::Other(format!("Failed to rekey to empty: {}", e)))?;
        } else {
            db.pragma_update(None, "rekey", new_password)
                .map_err(|e| DatabaseError::Other(format!("Failed to rekey: {}", e)))?;
        }
        event!(Level::DEBUG, "Rekey completed successfully");
        return Ok(());
    }

    // Source is unencrypted, must use export method
    let temp_path = db_path.with_extension("tmp");

    // Remove any existing temp file from previous failed attempts
    let _ = std::fs::remove_file(&temp_path);

    // Create new encrypted database
    {
        let new_db = rusqlite::Connection::open(&temp_path)
            .map_err(|e| DatabaseError::Other(format!("Failed to create temp db: {}", e)))?;
        new_db
            .pragma_update(None, "key", new_password)
            .map_err(|e| DatabaseError::Other(format!("Failed to set key: {}", e)))?;
    }

    let old_db = rusqlite::Connection::open(db_path)
        .map_err(|e| DatabaseError::Other(format!("Failed to open source db: {}", e)))?;

    old_db
        .execute(
            "ATTACH DATABASE ?1 AS encrypted KEY ?2",
            rusqlite::params![temp_path.to_string_lossy(), new_password],
        )
        .map_err(|e| DatabaseError::Other(format!("Failed to attach encrypted db: {}", e)))?;

    old_db
        .query_row("SELECT sqlcipher_export('encrypted')", [], |_| Ok(()))
        .map_err(|e| DatabaseError::Other(format!("Failed to export data: {}", e)))?;

    old_db
        .execute("DETACH DATABASE encrypted", [])
        .map_err(|e| DatabaseError::Other(format!("Failed to detach db: {}", e)))?;

    drop(old_db);

    // Atomic replacement
    std::fs::remove_file(db_path)
        .map_err(|e| DatabaseError::Other(format!("Failed to remove original: {}", e)))?;
    std::fs::rename(&temp_path, db_path)
        .map_err(|e| DatabaseError::Other(format!("Failed to rename temp file: {}", e)))?;

    Ok(())
}

impl super::Api {
    pub fn get_database_state(&self, app_dir: &str) -> Result<DbEncryptionState> {
        get_db_state(app_dir)
    }
    pub fn attempt_database_password(
        &self,
        app_dir: String,
        password: String,
    ) -> Result<(), DatabaseError> {
        let db_file = Path::new(&app_dir).join("frostsnap.sqlite");
        open_database(&db_file, Some(&password)).map(|_| ())
    }
}

pub fn get_db_state(app_dir: &str) -> Result<DbEncryptionState> {
    let db_file = Path::new(app_dir).join("frostsnap.sqlite");
    if !db_file.exists() {
        return Ok(DbEncryptionState::Fresh);
    }

    // Try opening without password
    match rusqlite::Connection::open(&db_file) {
        Ok(conn) => match conn.pragma_query_value(None, "page_count", |row| row.get::<_, i32>(0)) {
            Ok(_) => {
                // Successfully opened without password - truly unencrypted
                Ok(DbEncryptionState::ExistingUnencrypted)
            }
            Err(rusqlite::Error::SqliteFailure(err, _))
                if err.code == rusqlite::ErrorCode::NotADatabase =>
            {
                // Failed without password - try with empty password
                match open_database(&db_file, Some("")) {
                    Ok(_) => {
                        // Opens with empty password
                        event!(Level::DEBUG, "Database encrypted with empty password");
                        Ok(DbEncryptionState::ExistingEncryptedEmpty)
                    }
                    Err(_) => {
                        // Doesn't open with empty password - encrypted with real password
                        Ok(DbEncryptionState::ExistingEncrypted)
                    }
                }
            }
            Err(e) => {
                event!(Level::ERROR, "Database check failed: {}", e);
                Ok(DbEncryptionState::ExistingUnencrypted)
            }
        },
        Err(_) => Ok(DbEncryptionState::ExistingUnencrypted),
    }
}

pub fn apply_password_change(
    main_db_state: DbEncryptionState,
    app_dir: String,
    old_password: String,
    new_password: String,
) -> Result<()> {
    event!(Level::INFO, "Starting password change operation");

    // Find backup files from previous failed attempt and restore if needed
    let backup_files = get_all_backup_files(&app_dir);
    if !backup_files.is_empty() {
        event!(
            Level::WARN,
            "Found backup files from previous failed attempt - restoring"
        );
        for backup_file in backup_files {
            let original_file = backup_file.with_extension("sqlite");
            event!(
                Level::INFO,
                "Restoring {} from backup",
                original_file.display()
            );

            if original_file.exists() {
                std::fs::remove_file(&original_file)?;
            }
            std::fs::rename(&backup_file, &original_file)?;
        }
    }

    // Get all database files that exist
    let all_dbs = get_all_database_files(&app_dir);

    // Create backups for all databases first
    for db_file in &all_dbs {
        let backup_file = db_file.with_extension("backup");
        std::fs::copy(db_file, &backup_file)?;
        event!(Level::DEBUG, "Created backup for {}", db_file.display());
    }

    // Now rekey all databases using the same operation
    for db_path in &all_dbs {
        let result = match main_db_state {
            DbEncryptionState::ExistingUnencrypted => {
                if new_password.is_empty() {
                    Ok(()) // No encryption needed
                } else {
                    change_database_encryption(db_path, None, &new_password)
                }
            }
            DbEncryptionState::ExistingEncryptedEmpty => {
                // Encrypted with empty password - use REKEY with old_password=""
                if new_password.is_empty() {
                    Ok(()) // Already encrypted with empty password
                } else {
                    change_database_encryption(db_path, Some(&old_password), &new_password)
                }
            }
            DbEncryptionState::ExistingEncrypted => {
                change_database_encryption(db_path, Some(&old_password), &new_password)
            }
            DbEncryptionState::Fresh => {
                if !new_password.is_empty() {
                    // Create new encrypted database
                    match rusqlite::Connection::open(db_path) {
                        Ok(db) => db
                            .pragma_update(None, "key", &new_password)
                            .map_err(|e| DatabaseError::Other(format!("Failed to set key: {}", e))),
                        Err(e) => Err(DatabaseError::Other(format!("Failed to create db: {}", e))),
                    }
                } else {
                    Ok(()) // Create unencrypted database (default behavior)
                }
            }
        };

        if let Err(e) = result {
            event!(Level::ERROR, "Failed to rekey {}: {}", db_path.display(), e);

            // Restore all databases from backups
            for db_file in &all_dbs {
                let backup_file = db_file.with_extension("backup");
                if backup_file.exists() {
                    let _ = std::fs::remove_file(db_file);
                    let _ = std::fs::rename(&backup_file, db_file);
                }
            }

            return Err(anyhow::anyhow!(
                "Rekey failed on {}: {}",
                db_path.display(),
                e
            ));
        }

        event!(Level::INFO, "Successfully rekeyed {}", db_path.display());
    }

    // Success! Clean up backups
    for db_file in &all_dbs {
        let backup_file = db_file.with_extension("backup");
        let _ = std::fs::remove_file(&backup_file);
    }

    event!(
        Level::INFO,
        "Password change operation completed successfully"
    );

    Ok(())
}
