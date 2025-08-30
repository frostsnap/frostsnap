use rusqlite::{params, Connection, Result as SqlResult};
use std::time::SystemTime;

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn new() -> SqlResult<Self> {
        let conn = Connection::open("factory.db")?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS devices (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                operator TEXT NOT NULL,
                factory_completed_at INTEGER NOT NULL,
                case_color TEXT NOT NULL,
                serial_number TEXT UNIQUE NOT NULL,
                genuine_verified BOOLEAN DEFAULT FALSE,
                status TEXT CHECK(status IN ('factory_complete', 'genuine_verified', 'failed')),
                failure_reason TEXT
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS serial_counter (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                current_serial INTEGER NOT NULL DEFAULT 1000000
            )",
            [],
        )?;

        conn.execute(
            "INSERT OR IGNORE INTO serial_counter (id, current_serial) VALUES (1, 1000000)",
            [],
        )?;

        Ok(Database { conn })
    }

    /// Mark a device as factory complete - should only happen once per serial
    pub fn mark_factory_complete(
        &self,
        serial_number: &str,
        color: &str,
        operator: &str,
    ) -> SqlResult<()> {
        let timestamp = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        self.conn.execute(
            "INSERT INTO devices (serial_number, case_color, operator, factory_completed_at, status) 
             VALUES (?1, ?2, ?3, ?4, 'factory_complete')",
            params![serial_number, color, operator, timestamp],
        )?;

        Ok(())
    }

    /// Mark a device as genuine verified - can happen multiple times
    pub fn mark_genuine_verified(&self, serial_number: &str) -> SqlResult<()> {
        self.conn.execute(
            "UPDATE devices SET genuine_verified = TRUE, status = 'genuine_verified' 
             WHERE serial_number = ?1",
            params![serial_number],
        )?;

        Ok(())
    }

    /// Mark a device as failed
    pub fn mark_failed(
        &self,
        serial_number: &str,
        color: &str,
        operator: &str,
        reason: &str,
    ) -> SqlResult<()> {
        let timestamp = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Try insert first, then update if it fails due to constraint
        match self.conn.execute(
            "INSERT INTO devices (serial_number, case_color, operator, factory_completed_at, status, failure_reason) 
             VALUES (?1, ?2, ?3, ?4, 'failed', ?5)",
            params![serial_number, color, operator, timestamp, reason],
        ) {
            Ok(_) => Ok(()),
            Err(_) => {
                // Device exists, update it
                self.conn.execute(
                    "UPDATE devices SET status = 'failed', failure_reason = ?2 WHERE serial_number = ?1",
                    params![serial_number, reason],
                )?;
                Ok(())
            }
        }
    }

    // Keep your existing serial methods unchanged
    pub fn get_next_serial(&self) -> SqlResult<u32> {
        let tx = self.conn.unchecked_transaction()?;
        let current: u32 = tx.query_row(
            "SELECT current_serial FROM serial_counter WHERE id = 1",
            [],
            |row| row.get(0),
        )?;
        let next = current + 1;
        tx.execute(
            "UPDATE serial_counter SET current_serial = ?1 WHERE id = 1",
            params![next],
        )?;
        tx.commit()?;
        Ok(next)
    }

    pub fn get_current_serial(&self) -> SqlResult<u32> {
        let current: u32 = self.conn.query_row(
            "SELECT current_serial FROM serial_counter WHERE id = 1",
            [],
            |row| row.get(0),
        )?;
        Ok(current)
    }

    pub fn set_serial_counter(&self, value: u32) -> SqlResult<()> {
        self.conn.execute(
            "UPDATE serial_counter SET current_serial = ?1 WHERE id = 1",
            params![value],
        )?;
        Ok(())
    }
}
