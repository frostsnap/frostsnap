use crate::{Kind, MasterAppkey};
use frostsnap_macros::Kind as KindDerive;

/// Screen verification messages (for verifying addresses on device screens)
#[derive(Clone, Debug, bincode::Encode, bincode::Decode, KindDerive)]
pub enum ScreenVerify {
    VerifyAddress {
        master_appkey: MasterAppkey,
        derivation_index: u32,
    },
}
