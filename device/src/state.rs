#[derive(bincode::Encode, bincode::Decode, Debug, Clone)]

pub struct FrostState {
    #[bincode(with_serde)]
    pub signer: frostsnap_core::FrostSigner,
}
