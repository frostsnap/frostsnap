use std::{
    collections::{BTreeMap, BTreeSet},
    str::FromStr,
};

use bitcoin::{hashes::Hash, OutPoint, Txid};
use frostsnap_core::{
    silent_payments::{
        combine_ecdh_shares, dleq_prove, dleq_verify, input_hash, output_keys,
        SilentPaymentRecipient,
    },
    tweak::{
        AppTweak, BitcoinAccount, BitcoinAccountKeychain, BitcoinBip32Path, Keychain, TweakContext,
        Xpub,
    },
};
use schnorr_fun::{
    frost::{PairedSecretShare, SecretShare, ShareIndex, SharedKey},
    fun::{g, hex, marker::*, poly, s, Point, Scalar, G},
};
use serde_json::Value;

fn point(hex_str: &str) -> Point {
    Point::from_slice(&hex::decode(hex_str).unwrap()).unwrap()
}

fn share_index(index: usize) -> ShareIndex {
    Scalar::<Public, Zero>::from(index).non_zero().unwrap()
}

fn path(account: BitcoinAccount, keychain: Keychain, index: u32) -> BitcoinBip32Path {
    BitcoinBip32Path {
        account_keychain: BitcoinAccountKeychain { account, keychain },
        index,
    }
}

fn identity_input() -> TweakContext {
    TweakContext {
        gacc: Scalar::<Public>::one().mark_zero(),
        tacc: Scalar::<Public, Zero>::zero(),
        gv: Scalar::<Public>::one().mark_zero(),
    }
}

/// A nonzero scalar from 32 bytes, reduced mod the curve order.
fn scalar(bytes: [u8; 32]) -> Scalar {
    Scalar::<Secret, Zero>::from_bytes_mod_order(bytes)
        .non_zero()
        .unwrap()
}

/// The point `secret * G` for a secret used as a root key in these tests.
fn root_public(secret: Scalar) -> Point {
    g!(secret * G).normalize().non_zero().unwrap()
}

/// secp256k1's own generator, compressed. The DLEQ proofs here fix the generator to `G`, so the
/// BIP374 vectors that use another generator are skipped.
const SECP256K1_G: &str = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";

#[test]
fn bip374_generate_proof_vectors() {
    let csv = include_str!("data/bip374_generate_proof.csv");
    for row in csv.lines().skip(1) {
        let fields = row.split(',').collect::<Vec<_>>();
        let [index, g_hex, a_hex, b_hex, r_hex, msg_hex, expected, comment] = fields.as_slice()
        else {
            panic!("malformed row: {row}");
        };
        if g_hex.to_lowercase() != SECP256K1_G {
            continue;
        }

        let a = Scalar::from_slice(&hex::decode(a_hex).unwrap());
        // Infinity cannot be represented as a `Point`, and the spec fails it, so it is `None` here.
        let b = (*b_hex != "INFINITY").then(|| point(b_hex));
        let r = hex::decode_array::<32>(r_hex).unwrap();
        let m = (!msg_hex.is_empty()).then(|| hex::decode_array::<32>(msg_hex).unwrap());
        let expected_proof =
            (*expected != "INVALID").then(|| hex::decode_array::<64>(expected).unwrap());

        let actual = a
            .zip(b)
            .and_then(|(a, b)| dleq_prove(a, b, &r, m.as_ref()))
            .map(|(proof, _)| proof);
        assert_eq!(actual, expected_proof, "vector {index} ({comment})");
    }
}

#[test]
fn bip374_verify_proof_vectors() {
    let csv = include_str!("data/bip374_verify_proof.csv");
    for row in csv.lines().skip(1) {
        let fields = row.split(',').collect::<Vec<_>>();
        let [index, g_hex, a_hex, b_hex, c_hex, proof_hex, msg_hex, expected, comment] =
            fields.as_slice()
        else {
            panic!("malformed row: {row}");
        };
        if g_hex.to_lowercase() != SECP256K1_G {
            continue;
        }

        let proof = hex::decode_array::<64>(proof_hex).unwrap();
        let m = (!msg_hex.is_empty()).then(|| hex::decode_array::<32>(msg_hex).unwrap());
        let actual = dleq_verify(point(a_hex), point(b_hex), point(c_hex), &proof, m.as_ref());
        assert_eq!(actual, *expected == "TRUE", "vector {index} ({comment})");
    }
}

/// The BIP352 output keys of a sending case, computed from a fixed summed input secret and then
/// from every 2-of-3 split of that secret.
///
/// Cases whose vector expects no outputs are skipped, since there is nothing to derive.
#[test]
fn bip352_sending_vectors() {
    let cases: Value =
        serde_json::from_str(include_str!("data/bip352_sending_vectors.json")).unwrap();
    let mut checked = 0;

    for case in cases.as_array().unwrap() {
        for sending in case["sending"].as_array().unwrap() {
            let given = &sending["given"];
            let expected = &sending["expected"];
            let expected_sets = expected["outputs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|outputs| {
                    outputs
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|o| o.as_str().unwrap().to_string())
                        .collect::<BTreeSet<_>>()
                })
                .collect::<Vec<_>>();
            if expected_sets.iter().all(BTreeSet::is_empty) {
                continue;
            }

            // Inputs without a usable key, such as script path spends of the NUMS key, are left out
            // of the sum, so the case is only checked when every input contributes a key.
            let vins = given["vin"].as_array().unwrap();
            if expected["input_pub_keys"].as_array().unwrap().len() != vins.len() {
                continue;
            }
            let mut a_sum = Scalar::<Secret, Zero>::zero();
            let mut outpoints = Vec::new();
            for vin in vins {
                let txid = Txid::from_str(vin["txid"].as_str().unwrap()).unwrap();
                let vout = vin["vout"].as_u64().unwrap() as u32;
                outpoints.push(OutPoint { txid, vout });

                // Taproot inputs use the even-Y secret, as in BIP352.
                let spk = vin["prevout"]["scriptPubKey"]["hex"].as_str().unwrap();
                let is_p2tr = spk.len() == 68 && spk.starts_with("5120");
                let mut key = Scalar::<Secret>::from_slice(
                    &hex::decode(vin["private_key"].as_str().unwrap()).unwrap(),
                )
                .unwrap();
                if is_p2tr && !g!(key * G).normalize().is_y_even() {
                    key = -key;
                }
                a_sum = s!(a_sum + key);
            }
            assert_eq!(
                a_sum.to_bytes(),
                hex::decode_array::<32>(expected["input_private_key_sum"].as_str().unwrap())
                    .unwrap(),
                "{}",
                case["comment"]
            );
            let a_sum = a_sum.non_zero().unwrap();

            let smallest = outpoints
                .iter()
                .min_by_key(|op| (op.txid.to_byte_array(), op.vout.to_le_bytes()))
                .unwrap();
            let ih = input_hash(smallest, g!(a_sum * G).normalize().non_zero().unwrap()).unwrap();

            let mut recipients = Vec::new();
            for entry in given["recipients"].as_array().unwrap() {
                let count = entry.get("count").and_then(Value::as_u64).unwrap_or(1);
                for _ in 0..count {
                    recipients.push(SilentPaymentRecipient {
                        scan_key: point(entry["scan_pub_key"].as_str().unwrap()),
                        spend_key: point(entry["spend_pub_key"].as_str().unwrap()),
                    });
                }
            }
            let scan_keys = recipients
                .iter()
                .map(|r| r.scan_key)
                .collect::<BTreeSet<_>>();

            let assert_matches = |secrets: &BTreeMap<Point, Point>, label: &str| {
                let outputs = output_keys(&recipients, secrets)
                    .unwrap_or_else(|| panic!("{label}: outputs failed"));
                let actual = outputs
                    .iter()
                    .map(|o| hex::encode(o))
                    .collect::<BTreeSet<_>>();
                assert!(
                    expected_sets.contains(&actual),
                    "{}: {label} did not match any expected output set",
                    case["comment"]
                );
            };

            // One party holding the whole input secret.
            let single = scan_keys
                .iter()
                .map(|scan| {
                    let ecdh = g!((ih * a_sum) * scan).normalize().non_zero().unwrap();
                    (*scan, ecdh)
                })
                .collect::<BTreeMap<_, _>>();
            assert_matches(&single, "single party");

            // The same secret split as f(x) = a_sum + c*x, checked for every 2-of-3 subset.
            let c = Scalar::<Secret, Zero>::from_bytes_mod_order([0x5c; 32]);
            let f = [a_sum.mark_zero(), c];
            let share_secrets = (1..=3)
                .map(|j| poly::scalar::eval(&f, share_index(j)))
                .collect::<Vec<_>>();
            for subset in [[0, 1], [0, 2], [1, 2]] {
                let secrets = scan_keys
                    .iter()
                    .map(|scan| {
                        let shares = subset
                            .iter()
                            .map(|&j| {
                                let index = share_index(j + 1);
                                let share =
                                    g!(share_secrets[j] * scan).normalize().non_zero().unwrap();
                                (index, share)
                            })
                            .collect::<Vec<_>>();
                        let ecdh =
                            combine_ecdh_shares(ih, *scan, &[identity_input()], &shares).unwrap();
                        (*scan, ecdh)
                    })
                    .collect::<BTreeMap<_, _>>();
                assert_matches(&secrets, &format!("subset {subset:?}"));
            }
            checked += 1;
        }
    }
    assert!(checked > 0);
}

/// BIP352 allows at most `K_max` outputs to one scan key. The limit is the recipient count, so the
/// `K_max + 1`th output to the same scan key is refused.
#[test]
fn bip352_output_limit_per_scan_key() {
    let scan = root_public(scalar([0x11; 32]));
    let spend = root_public(scalar([0x12; 32]));
    let secrets = BTreeMap::from([(scan, scan)]);
    let recipient = SilentPaymentRecipient {
        scan_key: scan,
        spend_key: spend,
    };

    let at_limit = vec![recipient; 2323];
    assert_eq!(output_keys(&at_limit, &secrets).unwrap().len(), 2323);

    let over_limit = vec![recipient; 2324];
    assert!(output_keys(&over_limit, &secrets).is_none());
}

/// Each path's tweak context must reproduce the secret that `derive_xonly_key` gives it when the
/// whole account secret is held by one party. Several paths are used so that some internal keys
/// have odd Y, which is where the even-Y handling matters.
#[test]
fn tweak_context_matches_derive_xonly_key() {
    let root_secret = scalar([0x42; 32]);
    let root_share = PairedSecretShare::new_unchecked(
        SecretShare {
            index: share_index(1),
            share: root_secret.mark_zero(),
        },
        root_public(root_secret),
    );
    let app_share = Xpub::from_rootkey(root_share).rootkey_to_master_appkey();

    let account = BitcoinAccount {
        index: 3,
        ..Default::default()
    };
    let account_share = account.derive_account_key(&app_share);
    let account_xpub = Xpub::new(account_share.key().public_key(), account_share.chaincode);
    let account_secret = account_share.key().share();

    // The account image from the app polynomial matches the account share.
    let root_shared_key = SharedKey::from_poly(vec![root_public(root_secret).mark_zero()])
        .non_zero()
        .unwrap();
    let app_shared_key = Xpub::from_rootkey(root_shared_key).rootkey_to_master_appkey();
    let account_shared_key = account.derive_account_key(&app_shared_key);
    assert_eq!(
        account_shared_key.key().share_image(share_index(1)).image,
        g!(account_secret * G).normalize()
    );

    let mut saw_odd_internal_key = false;
    for keychain_path in (0..16).flat_map(|i| {
        [
            path(account, Keychain::External, i),
            path(account, Keychain::Internal, i),
        ]
    }) {
        let child = account_share.derive_bip32([
            keychain_path.account_keychain.keychain as u32,
            keychain_path.index,
        ]);
        saw_odd_internal_key |= !child.key().public_key().is_y_even();

        let ctx = keychain_path.tweak_context(&account_xpub);
        let expected = s!(ctx.gv * (ctx.gacc * account_secret + ctx.tacc));
        let derived = AppTweak::Bitcoin(keychain_path).derive_xonly_key(&app_share);
        assert_eq!(derived.share(), expected, "path {keychain_path:?}");
    }
    assert!(
        saw_odd_internal_key,
        "no path exercised an odd-Y internal key"
    );
}

/// Shares of the account secret for a 2-of-3 sharing, combined over several inputs, must give the
/// BIP352 shared secret of the sum of the input secrets computed directly.
#[test]
fn multi_input_ecdh_matches_sum_of_input_secrets() {
    let root_secret = scalar([0x17; 32]);
    let c = Scalar::<Secret, Zero>::from_bytes_mod_order([0x29; 32]);
    let root_poly = [root_secret.mark_zero(), c];
    let root_points = [root_public(root_secret).mark_zero(), g!(c * G).normalize()];
    let root_shared_key = SharedKey::from_poly(root_points.to_vec())
        .non_zero()
        .unwrap();

    let account = BitcoinAccount::default();
    let account_key_shares = (1..=3)
        .map(|j| {
            let index = share_index(j);
            let root_share = PairedSecretShare::new_unchecked(
                SecretShare {
                    index,
                    share: poly::scalar::eval(&root_poly, index),
                },
                root_public(root_secret),
            );
            let app_share = Xpub::from_rootkey(root_share).rootkey_to_master_appkey();
            account.derive_account_key(&app_share).key().share()
        })
        .collect::<Vec<_>>();

    let account_shared_key =
        account.derive_account_key(&Xpub::from_rootkey(root_shared_key).rootkey_to_master_appkey());
    let account_xpub = Xpub::new(
        account_shared_key.key().public_key(),
        account_shared_key.chaincode,
    );

    // Ground truth: the input secrets as derive_xonly_key gives them with the root secret whole.
    let full_app_share = Xpub::from_rootkey(PairedSecretShare::new_unchecked(
        SecretShare {
            index: share_index(1),
            share: root_secret.mark_zero(),
        },
        root_public(root_secret),
    ))
    .rootkey_to_master_appkey();

    let paths = (0..4)
        .flat_map(|i| {
            let index = i;
            [
                BitcoinBip32Path::external(index),
                BitcoinBip32Path::internal(index),
            ]
        })
        .collect::<Vec<_>>();

    let input_secret_sum = paths
        .iter()
        .fold(Scalar::<Secret, Zero>::zero(), |sum, path| {
            let input_secret = AppTweak::Bitcoin(*path)
                .derive_xonly_key(&full_app_share)
                .share();
            s!(sum + input_secret)
        });
    let input_secret_sum = input_secret_sum.non_zero().unwrap();

    let ih = Scalar::<Public, Zero>::from_bytes_mod_order([0x05; 32])
        .non_zero()
        .unwrap();
    let b_scan = root_public(scalar([0x33; 32]));
    let expected = g!((ih * input_secret_sum) * b_scan)
        .normalize()
        .non_zero()
        .unwrap();

    let inputs = paths
        .iter()
        .map(|path| path.tweak_context(&account_xpub))
        .collect::<Vec<_>>();

    for subset in [[0, 1], [0, 2], [1, 2]] {
        let shares = subset
            .iter()
            .map(|&j| {
                let share = g!(account_key_shares[j] * b_scan)
                    .normalize()
                    .non_zero()
                    .unwrap();
                (share_index(j + 1), share)
            })
            .collect::<Vec<_>>();
        let combined = combine_ecdh_shares(ih, b_scan, &inputs, &shares).unwrap();
        assert_eq!(combined, expected, "subset {subset:?}");

        // Each share is proven against its image from the account shared key.
        let input_hash_bytes = ih.to_bytes();
        for &j in &subset {
            let secret = account_key_shares[j].non_zero().unwrap();
            let (proof, _) =
                dleq_prove(secret, b_scan, &[7u8; 32], Some(&input_hash_bytes)).expect("proof");
            let image = account_shared_key
                .key()
                .share_image(share_index(j + 1))
                .image
                .non_zero()
                .unwrap();
            let share = shares[subset.iter().position(|&x| x == j).unwrap()].1;
            assert!(dleq_verify(
                image,
                b_scan,
                share,
                &proof,
                Some(&input_hash_bytes)
            ));
        }
    }

    // Two entries with the same share index are not a valid threshold set.
    let duplicate = g!(account_key_shares[0] * b_scan)
        .normalize()
        .non_zero()
        .unwrap();
    assert!(combine_ecdh_shares(
        ih,
        b_scan,
        &inputs,
        &[(share_index(1), duplicate), (share_index(1), duplicate)],
    )
    .is_none());
}
