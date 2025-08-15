use rusqlite::{params, Connection, Result as SqlResult};
use std::time::SystemTime;

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn new() -> SqlResult<Self> {
        let conn = Connection::open("factory.db")?;

        // Create devices table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS devices (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                serial_number TEXT UNIQUE NOT NULL,
                case_color TEXT NOT NULL,
                operator TEXT NOT NULL,
                flashed_at INTEGER NOT NULL,
                status TEXT CHECK(status IN ('flashed', 'failed')) DEFAULT 'flashed',
                failure_reason TEXT
            )",
            [],
        )?;

        // Create serial counter table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS serial_counter (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                current_serial INTEGER NOT NULL DEFAULT 1000000
            )",
            [],
        )?;

        // Initialize serial counter if it doesn't exist
        conn.execute(
            "INSERT OR IGNORE INTO serial_counter (id, current_serial) VALUES (1, 1000000)",
            [],
        )?;

        Ok(Database { conn })
    }

    pub fn insert_device(
        &self,
        serial_number: &str,
        color: &str,
        operator: &str,
        status: &str,
        failure_reason: Option<&str>,
    ) -> SqlResult<()> {
        let timestamp = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        self.conn.execute(
            "INSERT INTO devices (serial_number, case_color, operator, flashed_at, status, failure_reason) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![serial_number, color, operator, timestamp, status, failure_reason],
        )?;
        Ok(())
    }

    pub fn get_device_count(&self, color: Option<&str>) -> SqlResult<u32> {
        match color {
            Some(c) => {
                let count: u32 = self.conn.query_row(
                    "SELECT COUNT(*) FROM devices WHERE case_color = ?1 AND status = 'flashed'",
                    params![c],
                    |row| row.get(0),
                )?;
                Ok(count)
            }
            None => {
                let count: u32 = self.conn.query_row(
                    "SELECT COUNT(*) FROM devices WHERE status = 'flashed'",
                    [],
                    |row| row.get(0),
                )?;
                Ok(count)
            }
        }
    }

    pub fn get_color_counts(&self) -> SqlResult<Vec<(String, u32)>> {
        let mut stmt = self.conn.prepare(
            "SELECT case_color, COUNT(*) FROM devices WHERE status = 'flashed' GROUP BY case_color ORDER BY case_color"
        )?;

        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Get the next serial number atomically
    pub fn get_next_serial(&self) -> SqlResult<u32> {
        // Use a transaction to ensure atomicity
        let tx = self.conn.unchecked_transaction()?;

        // Get current serial number
        let current: u32 = tx.query_row(
            "SELECT current_serial FROM serial_counter WHERE id = 1",
            [],
            |row| row.get(0),
        )?;

        let next = current + 1;

        // Update to next serial number
        tx.execute(
            "UPDATE serial_counter SET current_serial = ?1 WHERE id = 1",
            params![next],
        )?;

        tx.commit()?;
        Ok(next)
    }

    /// Get current serial number without incrementing
    pub fn get_current_serial(&self) -> SqlResult<u32> {
        let current: u32 = self.conn.query_row(
            "SELECT current_serial FROM serial_counter WHERE id = 1",
            [],
            |row| row.get(0),
        )?;
        Ok(current)
    }

    /// Set the serial counter to a specific value (for initialization/migration)
    pub fn set_serial_counter(&self, value: u32) -> SqlResult<()> {
        self.conn.execute(
            "UPDATE serial_counter SET current_serial = ?1 WHERE id = 1",
            params![value],
        )?;
        Ok(())
    }
}
