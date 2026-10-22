use frostsnap_core::silent_payments::{dleq_prove, dleq_verify};
use schnorr_fun::fun::{hex, Point, Scalar};

fn point(hex_str: &str) -> Point {
    Point::from_slice(&hex::decode(hex_str).unwrap()).unwrap()
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
