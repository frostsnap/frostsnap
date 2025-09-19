use flutter_rust_bridge::frb;
pub use frostsnap_coordinator::frostsnap_comms::DeviceName;

#[frb(sync, type_64bit_int)]
pub fn key_name_max_length() -> usize {
    frostsnap_coordinator::frostsnap_comms::KEY_NAME_MAX_LENGTH
}

#[frb(external)]
impl DeviceName {
    #[frb(sync)]
    pub fn to_string(&self) -> String {}

    #[frb(sync, type_64bit_int)]
    pub fn max_length() -> usize {}
}
