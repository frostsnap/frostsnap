use std::fs;
use std::io::{self};

const COUNTER_FILE: &str = "SERIAL_NUMBER_COUNTER";

fn read_serial() -> io::Result<u32> {
    let content = fs::read_to_string(COUNTER_FILE)?;
    content
        .trim()
        .parse::<u32>()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn write_serial(value: u32) -> io::Result<()> {
    let temp_file = format!("{COUNTER_FILE}.tmp");

    fs::write(&temp_file, value.to_string())?;

    fs::rename(&temp_file, COUNTER_FILE)?;

    Ok(())
}

pub fn get_next_serial_number() -> io::Result<u32> {
    let current = read_serial()?;
    let next = current + 1;
    write_serial(next)?;
    Ok(next)
}
