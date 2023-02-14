//! Interfaces though which the device can communicate -- Currently just contains serial, but
//! would be nice to have serial, i2c, http, all in the one `io/` directory.

mod http;
mod i2c;
mod uart;
mod wifi;

use crate::message::FrostMessage;

// Currently devices communication through rounds of FrostMessages
pub trait DeviceIO {
    fn read_messages(&mut self) -> Vec<FrostMessage>;
    fn write_messages(&mut self, messages: Vec<FrostMessage>);
}
