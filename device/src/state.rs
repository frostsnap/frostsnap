use frostsnap_core::schnorr_fun::fun::Scalar;
use frostsnap_core::FrostsnapKey;

#[derive(bincode::Encode, bincode::Decode, Debug, Clone)]
// TODO: Make FrostPhase an option of a key
pub struct FrostState {
    #[bincode(with_serde)]
    pub secret: Scalar,
    pub phase: FrostPhase,
}

#[derive(bincode::Encode, bincode::Decode, Debug, Clone)]
pub enum FrostPhase {
    #[bincode(with_serde)]
    PreKeygen,
    Key {
        #[bincode(with_serde)]
        frost_signer: frostsnap_core::FrostSigner,
    },
}
