use crate::db::Database;
use std::io;

/// Get the next serial number from the database
pub fn get_next() -> io::Result<u32> {
    let db = Database::new().map_err(|e| io::Error::other(format!("Database error: {e}")))?;

    db.get_next_serial()
        .map_err(|e| io::Error::other(format!("Failed to get next serial: {e}")))
}

/// Get current serial number without incrementing (for status/debugging)
pub fn get_current() -> io::Result<u32> {
    let db = Database::new().map_err(|e| io::Error::other(format!("Database error: {e}")))?;

    db.get_current_serial()
        .map_err(|e| io::Error::other(format!("Failed to get current serial: {e}")))
}
