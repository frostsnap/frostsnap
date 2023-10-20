#[derive(bincode::Encode, bincode::Decode, Debug, Clone)]

pub struct FrostState {
    pub signer: frostsnap_core::FrostSigner,
    pub name: Option<alloc::string::String>,
}
