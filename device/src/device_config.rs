// Set to false if we are debugging on UART0
pub const SILENCE_PRINTS: bool = false;

#[macro_export]
macro_rules! println {
    ($($arg:tt)*) => {
        {
            if !$crate::device_config::SILENCE_PRINTS {
                esp_println::println!($($arg)*);
            }
        }
    }
}
