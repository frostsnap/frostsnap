use bitcoin::{address::NetworkChecked, Address};
use tweak::BitcoinBip32Path;

use super::*;
/// Messages to the user often to ask them to confirm things. Often confirmations contain what we
/// call a "phase" which is both the data that describes the action and what will be passed back
/// into the core module once the action is confirmed to make progress.
#[derive(Clone, Debug)]
pub enum DeviceToUserMessage {
    FinalizeKeyGen,
    CheckKeyGen {
        phase: Box<KeyGenPhase2>,
    },
    SignatureRequest {
        phase: Box<SignPhase1>,
    },
    VerifyAddress {
        address: Address<NetworkChecked>,
        bip32_path: BitcoinBip32Path,
    },
    Restoration(restoration::ToUserRestoration),
}
