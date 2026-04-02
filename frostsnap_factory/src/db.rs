use frostsnap_comms::genuine_certificate::CaseColor;
use frostsnap_comms::Sha256Digest;
use std::cell::Cell;

pub struct DeviceRecord<'a> {
    pub serial_number: &'a str,
    pub color: CaseColor,
    pub operator: &'a str,
    pub board_revision: &'a str,
    pub batch_note: Option<&'a str>,
}

pub trait FactoryDatabase {
    fn get_next_serial(&self) -> Result<u32, Box<dyn std::error::Error>>;
    fn mark_factory_complete(
        &self,
        record: &DeviceRecord,
    ) -> Result<(), Box<dyn std::error::Error>>;
    fn mark_genuine_verified(
        &self,
        serial_number: &str,
        firmware_digest: Sha256Digest,
    ) -> Result<(), Box<dyn std::error::Error>>;
    fn mark_failed(
        &self,
        record: &DeviceRecord,
        reason: &str,
    ) -> Result<(), Box<dyn std::error::Error>>;
}

/// No-op database for single-device dev provisioning
pub struct DevDatabase {
    serial: Cell<u32>,
}

impl DevDatabase {
    pub fn new(starting_serial: u32) -> Self {
        Self {
            serial: Cell::new(starting_serial),
        }
    }
}

impl FactoryDatabase for DevDatabase {
    fn get_next_serial(&self) -> Result<u32, Box<dyn std::error::Error>> {
        let s = self.serial.get();
        self.serial.set(s + 1);
        Ok(s)
    }

    fn mark_factory_complete(&self, _: &DeviceRecord) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }

    fn mark_genuine_verified(
        &self,
        _: &str,
        _: Sha256Digest,
    ) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }

    fn mark_failed(&self, _: &DeviceRecord, _: &str) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }
}

/// MySQL-backed database for production factory batches
pub struct MysqlDatabase {
    pool: mysql::Pool,
}

impl MysqlDatabase {
    pub fn new(db_connection: String) -> Result<Self, Box<dyn std::error::Error>> {
        use mysql::prelude::*;

        let opts = mysql::Opts::from_url(&db_connection)?;
        let pool = mysql::Pool::new(opts)?;
        let mut conn = pool.get_conn()?;

        conn.query_drop(
            "CREATE TABLE IF NOT EXISTS devices (
                id INT AUTO_INCREMENT PRIMARY KEY,
                operator VARCHAR(255) NOT NULL,
                factory_completed_at BIGINT NOT NULL,
                case_color VARCHAR(50) NOT NULL,
                serial_number VARCHAR(255) UNIQUE NOT NULL,
                board_revision VARCHAR(50) NOT NULL,
                firmware_hash VARCHAR(64),
                genuine_verified BOOLEAN DEFAULT FALSE,
                status ENUM('factory_complete', 'genuine_verified', 'failed'),
                failure_reason TEXT,
                batch_note TEXT
            )",
        )?;

        conn.query_drop(
            "CREATE TABLE IF NOT EXISTS serial_counter (
                id INT PRIMARY KEY,
                current_serial INT NOT NULL DEFAULT 00001000,
                CHECK (id = 1)
            )",
        )?;

        conn.query_drop(
            "INSERT IGNORE INTO serial_counter (id, current_serial) VALUES (1, 00001000)",
        )?;

        Ok(MysqlDatabase { pool })
    }

    fn get_conn(&self) -> mysql::Result<mysql::PooledConn> {
        self.pool.get_conn()
    }
}

impl FactoryDatabase for MysqlDatabase {
    fn get_next_serial(&self) -> Result<u32, Box<dyn std::error::Error>> {
        use mysql::prelude::*;

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

    fn mark_factory_complete(
        &self,
        record: &DeviceRecord,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use mysql::prelude::*;
        use std::time::SystemTime;

        let timestamp = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let mut conn = self.get_conn()?;
        conn.exec_drop(
            "INSERT INTO devices (serial_number, case_color, operator, factory_completed_at, status, board_revision, batch_note)
             VALUES (?, ?, ?, ?, 'factory_complete', ?, ?)",
            (
                record.serial_number,
                record.color.to_string(),
                record.operator,
                timestamp,
                record.board_revision,
                record.batch_note,
            ),
        )?;
        Ok(())
    }

    fn mark_genuine_verified(
        &self,
        serial_number: &str,
        firmware_digest: Sha256Digest,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use mysql::prelude::*;

        let mut conn = self.get_conn()?;

        let exists: Option<u8> = conn.exec_first(
            "SELECT 1 FROM devices WHERE serial_number = ?",
            (serial_number,),
        )?;

        if exists.is_none() {
            return Err(format!("Serial number {} not found in database", serial_number).into());
        }

        conn.exec_drop(
            "UPDATE devices SET firmware_hash = ?, genuine_verified = TRUE, status = 'genuine_verified'
             WHERE serial_number = ?",
            (firmware_digest.to_string(), serial_number),
        )?;

        Ok(())
    }

    fn mark_failed(
        &self,
        record: &DeviceRecord,
        reason: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use mysql::prelude::*;
        use std::time::SystemTime;

        let timestamp = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let mut conn = self.get_conn()?;

        match conn.exec_drop(
            "INSERT INTO devices (serial_number, case_color, operator, factory_completed_at, status, board_revision, failure_reason, batch_note)
             VALUES (?, ?, ?, ?, 'failed', ?, ?, ?)",
            (
                record.serial_number,
                record.color.to_string(),
                record.operator,
                timestamp,
                record.board_revision,
                reason,
                record.batch_note,
            ),
        ) {
            Ok(_) => Ok(()),
            Err(_) => {
                conn.exec_drop(
                    "UPDATE devices SET status = 'failed', failure_reason = ? WHERE serial_number = ?",
                    (reason, record.serial_number),
                )?;
                Ok(())
            }
        }
    }
}
