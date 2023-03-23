// Set to false if we are debugging on UART0
pub const SILENCE_PRINTS: bool = true;

macro_rules! println {
    ($($arg:tt)*) => {
        {
            if !SILENCE_PRINTS {
                esp_println::println!($($arg)*);
            }
        }
    }
}
