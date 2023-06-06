use bdk_chain::bitcoin;
use bdk_chain::miniscript::{
    descriptor::Tr,
    descriptor::{SinglePub, SinglePubKey},
    Descriptor, DescriptorPublicKey,
};

use frostsnap_core::CoordinatorFrostKey;

use bdk_chain::{
    indexed_tx_graph::IndexedAdditions,
    indexed_tx_graph::IndexedTxGraph,
    local_chain::{self, LocalChain},
    ConfirmationTimeAnchor, SpkTxOutIndex,
};
use frostsnap_core::schnorr_fun::{frost::FrostKey, fun::marker::Normal};

#[derive(Debug)]
pub struct Wallet {
    pub coordinator_frost_key: frostsnap_core::CoordinatorFrostKey,
    pub chain: LocalChain,
    pub graph: IndexedTxGraph<ConfirmationTimeAnchor, SpkTxOutIndex<()>>,
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize, Default)]
pub struct ChangeSet {
    pub chain_changeset: local_chain::ChangeSet,
    pub indexed_additions: IndexedAdditions<ConfirmationTimeAnchor, ()>,
}

impl bdk_chain::Append for ChangeSet {
    fn append(&mut self, other: Self) {
        bdk_chain::Append::append(&mut self.chain_changeset, other.chain_changeset);
        bdk_chain::Append::append(&mut self.indexed_additions, other.indexed_additions);
    }

    fn is_empty(&self) -> bool {
        self.chain_changeset.is_empty() && self.indexed_additions.is_empty()
    }
}

pub fn get_descriptor(key: &FrostKey<Normal>) -> Descriptor<DescriptorPublicKey> {
    let key: bitcoin::secp256k1::PublicKey =
        bitcoin::secp256k1::PublicKey::from_slice(key.public_key().to_bytes().as_ref()).unwrap();
    let key = bitcoin::PublicKey {
        compressed: true,
        inner: key,
    };
    let key = DescriptorPublicKey::Single(SinglePub {
        origin: None,
        key: SinglePubKey::FullKey(key),
    });
    let tr = Tr::new(key, None).expect("infallible since it's None");
    let descriptor = Descriptor::Tr(tr);
    descriptor
}

impl Wallet {
    pub fn new(coordinator_frost_key: CoordinatorFrostKey, changeset: ChangeSet) -> Self {
        let descriptor = get_descriptor(coordinator_frost_key.frost_key());
        let mut chain: LocalChain = Default::default();
        chain.apply_changeset(changeset.chain_changeset);
        let index = SpkTxOutIndex::default();
        let mut graph = IndexedTxGraph::new(index);
        graph
            .index
            .insert_spk((), descriptor.at_derivation_index(0).script_pubkey());
        graph.apply_additions(changeset.indexed_additions);
        Self {
            coordinator_frost_key,
            chain,
            graph,
        }
    }

    pub fn next_address(&mut self, network: bitcoin::Network) -> bitcoin::Address {
        let spk = self.graph.index.spk_at_index(&()).unwrap();
        bitcoin::Address::from_script(spk, network).unwrap()
    }

    pub fn next_change_script_pubkey(&mut self) -> bitcoin::Script {
        let spk = self.graph.index.spk_at_index(&()).unwrap();
        spk.clone()
    }
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum Keychain {
    Internal,
    External,
}
