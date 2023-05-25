use bitcoin::{
    schnorr::TweakedPublicKey,
    secp256k1::PublicKey,
    util::{
        bip32::{ChainCode, ExtendedPubKey, Fingerprint},
        taproot::TapTweakHash,
    },
    util::{
        bip32::{ChildNumber, DerivationPath},
        taproot::TapBranchHash,
    },
    Address, Network, XOnlyPublicKey,
};
use schnorr_fun::{
    frost::FrostKey,
    fun::{
        marker::{EvenY, Normal, Public, Zero},
        Scalar,
    },
};

pub fn get_xpub(frost_key: &FrostKey<Normal>, chaincode: [u8; 32]) -> ExtendedPubKey {
    ExtendedPubKey {
        network: bitcoin::Network::Bitcoin,
        depth: 0,
        parent_fingerprint: Fingerprint::default(),
        child_number: ChildNumber::Normal { index: 0 },
        public_key: PublicKey::from_slice(&frost_key.public_key().to_bytes()).unwrap(),
        chain_code: ChainCode::from(chaincode.as_slice()),
    }
}

pub fn derive_frost_address<C: bitcoin::secp256k1::Verification>(
    // secp: &Secp256k1<C>,
    frost_key: &FrostKey<Normal>,
    chaincode: [u8; 32],
    derivation_path: DerivationPath,
    merkle_root: Option<TapBranchHash>,
) -> (Address, FrostKey<EvenY>) {
    let xpub = get_xpub(frost_key, chaincode);
    let mut frost_key = frost_key.clone();

    let derived_xpub = xpub.clone();
    for child_number in derivation_path.into_iter() {
        // Derive child public keys by adding tweak to FROST key, and compare to rust bitcoin ckd_pub
        let (bip32_tweak, _chaincode) = derived_xpub.ckd_pub_tweak(*child_number).unwrap();
        let bip32_tweak: Scalar<Public, Zero> =
            Scalar::from_bytes(bip32_tweak.secret_bytes()).unwrap();
        frost_key = frost_key.tweak(bip32_tweak).unwrap();
    }

    let frost_xonly = frost_key.into_xonly_key();
    let tweak = TapTweakHash::from_key_and_tweak(
        XOnlyPublicKey::from_slice(&frost_xonly.public_key().to_xonly_bytes()).unwrap(),
        merkle_root,
    )
    .to_scalar();
    let tweaked_frost_xonly = frost_xonly
        .tweak(Scalar::<Public, Zero>::from_bytes(tweak.to_be_bytes()).unwrap())
        .unwrap();

    let address = Address::p2tr_tweaked(
        TweakedPublicKey::dangerous_assume_tweaked(
            XOnlyPublicKey::from_slice(&tweaked_frost_xonly.public_key().to_xonly_bytes()).unwrap(),
        ),
        Network::Bitcoin,
    );

    (address, tweaked_frost_xonly)
}
