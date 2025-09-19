use crate::api::bitcoin::BitcoinNetworkExt;
use crate::api::super_wallet::BitcoinNetwork;
use anyhow::Result;
use std::path::Path;
use tracing::{event, Level};

#[derive(Debug)]
pub enum DbEncryptionState {
    Fresh,               // No database exists
    ExistingUnencrypted, // Database exists and not encrypted
    ExistingEncrypted,   // Database exists and encrypted
}

#[derive(Debug)]
pub enum DatabaseError {
    WrongPassword,
    Other(String),
}

#[derive(bincode::Encode, bincode::Decode)]
struct ReKeyContents {
    old_password: String,
    new_password: String,
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

pub(crate) fn open_database(
    db_path: &Path,
    password: Option<&str>,
) -> Result<rusqlite::Connection, DatabaseError> {
    let conn = rusqlite::Connection::open(db_path)
        .map_err(|e| DatabaseError::Other(format!("Failed to open database: {}", e)))?;

    #[cfg(not(target_os = "android"))]
    {
        if let Some(password) = password {
            conn.pragma_update(None, "key", password)
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

impl super::Api {
    pub fn get_database_state(&self, app_dir: &str) -> Result<DbEncryptionState> {
        let db_file = Path::new(app_dir).join("frostsnap.sqlite");

        if !db_file.exists() {
            return Ok(DbEncryptionState::Fresh);
        }

        let is_encrypted = {
            if let Ok(conn) = rusqlite::Connection::open(&db_file) {
                match conn.pragma_query_value(None, "page_count", |row| row.get::<_, i32>(0)) {
                    Ok(_) => false,
                    Err(rusqlite::Error::SqliteFailure(err, _))
                        if err.code == rusqlite::ErrorCode::NotADatabase =>
                    {
                        true
                    }
                    Err(e) => {
                        event!(
                            Level::ERROR,
                            e = e.to_string(),
                            "failed to check database encryption"
                        );
                        false
                    }
                }
            } else {
                false
            }
        };

        if is_encrypted {
            Ok(DbEncryptionState::ExistingEncrypted)
        } else {
            Ok(DbEncryptionState::ExistingUnencrypted)
        }
    }

    pub fn attempt_database_password(
        &self,
        app_dir: String,
        password: String,
    ) -> Result<(), DatabaseError> {
        let db_file = Path::new(&app_dir).join("frostsnap.sqlite");
        if let Err(e) = open_database(&db_file, Some(&password)) {
            event!(Level::INFO, e = e.to_string(), "Password attempt failed");
            return Err(e);
        }
        Ok(())
    }

    pub fn schedule_rekey(
        &self,
        app_dir: String,
        old_password: String,
        new_password: String,
    ) -> Result<()> {
        let rekey_file = Path::new(&app_dir).join(".pending_rekey");
        let data = ReKeyContents {
            old_password,
            new_password,
        };
        let encoded = bincode::encode_to_vec(&data, bincode::config::standard())?;
        std::fs::write(rekey_file, encoded)?;
        Ok(())
    }

    pub fn check_and_apply_pending_rekey(&self, app_dir: String) -> Result<()> {
        let rekey_file = Path::new(&app_dir).join(".pending_rekey");
        event!(Level::INFO, "checking for pending rekey");
        if !rekey_file.exists() {
            return Ok(());
        }
        event!(Level::INFO, "pending rekey found!");
        let content = std::fs::read(&rekey_file)?;
        let ReKeyContents {
            old_password,
            new_password,
        } = bincode::decode_from_slice(&content, bincode::config::standard())?.0;

        let db_file = Path::new(&app_dir).join("frostsnap.sqlite");
        let db_encryption_state = self.get_database_state(&app_dir)?;

        match db_encryption_state {
            DbEncryptionState::ExistingUnencrypted => {
                // Migrate existing unencrypted database to encrypted
                let temp_file = db_file.with_extension("sqlite.tmp");

                // Create new encrypted database
                let new_db = rusqlite::Connection::open(&temp_file)?;
                new_db.pragma_update(None, "key", &new_password)?;
                drop(new_db);

                // Export all data from old to new
                let old_db = rusqlite::Connection::open(&db_file)?;
                old_db.execute(
                    &format!(
                        "ATTACH DATABASE '{}' AS encrypted KEY '{}'",
                        temp_file.display(),
                        new_password.replace("'", "''")
                    ),
                    [],
                )?;
                old_db.query_row("SELECT sqlcipher_export('encrypted')", [], |_| Ok(()))?;
                old_db.execute("DETACH DATABASE encrypted", [])?;
                drop(old_db);

                // Replace old with encrypted
                std::fs::remove_file(&db_file)?;
                std::fs::rename(&temp_file, &db_file)?;

                event!(Level::INFO, "Migrated database to encrypted");
            }
            DbEncryptionState::ExistingEncrypted => {
                // Change password of already encrypted database
                let db = open_database(&db_file, Some(&old_password))?;
                db.pragma_update(None, "rekey", &new_password)?;
                event!(Level::INFO, "Changed database password");
            }
            DbEncryptionState::Fresh => {
                // No existing database - create new encrypted one if password provided
                if !new_password.is_empty() {
                    let db = rusqlite::Connection::open(&db_file)?;
                    db.pragma_update(None, "key", &new_password)?;
                    event!(Level::INFO, "Created new encrypted database");
                }
                // If new_password is empty, just let the app create unencrypted DB normally
            }
        }

        {
            for network in [
                BitcoinNetwork::Bitcoin,
                BitcoinNetwork::Testnet,
                BitcoinNetwork::Signet,
                BitcoinNetwork::Regtest,
            ] {
                let bdk_file = network.bdk_file(&app_dir);
                if bdk_file.exists() {
                    let bdk_state = {
                        match rusqlite::Connection::open(&bdk_file) {
                            Ok(db) => match db.prepare("SELECT name FROM sqlite_master LIMIT 1") {
                                Ok(_) => DbEncryptionState::ExistingUnencrypted,
                                Err(_) => DbEncryptionState::ExistingEncrypted,
                            },
                            Err(_) => continue, // Skip if can't open
                        }
                    };

                    match bdk_state {
                        DbEncryptionState::ExistingUnencrypted => {
                            // Migrate BDK database to encrypted
                            let temp_file = bdk_file.with_extension("tmp");
                            let new_db = rusqlite::Connection::open(&temp_file)?;
                            new_db.pragma_update(None, "key", &new_password)?;
                            drop(new_db);

                            let old_db = rusqlite::Connection::open(&bdk_file)?;
                            old_db.execute(
                                &format!(
                                    "ATTACH DATABASE '{}' AS encrypted KEY '{}'",
                                    temp_file.display(),
                                    new_password.replace("'", "''")
                                ),
                                [],
                            )?;
                            old_db.query_row("SELECT sqlcipher_export('encrypted')", [], |_| {
                                Ok(())
                            })?;
                            old_db.execute("DETACH DATABASE encrypted", [])?;
                            drop(old_db);

                            std::fs::remove_file(&bdk_file)?;
                            std::fs::rename(&temp_file, &bdk_file)?;
                            event!(
                                Level::INFO,
                                "Migrated BDK database to encrypted: {}",
                                network
                            );
                        }
                        DbEncryptionState::ExistingEncrypted => {
                            // Rekey existing encrypted BDK database
                            let db = open_database(&bdk_file, Some(&old_password))?;
                            db.pragma_update(None, "rekey", &new_password)?;
                            event!(Level::INFO, "Rekeyed BDK database: {}", network);
                        }
                        _ => {}
                    }
                }
            }
        }

        std::fs::remove_file(rekey_file)?;
        Ok(())
    }
}
