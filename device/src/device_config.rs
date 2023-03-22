// Set to false if we are debugging on UART0
pub const DOUBLE_ENDED: bool = true;

macro_rules! println {
    ($($arg:tt)*) => {
        {
            if !DOUBLE_ENDED {
                esp_println::println!($($arg)*);
            }
        }
    }
}
