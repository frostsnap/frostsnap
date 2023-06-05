use anyhow::Context;
use bdk_chain::bitcoin::secp256k1::schnorr;
use bdk_chain::bitcoin::util::sighash::SighashCache;
use bdk_chain::bitcoin::{
    self, PackedLockTime, SchnorrSig, SchnorrSighashType, Script, Sequence, Transaction, TxIn,
    TxOut, Witness,
};
use bdk_chain::miniscript::{
    descriptor::Tr,
    descriptor::{SinglePub, SinglePubKey},
    Descriptor, DescriptorPublicKey,
};
use bdk_electrum::electrum_client::ElectrumApi;
use bdk_electrum::{electrum_client, v2::ElectrumExt};
use bitcoin::Network;
use frostsnap_core::CoordinatorFrostKey;
use tracing::{event, Level};

use bdk_chain::{
    indexed_tx_graph::IndexedAdditions,
    indexed_tx_graph::IndexedTxGraph,
    local_chain::{self, LocalChain},
    ChainOracle, ConfirmationTimeAnchor, SpkTxOutIndex,
};
use frostsnap_core::schnorr_fun::{frost::FrostKey, fun::marker::Normal};

use crate::db::Db;
use crate::ports::Ports;
use crate::signer::Signer;

#[derive(Debug)]
pub struct Wallet {
    coordinator_frost_key: frostsnap_core::CoordinatorFrostKey,
    chain: LocalChain,
    graph: IndexedTxGraph<ConfirmationTimeAnchor, SpkTxOutIndex<()>>,
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

fn get_descriptor(key: &FrostKey<Normal>) -> Descriptor<DescriptorPublicKey> {
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

#[derive(Debug, Clone, clap::Subcommand)]
pub enum Commands {
    Balance,
    Address,
    Sync,
    Send {
        address: bitcoin::Address,
        value: u64,
        #[clap(default_value = "1.0")]
        feerate: f32,
    },
}

impl Commands {
    pub fn run(
        &self,
        wallet: &mut Wallet,
        db: &mut Db,
        ports: &mut Ports,
        network: Network,
    ) -> anyhow::Result<()> {
        let electrum_url = match network {
            Network::Bitcoin => "ssl://electrum.blockstream.info:50002",
            Network::Testnet => "ssl://electrum.blockstream.info:60002",
            Network::Regtest => "tcp://localhost:60401",
            Network::Signet => "tcp://signet-electrumx.wakiyamap.dev:50001",
        };

        match self {
            Commands::Sync => {
                let config = electrum_client::Config::builder()
                    .validate_domain(matches!(network, Network::Bitcoin))
                    .build();

                let client = electrum_client::Client::from_config(electrum_url, config)?;
                let c = wallet.chain.blocks().clone();

                let spk = wallet.graph.index.spk_at_index(&()).unwrap().clone();

                let response = client
                    .scan_without_keychain(
                        &c,
                        core::iter::once(spk),
                        core::iter::empty(),
                        core::iter::empty(),
                        1,
                    )
                    .context("scanning the blockchain")?;

                let missing_txids = response.missing_full_txs(&wallet.graph.graph());

                let update =
                    response.finalize_as_confirmation_time(&client, None, missing_txids)?;

                let changeset = ChangeSet {
                    chain_changeset: wallet.chain.apply_update(update.chain)?,
                    indexed_additions: wallet.graph.apply_update(update.graph),
                };
                db.save(changeset)?;

                Ok(())
            }
            Commands::Address => {
                println!("{}", wallet.next_address(network));
                Ok(())
            }
            Commands::Balance => {
                fn print_balances<'a>(
                    title_str: &'a str,
                    items: impl IntoIterator<Item = (&'a str, u64)>,
                ) {
                    println!("{}:", title_str);
                    for (name, amount) in items.into_iter() {
                        println!("    {:<10} {:>12} sats", name, amount)
                    }
                }
                let graph = &wallet.graph;
                let chain = &wallet.chain;

                let balance = graph.graph().try_balance(
                    chain,
                    chain.get_chain_tip()?.unwrap_or_default(),
                    graph.index.outpoints().iter().cloned(),
                    |_, _| false,
                )?;

                let confirmed_total = balance.confirmed + balance.immature;
                let unconfirmed_total = balance.untrusted_pending + balance.trusted_pending;

                print_balances(
                    "confirmed",
                    [
                        ("total", confirmed_total),
                        ("spendable", balance.confirmed),
                        ("immature", balance.immature),
                    ],
                );
                print_balances(
                    "unconfirmed",
                    [
                        ("total", unconfirmed_total),
                        ("trusted", balance.trusted_pending),
                        ("untrusted", balance.untrusted_pending),
                    ],
                );

                Ok(())
            }
            Commands::Send {
                address,
                value,
                feerate,
            } => {
                let value = *value;
                use bdk_coin_select::{
                    change_policy, metrics, Candidate, CoinSelector, Drain, FeeRate, Target,
                };
                let chain = &wallet.chain;
                let chain_tip = chain.get_chain_tip()?.unwrap_or_default();
                let outpoints = wallet.graph.index.outpoints().iter().cloned();
                let unspents = wallet
                    .graph
                    .graph()
                    .filter_chain_unspents(chain, chain_tip, outpoints)
                    .collect::<Vec<_>>();
                let candidates = unspents
                    .iter()
                    .map(|(_, utxo)| Candidate::new_tr_keyspend(utxo.txout.value))
                    .collect::<Vec<_>>();

                let mut tx_template = Transaction {
                    input: vec![],
                    output: vec![TxOut {
                        value,
                        script_pubkey: address.script_pubkey(),
                    }],
                    version: 1,
                    lock_time: PackedLockTime::ZERO,
                };

                let mut coin_selector = CoinSelector::new(&candidates, tx_template.weight() as u32);

                let target = Target {
                    feerate: FeeRate::from_sat_per_vb(*feerate),
                    min_fee: 0,
                    value,
                };

                let drain = Drain::new_tr_keyspend();
                let long_term_feerate = FeeRate::from_sat_per_vb(1.0);
                let change_policy = &change_policy::min_waste(drain, long_term_feerate);

                let bnb = coin_selector
                    .branch_and_bound(metrics::Waste {
                        target,
                        long_term_feerate,
                        change_policy,
                    })
                    .take(100_000)
                    .filter_map(|x| x)
                    .next();

                match bnb {
                    Some((solution, waste)) => {
                        event!(
                            Level::DEBUG,
                            waste = waste.to_string(),
                            "found branch and bound solution"
                        );
                        coin_selector = solution;
                    }
                    None => {
                        coin_selector.select_until_target_met(target, drain)?;
                    }
                }

                let change_output = change_policy(&coin_selector, target);

                if change_output.is_some() {
                    tx_template.output.push(TxOut {
                        value: change_output.value,
                        script_pubkey: wallet.next_change_script_pubkey(),
                    });
                }

                let mut prevouts = vec![];

                for (index, _candidate) in coin_selector.selected() {
                    let full_txout = &unspents[index].1;
                    tx_template.input.push(TxIn {
                        previous_output: full_txout.outpoint,
                        script_sig: Script::default(),
                        sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
                        witness: Witness::new(),
                    });

                    prevouts.push(&full_txout.txout);
                }

                let mut messages = vec![];
                let _sighash_tx = tx_template.clone();
                let schnorr_sighashty = SchnorrSighashType::Default;
                for (i, _) in tx_template.input.iter().enumerate() {
                    let mut sighash_cache = SighashCache::new(&_sighash_tx);
                    let sighash = sighash_cache.taproot_key_spend_signature_hash(
                        i,
                        &bitcoin::psbt::Prevouts::All(&prevouts),
                        schnorr_sighashty,
                    )?;
                    messages.push(sighash);
                }

                println!(
                    "inputs {:?}",
                    prevouts.iter().map(|x| x.value).collect::<Vec<_>>()
                );
                println!(
                    "outputs {:?}",
                    tx_template
                        .output
                        .iter()
                        .map(|x| x.value)
                        .collect::<Vec<_>>()
                );
                println!("{:?}", messages);

                let coordinator = frostsnap_core::FrostCoordinator::from_stored_key(
                    wallet.coordinator_frost_key.clone(),
                );
                let mut signer = Signer::new(db, ports, coordinator);

                let request_sign_message =
                    frostsnap_ext::sign_messages::RequestSignMessage::Transaction {
                        tx_template: tx_template.clone(),
                        prevouts: prevouts.into_iter().cloned().collect(),
                    };
                let signatures = signer.sign_message_request(request_sign_message, true)?;

                assert_eq!(signatures.len(), tx_template.input.len());
                for (txin, signature) in tx_template.input.iter_mut().zip(signatures) {
                    let schnorr_sig = SchnorrSig {
                        sig: schnorr::Signature::from_slice(signature.to_bytes().as_ref()).unwrap(),
                        hash_ty: schnorr_sighashty,
                    };
                    let witness = Witness::from_vec(vec![schnorr_sig.to_vec()]);
                    txin.witness = witness;
                }

                let config = electrum_client::Config::builder()
                    .validate_domain(matches!(network, Network::Bitcoin))
                    .build();
                let client = electrum_client::Client::from_config(electrum_url, config)?;
                println!("TXID: {}", tx_template.txid());

                client
                    .transaction_broadcast(&tx_template)
                    .context("broadcasting transaction")?;

                Ok(())
            }
        }
    }
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum Keychain {
    Internal,
    External,
}
