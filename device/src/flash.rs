// header (FlashHeader/Header/device_keypair) lifted into frostsnap_embedded; re-exported
pub use frostsnap_embedded::flash_header::*;
// log (Mutation/MutationLog/ShareSlot) lifted into frostsnap_embedded; re-exported
pub use frostsnap_embedded::flash_log::*;
mod genuine_certificate;
pub use genuine_certificate::*;
