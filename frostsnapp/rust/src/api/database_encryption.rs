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
    pub fn get_database_state(&self, app_dir: String) -> Result<DbEncryptionState> {
        let db_file = Path::new(&app_dir).join("frostsnap.sqlite");

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
        open_database(&db_file, Some(&password))?;
        Ok(())
    }

    pub fn rekey_database(
        &self,
        app_dir: String,
        old_password: Option<String>,
        new_password: Option<String>,
    ) -> Result<(), DatabaseError> {
        let db_file = Path::new(&app_dir).join("frostsnap.sqlite");

        // Open with current password (if any)
        let db = open_database(&db_file, old_password.as_deref())?;

        // Rekey to new password (or remove encryption if new_password is None)
        let rekey_value = new_password.as_deref().unwrap_or("");
        db.pragma_update(None, "rekey", &rekey_value)
            .map_err(|e| DatabaseError::Other(format!("Failed to rekey database: {}", e)))?;

        Ok(())
    }
}
