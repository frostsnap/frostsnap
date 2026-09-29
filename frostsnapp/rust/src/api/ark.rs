//! Boarding into an Ark from a Frostsnap wallet. **Beta, signet only.**
//!
//! A board is an ordinary on-chain payment to a script shared between the user and the Ark
//! server. The Frostsnap wallet builds and signs that payment exactly as it signs any other
//! send; what is new is that, before it is broadcast, the unsigned transaction goes to the Ark
//! server, which cosigns the VTXO's exit transaction against the funding txid. Only once that
//! cosignature is verified is the funding transaction broadcast, so the coins never sit in a
//! board output without a way back out.
//!
//! The devices sign only the funding transaction's taproot key spends. The VTXO key itself is
//! held in software by [bark](https://gitlab.com/ark-bitcoin/bark), in the app's data directory,
//! under a mnemonic of its own. Holding it on the devices needs MuSig2 support in the firmware
//! and is not attempted here.

use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

use anyhow::{anyhow, bail, ensure, Context, Result};
use bark::{Config, OpenWalletArgs, Wallet, WalletSeed};
use bitcoin::{Network as BitcoinNetwork, Psbt, ScriptBuf};
use flutter_rust_bridge::frb;
use frostsnap_core::message::EncodedSignature;
use serde::{Deserialize, Serialize};

use super::signing::UnsignedTx;

const MNEMONIC_FILE: &str = "mnemonic";
const PENDING_BOARD_FILE: &str = "pending-board.json";

/// The Ark server and esplora instance for each network the beta supports.
fn endpoints(network: BitcoinNetwork) -> Result<(&'static str, &'static str)> {
    match network {
        BitcoinNetwork::Signet => Ok((
            "https://ark.signet.2nd.dev",
            "https://esplora.signet.2nd.dev",
        )),
        other => bail!("Ark boarding is a signet-only beta; {other} is not supported yet"),
    }
}

/// The Ark server boards on `network` go to, shown before anything connects.
#[frb(sync)]
pub fn ark_server_address(network: BitcoinNetwork) -> Option<String> {
    endpoints(network).ok().map(|(server, _)| server.to_owned())
}

/// bark needs a tokio runtime; frb's thread pool is not one. Every call below blocks one of
/// frb's worker threads on this runtime, which is how the other blocking calls in this crate
/// already behave.
fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .thread_name("ark")
            .build()
            .expect("tokio runtime")
    })
}

/// The board address handed out and not yet boarded, so the key and expiry that address
/// commits to survive the app being closed between building the send and signing it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct PendingBoard {
    key_index: u32,
    expiry_height: u32,
    script_pubkey: ScriptBuf,
}

#[derive(Clone, Debug)]
#[frb(type_64bit_int)]
pub struct ArkBalance {
    pub spendable_sat: u64,
    pub pending_board_sat: u64,
}

#[derive(Clone, Debug)]
pub struct ArkBoardAddress {
    pub address: bitcoin::Address,
    /// Block height at which the VTXO this board creates expires.
    pub expiry_height: u32,
}

#[derive(Clone, Debug)]
#[frb(type_64bit_int)]
pub struct ArkBoardReceipt {
    pub funding_txid: String,
    pub vtxo_id: String,
    pub amount_sat: u64,
}

#[frb(opaque)]
pub struct ArkWallet {
    wallet: Wallet,
    datadir: PathBuf,
    network: BitcoinNetwork,
}

impl ArkWallet {
    /// Opens the Ark wallet for `network`, creating it and its mnemonic on first use.
    pub fn open(app_dir: String, network: BitcoinNetwork) -> Result<ArkWallet> {
        let (server, esplora) = endpoints(network)?;
        let datadir = Path::new(&app_dir).join("ark").join(network.to_string());
        let mnemonic = load_or_create_mnemonic(&datadir)?;

        let mut config = Config::network_default(network);
        config.server_address = server.to_owned();
        config.esplora_address = Some(esplora.to_owned());
        // Servers require `<name>/<version>`.
        config.user_agent =
            Some(concat!("frostsnap-ark-beta/", env!("CARGO_PKG_VERSION")).to_owned());

        let wallet = runtime().block_on(Wallet::open(
            network,
            WalletSeed::new_from_mnemonic(network, &mnemonic),
            config,
            OpenWalletArgs {
                datadir: Some(datadir.clone()),
                create_if_not_exists: true,
                ..Default::default()
            },
        ))?;

        Ok(ArkWallet {
            wallet,
            datadir,
            network,
        })
    }

    #[frb(sync)]
    pub fn server_address(&self) -> String {
        endpoints(self.network)
            .map(|(server, _)| server.to_owned())
            .unwrap_or_default()
    }

    /// The Ark server's public key, so the user can see which Ark they are boarding into.
    pub fn server_pubkey(&self) -> Result<String> {
        runtime().block_on(async {
            let info = self
                .wallet
                .ark_info()
                .await?
                .context("Ark server unreachable")?;
            Ok(info.server_pubkey.to_string())
        })
    }

    /// Picks up confirmed boards and anything else that moved since the last sync.
    pub fn sync(&self) -> Result<()> {
        runtime().block_on(async {
            self.wallet.sync().await;
            self.wallet.sync_pending_boards().await
        })
    }

    pub fn balance(&self) -> Result<ArkBalance> {
        let balance = runtime().block_on(self.wallet.balance())?;
        Ok(ArkBalance {
            spendable_sat: balance.spendable.to_sat(),
            pending_board_sat: balance.pending_board.to_sat(),
        })
    }

    /// The smallest board the server accepts.
    #[frb(type_64bit_int)]
    pub fn min_board_sat(&self) -> Result<u64> {
        runtime().block_on(async {
            let info = self
                .wallet
                .ark_info()
                .await?
                .context("Ark server unreachable")?;
            Ok(info.min_board_amount.to_sat())
        })
    }

    /// The address a board pays. The same one is returned until it is boarded, so a send built
    /// against it and signed later still matches.
    pub fn board_address(&self) -> Result<ArkBoardAddress> {
        if let Some(pending) = self.read_pending()? {
            let address = bitcoin::Address::from_script(&pending.script_pubkey, self.network)?;
            return Ok(ArkBoardAddress {
                address,
                expiry_height: pending.expiry_height,
            });
        }
        let (address, expiry_height) = runtime().block_on(async {
            let (keypair, key_index) = self.wallet.derive_store_next_keypair().await?;
            let (address, expiry_height) = self.wallet.board_funding_address(&keypair).await?;
            self.write_pending(&PendingBoard {
                key_index,
                expiry_height,
                script_pubkey: address.script_pubkey(),
            })?;
            anyhow::Ok((address, expiry_height))
        })?;
        Ok(ArkBoardAddress {
            address,
            expiry_height,
        })
    }

    /// Whether this transaction pays the pending board address, and so must be cosigned by the
    /// Ark server before it is broadcast.
    #[frb(sync)]
    pub fn is_board(&self, unsigned_tx: &UnsignedTx) -> bool {
        let Ok(Some(pending)) = self.read_pending() else {
            return false;
        };
        unsigned_tx
            .to_unsigned_psbt()
            .map(|psbt| pays_script(&psbt, &pending.script_pubkey))
            .unwrap_or(false)
    }

    /// Has the Ark server cosign the board this transaction funds.
    ///
    /// Must be called **before** the transaction is broadcast. The PSBT handed to bark carries
    /// no witnesses, so bark does not broadcast it; the caller broadcasts the signed transaction
    /// the usual way once this returns, and bark registers the VTXO when it confirms. The
    /// signatures are checked to produce a transaction first, so a board is never cosigned for
    /// a transaction the wallet could not then send.
    pub fn board(
        &self,
        unsigned_tx: &UnsignedTx,
        signatures: Vec<EncodedSignature>,
    ) -> Result<ArkBoardReceipt> {
        let pending = self
            .read_pending()?
            .ok_or_else(|| anyhow!("no board address has been handed out"))?;
        let psbt = unsigned_tx.to_unsigned_psbt()?;
        ensure!(
            pays_script(&psbt, &pending.script_pubkey),
            "this transaction does not pay the board address"
        );
        let signed = unsigned_tx.with_signatures(signatures)?;
        ensure!(
            signed.compute_txid() == psbt.unsigned_tx.compute_txid(),
            "signing changed the txid; the board would commit to a transaction that never confirms"
        );

        let board = runtime().block_on(async {
            let keypair = self.wallet.peek_keypair(pending.key_index).await?;
            self.wallet
                .board_psbt(psbt, keypair, pending.expiry_height)
                .await
        })?;
        self.clear_pending()?;

        Ok(ArkBoardReceipt {
            funding_txid: board.funding_tx.compute_txid().to_string(),
            vtxo_id: board
                .vtxos
                .first()
                .map(|id| id.to_string())
                .unwrap_or_default(),
            amount_sat: board.amount.to_sat(),
        })
    }

    fn read_pending(&self) -> Result<Option<PendingBoard>> {
        let path = self.datadir.join(PENDING_BOARD_FILE);
        match std::fs::read(&path) {
            Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }

    fn write_pending(&self, pending: &PendingBoard) -> Result<()> {
        std::fs::write(
            self.datadir.join(PENDING_BOARD_FILE),
            serde_json::to_vec_pretty(pending)?,
        )?;
        Ok(())
    }

    fn clear_pending(&self) -> Result<()> {
        match std::fs::remove_file(self.datadir.join(PENDING_BOARD_FILE)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    }
}

/// bark refuses a PSBT paying the board script twice, so exactly one match is what counts.
fn pays_script(psbt: &Psbt, script_pubkey: &ScriptBuf) -> bool {
    psbt.unsigned_tx
        .output
        .iter()
        .filter(|o| &o.script_pubkey == script_pubkey)
        .count()
        == 1
}

fn load_or_create_mnemonic(datadir: &Path) -> Result<bark::bip39::Mnemonic> {
    std::fs::create_dir_all(datadir)?;
    let path = datadir.join(MNEMONIC_FILE);
    match std::fs::read_to_string(&path) {
        Ok(words) => Ok(words.trim().parse()?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let mnemonic = bark::bip39::Mnemonic::generate(12)?;
            std::fs::write(&path, mnemonic.to_string())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
            }
            Ok(mnemonic)
        }
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::{absolute::LockTime, transaction::Version, Amount, Transaction, TxOut};

    fn psbt_paying(scripts: &[ScriptBuf]) -> Psbt {
        let tx = Transaction {
            version: Version::TWO,
            lock_time: LockTime::ZERO,
            input: vec![],
            output: scripts
                .iter()
                .map(|s| TxOut {
                    value: Amount::from_sat(20_000),
                    script_pubkey: s.clone(),
                })
                .collect(),
        };
        Psbt::from_unsigned_tx(tx).unwrap()
    }

    #[test]
    fn board_output_must_appear_exactly_once() {
        let board = ScriptBuf::from_hex("5120").unwrap();
        let change = ScriptBuf::from_hex("0014").unwrap();
        assert!(pays_script(
            &psbt_paying(&[change.clone(), board.clone()]),
            &board
        ));
        assert!(!pays_script(&psbt_paying(&[change.clone()]), &board));
        assert!(!pays_script(
            &psbt_paying(&[board.clone(), board.clone()]),
            &board
        ));
    }

    #[test]
    fn only_signet_is_supported() {
        assert!(endpoints(BitcoinNetwork::Signet).is_ok());
        assert!(endpoints(BitcoinNetwork::Bitcoin).is_err());
        assert!(endpoints(BitcoinNetwork::Testnet).is_err());
    }

    #[test]
    fn mnemonic_is_created_once_and_reused() {
        let dir = std::env::temp_dir().join(format!("frostsnap-ark-test-{}", std::process::id()));
        let first = load_or_create_mnemonic(&dir).unwrap();
        let second = load_or_create_mnemonic(&dir).unwrap();
        assert_eq!(first, second);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn pending_board_round_trips() {
        let pending = PendingBoard {
            key_index: 3,
            expiry_height: 324_200,
            script_pubkey: ScriptBuf::from_hex("5120").unwrap(),
        };
        let json = serde_json::to_vec(&pending).unwrap();
        assert_eq!(
            serde_json::from_slice::<PendingBoard>(&json).unwrap(),
            pending
        );
    }
}
