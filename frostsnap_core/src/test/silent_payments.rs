//! Shared harness for silent payment sends over remote signing: a coordinator per participant, the
//! ECDH round of each participant's device, and the helpers that check what the sends produced.
//! Used by the remote signing tests and the silent payment send tests.

use crate::{
    bitcoin_transaction::{LocalSpk, TransactionTemplate},
    coordinator::{
        remote_signing::{combine_signatures, RemoteSignSessionId, SignOffer, SpSubset},
        signing::{ParticipantBinonces, ParticipantSignatureShares},
        CoordinatorToUserMessage, CoordinatorToUserSigningMessage, KeyContext, StartSignError,
    },
    device::KeyPurpose,
    message::{signing::SpEcdhSessionId, GroupSignReq},
    silent_payments::{self, complete_sp_task, output_keys, SilentPaymentRecipient},
    test::{Env, Run, TestDeviceKeyGen, TEST_ENCRYPTION_KEY},
    tweak::{AppTweak, BitcoinBip32Path, Xpub},
    AccessStructureRef, DeviceId, MasterAppkey, SignSessionId, WireSignTask,
};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::String,
    vec::Vec,
};
use bitcoin::{Amount, Network, OutPoint};
use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};
use schnorr_fun::{
    frost::{PairedSecretShare, SecretShare, ShareIndex},
    fun::{g, marker::*, poly, s, Point, Scalar, G},
};

pub const NETWORK: Network = Network::Bitcoin;
pub const SP_ID: RemoteSignSessionId = RemoteSignSessionId([9u8; 32]);

#[derive(Default)]
pub struct RemoteSigningEnv {
    pub shares: BTreeMap<(usize, SignSessionId), Vec<ParticipantSignatureShares>>,
    pub sp_ecdh_got: Vec<(SpEcdhSessionId, DeviceId)>,
}

impl Env for RemoteSigningEnv {
    fn user_react_to_coordinator(
        &mut self,
        run: &mut Run,
        ci: usize,
        message: CoordinatorToUserMessage,
        rng: &mut impl rand_chacha::rand_core::RngCore,
    ) {
        match message {
            CoordinatorToUserMessage::KeyGen {
                keygen_id,
                inner:
                    crate::coordinator::CoordinatorToUserKeyGenMessage::KeyGenAck {
                        all_acks_received: true,
                        ..
                    },
            } => {
                let sends = run.participants[ci]
                    .coordinator
                    .finalize_keygen(keygen_id, TEST_ENCRYPTION_KEY, rng)
                    .unwrap();
                run.extend_from_coordinator(ci, sends);
            }
            CoordinatorToUserMessage::Signing(CoordinatorToUserSigningMessage::GotShare {
                session_id,
                shares,
                ..
            }) => {
                self.shares
                    .entry((ci, session_id))
                    .or_default()
                    .push(shares);
            }
            CoordinatorToUserMessage::Signing(
                CoordinatorToUserSigningMessage::GotSpEcdhShares { session_id, from },
            ) => {
                self.sp_ecdh_got.push((session_id, from));
            }
            _ => {}
        }
    }
}

pub fn scalar(byte: u8) -> Scalar {
    Scalar::<Secret, Zero>::from_bytes_mod_order([byte; 32])
        .non_zero()
        .unwrap()
}

pub fn public(secret: Scalar) -> Point {
    g!(secret * G).normalize().non_zero().unwrap()
}

pub fn recipient(scan: u8, spend: u8) -> (SilentPaymentRecipient, String) {
    let recipient = SilentPaymentRecipient {
        scan_key: public(scalar(scan)),
        spend_key: public(scalar(spend)),
    };
    let address = recipient.address(NETWORK).unwrap();
    (recipient, address)
}

pub fn owned(master_appkey: MasterAppkey, path: BitcoinBip32Path) -> LocalSpk {
    LocalSpk {
        master_appkey,
        bip32_path: path,
    }
}

/// A spend of imaginary coins of `master_appkey` at `inputs` paying `silent` to silent payment
/// recipients, `foreign` to a script nobody here owns, and the rest back to the wallet.
pub fn template(
    master_appkey: MasterAppkey,
    inputs: &[(BitcoinBip32Path, u64)],
    silent: &[(u64, SilentPaymentRecipient, String)],
    change: u64,
) -> TransactionTemplate {
    let mut template = TransactionTemplate::new();
    for (path, sats) in inputs {
        template.push_imaginary_owned_input(owned(master_appkey, *path), Amount::from_sat(*sats));
    }
    for (sats, recipient, address) in silent {
        template.push_silent_payment_output(Amount::from_sat(*sats), *recipient, address.clone());
    }
    template.push_owned_output(
        Amount::from_sat(change),
        owned(master_appkey, BitcoinBip32Path::internal(3)),
    );
    template
}

pub struct SpRemote {
    pub run: Run,
    pub env: RemoteSigningEnv,
    pub rng: ChaCha20Rng,
    pub access_structure_ref: AccessStructureRef,
    pub master_appkey: MasterAppkey,
    pub signing_key: KeyContext,
}

impl SpRemote {
    /// Three coordinators with one device each, a 2-of-3 bitcoin key, and nonces ready.
    pub fn new(seed: u8) -> Self {
        let mut rng = ChaCha20Rng::from_seed([seed; 32]);
        let mut env = RemoteSigningEnv::default();
        let mut run = Run::start_after_remote_keygen(
            &[1, 1, 1],
            2,
            "sp remote key",
            KeyPurpose::Bitcoin(NETWORK),
            &mut env,
            &mut rng,
        );
        run.replenish_all_nonces(2, &mut env, &mut rng);
        let access_structure = run.participants[0]
            .coordinator
            .iter_access_structures()
            .next()
            .unwrap();
        let signing_key = KeyContext {
            app_shared_key: access_structure.app_shared_key(),
            purpose: KeyPurpose::Bitcoin(NETWORK),
        };
        Self {
            access_structure_ref: access_structure.access_structure_ref(),
            master_appkey: access_structure.master_appkey(),
            signing_key,
            run,
            env,
            rng,
        }
    }

    pub fn device(&self, participant: usize) -> crate::DeviceId {
        *self.run.participants[participant]
            .devices
            .keys()
            .next()
            .unwrap()
    }

    /// Participant `i` asks its device for ECDH shares and offers with them.
    pub fn offer_sp(&mut self, i: usize, template: &TransactionTemplate) -> SignOffer {
        let task = WireSignTask::BitcoinTransaction(template.clone());
        let device_id = self.device(i);
        let (ecdh_session, sends) = self.run.participants[i]
            .coordinator
            .start_remote_sp_ecdh(
                self.access_structure_ref,
                task.clone(),
                device_id,
                TEST_ENCRYPTION_KEY,
                &mut self.rng,
            )
            .unwrap();
        self.run.extend_from_coordinator(i, sends);
        self.run
            .run_until_finished(&mut self.env, &mut self.rng)
            .unwrap();
        assert!(
            self.env
                .sp_ecdh_got
                .iter()
                .any(|(sid, _)| *sid == ecdh_session),
            "the device reply must produce GotSpEcdhShares ToUser message"
        );
        self.run.participants[i]
            .coordinator
            .offer_to_sign_sp(
                SP_ID,
                self.access_structure_ref,
                task,
                device_id,
                ecdh_session,
            )
            .unwrap()
    }

    /// What the signers of `subset` take from the offers of `subset`.
    pub fn sp_subset(&self, task: &WireSignTask, offers: &[&SignOffer]) -> SpSubset {
        let binonces = binonces_of(offers);
        let shares: Vec<_> = offers
            .iter()
            .flat_map(|offer| offer.sp_ecdh.iter().copied())
            .collect();
        SpSubset {
            completed_task: complete_sp_task(task, &self.signing_key, &binonces, &shares).unwrap(),
            shares,
        }
    }

    /// Has the participants in `signers` sign with `binonces` and `sp`.
    pub fn sign(
        &mut self,
        signers: &[usize],
        binonces: &[ParticipantBinonces],
        sp: Option<&SpSubset>,
    ) -> Result<(), StartSignError> {
        for &i in signers {
            let device_id = self.device(i);
            let sends = self.run.participants[i]
                .coordinator
                .sign_with_nonce_reservation(SP_ID, device_id, binonces, sp, TEST_ENCRYPTION_KEY)?;
            self.run.extend_from_coordinator(i, sends);
        }
        self.run
            .run_until_finished(&mut self.env, &mut self.rng)
            .unwrap();
        Ok(())
    }

    /// The signature shares the `signers` produced for `task`, combined into signatures.
    pub fn combine(
        &self,
        task: &WireSignTask,
        signers: &[usize],
        binonces: &[ParticipantBinonces],
    ) -> Vec<schnorr_fun::Signature> {
        let session_id = GroupSignReq::from_binonces(
            task.clone(),
            self.access_structure_ref.access_structure_id,
            binonces,
        )
        .session_id();
        let shares: Vec<_> = signers
            .iter()
            .map(|i| self.env.shares[&(*i, session_id)][0].clone())
            .collect();
        let refs: Vec<_> = shares.iter().collect();
        combine_signatures(task.clone(), &self.signing_key, binonces, &refs)
            .unwrap()
            .into_iter()
            .map(|sig| sig.into_decoded().unwrap())
            .collect()
    }
}

pub fn binonces_of(offers: &[&SignOffer]) -> Vec<ParticipantBinonces> {
    offers
        .iter()
        .map(|offer| offer.participant_binonces.clone())
        .collect()
}

/// The root secret, recovered from the shares of `participants`' devices as a test-only
/// reconstruction.
pub fn reconstruct_root_secret(remote: &SpRemote, participants: &[usize]) -> Scalar {
    let ar = remote.access_structure_ref;
    let shares = participants
        .iter()
        .map(|&i| {
            // Each coordinator knows its own device's share, whoever else it knows.
            let key = remote.run.participants[i]
                .coordinator
                .get_frost_key(ar.key_id)
                .unwrap();
            let access_structure = key.access_structures().next().unwrap();
            let device_id = remote.device(i);
            let contrib = key
                .complete_key
                .coord_share_decryption_contrib(
                    ar.access_structure_id,
                    device_id,
                    TEST_ENCRYPTION_KEY,
                )
                .unwrap()
                .1;
            let index = access_structure.device_to_share_indicies()[&device_id];
            let share = remote.run.participants[i].devices[&device_id]
                .get_encrypted_share(ar, index)
                .unwrap()
                .decrypt(ar, contrib, &mut TestDeviceKeyGen)
                .unwrap();
            (index, share.share)
        })
        .collect::<Vec<_>>();

    let indices = shares.iter().map(|(index, _)| *index).collect::<Vec<_>>();
    shares
        .iter()
        .fold(Scalar::<Secret, Zero>::zero(), |acc, (index, share)| {
            let lambda = poly::eval_basis_poly_at_0(*index, indices.iter().copied());
            s!(acc + lambda * share)
        })
        .non_zero()
        .unwrap()
}

/// The BIP352 output keys of `recipients`, from the input secrets directly: the root secret is
/// taken whole and each input secret comes from `derive_xonly_key`, with no threshold involved.
pub fn expected_output_keys(
    root_secret: Scalar,
    inputs: &[(BitcoinBip32Path, OutPoint)],
    recipients: &[SilentPaymentRecipient],
) -> Vec<[u8; 32]> {
    let full_app_share = Xpub::from_rootkey(PairedSecretShare::new_unchecked(
        SecretShare {
            index: ShareIndex::from(Scalar::<Public, Zero>::from(1usize).non_zero().unwrap()),
            share: root_secret.mark_zero(),
        },
        public(root_secret),
    ))
    .rootkey_to_master_appkey();
    let a_sum = inputs
        .iter()
        .fold(Scalar::<Secret, Zero>::zero(), |sum, (path, _)| {
            let secret = AppTweak::Bitcoin(*path)
                .derive_xonly_key(&full_app_share)
                .share();
            s!(sum + secret)
        })
        .non_zero()
        .unwrap();

    let outpoints = inputs
        .iter()
        .map(|(_, outpoint)| *outpoint)
        .collect::<Vec<_>>();
    let smallest = silent_payments::smallest_outpoint(&outpoints).unwrap();
    let input_hash = silent_payments::input_hash(&smallest, public(a_sum)).unwrap();
    let shared_secrets = recipients
        .iter()
        .map(|recipient| recipient.scan_key)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|scan_key| {
            let secret = g!((input_hash * a_sum) * scan_key)
                .normalize()
                .non_zero()
                .unwrap();
            (scan_key, secret)
        })
        .collect::<BTreeMap<_, _>>();
    output_keys(recipients, &shared_secrets).unwrap()
}

pub fn bitcoin_template(task: &WireSignTask) -> &TransactionTemplate {
    let WireSignTask::BitcoinTransaction(template) = task else {
        panic!("expected a bitcoin transaction")
    };
    template
}
