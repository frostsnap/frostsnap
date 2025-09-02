use mysql::prelude::*;
use mysql::{Pool, PooledConn};
use std::time::SystemTime;

pub struct Database {
    pool: Pool,
}

impl Database {
    pub fn new(db_connection: String) -> Result<Self, Box<dyn std::error::Error>> {
        let opts = mysql::Opts::from_url(&db_connection)?;
        let pool = Pool::new(opts)?;
        let mut conn = pool.get_conn()?;

        conn.query_drop(
            "CREATE TABLE IF NOT EXISTS devices (
                id INT AUTO_INCREMENT PRIMARY KEY,
                operator VARCHAR(255) NOT NULL,
                factory_completed_at BIGINT NOT NULL,
                case_color VARCHAR(50) NOT NULL,
                serial_number VARCHAR(255) UNIQUE NOT NULL,
                genuine_verified BOOLEAN DEFAULT FALSE,
                status ENUM('factory_complete', 'genuine_verified', 'failed'),
                failure_reason TEXT,
                batch_note TEXT
            )",
        )?;

        conn.query_drop(
            "CREATE TABLE IF NOT EXISTS serial_counter (
                id INT PRIMARY KEY,
                current_serial INT NOT NULL DEFAULT 220825000,
                CHECK (id = 1)
            )",
        )?;

        conn.query_drop(
            "INSERT IGNORE INTO serial_counter (id, current_serial) VALUES (1, 220825000)",
        )?;

        Ok(Database { pool })
    }

    fn get_conn(&self) -> mysql::Result<PooledConn> {
        self.pool.get_conn()
    }

    /// Mark a device as factory complete - should only happen once per serial
    pub fn mark_factory_complete(
        &self,
        serial_number: &str,
        color: &str,
        operator: &str,
        batch_note: Option<&str>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let timestamp = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let mut conn = self.get_conn()?;
        conn.exec_drop(
            "INSERT INTO devices (serial_number, case_color, operator, factory_completed_at, status, batch_note) 
             VALUES (?, ?, ?, ?, 'factory_complete', ?)",
            (serial_number, color, operator, timestamp, batch_note),
        )?;

        Ok(())
    }

    /// Mark a device as genuine verified - can happen multiple times
    pub fn mark_genuine_verified(
        &self,
        serial_number: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut conn = self.get_conn()?;
        conn.exec_drop(
            "UPDATE devices SET genuine_verified = TRUE, status = 'genuine_verified' 
             WHERE serial_number = ?",
            (serial_number,),
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
        batch_note: Option<&str>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let timestamp = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let mut conn = self.get_conn()?;

        // Try insert first, then update if it fails due to constraint
        match conn.exec_drop(
            "INSERT INTO devices (serial_number, case_color, operator, factory_completed_at, status, failure_reason, batch_note) 
             VALUES (?, ?, ?, ?, 'failed', ?, ?)",
            (serial_number, color, operator, timestamp, reason, batch_note),
        ) {
            Ok(_) => Ok(()),
            Err(_) => {
                // Device exists, update it
                conn.exec_drop(
                    "UPDATE devices SET status = 'failed', failure_reason = ? WHERE serial_number = ?",
                    (reason, serial_number),
                )?;
                Ok(())
            }
        }
    }

    pub fn get_next_serial(&self) -> Result<u32, Box<dyn std::error::Error>> {
        let mut conn = self.get_conn()?;
        let mut tx = conn.start_transaction(mysql::TxOpts::default())?;

        let current: u32 = tx
            .query_first("SELECT current_serial FROM serial_counter WHERE id = 1")?
            .ok_or("Serial counter row not found!?")?;

        let next = current + 1;

        tx.exec_drop(
            "UPDATE serial_counter SET current_serial = ? WHERE id = 1",
            (next,),
        )?;

        tx.commit()?;
        Ok(next)
    }
}
