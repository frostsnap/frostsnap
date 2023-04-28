use frostsnap_core::schnorr_fun::fun::Scalar;
use frostsnap_core::FrostsnapKey;

#[derive(bincode::Encode, bincode::Decode, Debug, Clone)]
pub struct DeviceState {
    #[bincode(with_serde)]
    pub secret: Scalar,
    pub phase: DevicePhase,
}

#[derive(bincode::Encode, bincode::Decode, Debug, Clone)]
pub enum DevicePhase {
    #[bincode(with_serde)]
    PreKeygen,
    Key {
        #[bincode(with_serde)]
        frost_signer: frostsnap_core::FrostSigner,
    },
}
