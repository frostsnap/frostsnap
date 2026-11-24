use core::{convert::TryInto, str::FromStr};
use frost_backup::*;
use schnorr_fun::frost::SharedKey;
use secp256kfun::{marker::*, Point, Scalar};

mod common;
use common::{
    INVALID_SECRET_BACKUP_POLY_CHECKSUM, INVALID_SET_NO_FINGERPRINT, INVALID_SHARE_CHECKSUM,
    INVALID_SHARE_POLY_CHECKSUM, INVALID_SHARE_SCALAR, TEST_SECRET_BACKUP, TEST_SHARES_1_OF_1,
    TEST_SHARES_2_OF_3, TEST_SHARES_3_OF_5, TEST_SHARES_4_OF_4,
};

/// Iterator that generates all combinations of k elements from n elements
struct Combinations {
    n: usize,
    k: usize,
    combo: Vec<usize>,
    first: bool,
}

impl Combinations {
    fn new(n: usize, k: usize) -> Self {
        Combinations {
            n,
            k,
            combo: (0..k).collect(),
            first: true,
        }
    }
}

impl Iterator for Combinations {
    type Item = Vec<usize>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.k > self.n {
            return None;
        }

        if self.first {
            self.first = false;
            return Some(self.combo.clone());
        }

        // Find the rightmost element that can be incremented
        let mut i = self.k;
        while i > 0 && (i == self.k || self.combo[i - 1] == self.n - self.k + i - 1) {
            i -= 1;
        }

        if i == 0 {
            return None;
        }

        // Increment and reset following elements
        self.combo[i - 1] += 1;
        for j in i..self.k {
            self.combo[j] = self.combo[j - 1] + 1;
        }

        Some(self.combo.clone())
    }
}

/// Test vectors for a 1-of-1 scheme
#[test]
fn test_specification_1_of_1() {
    // Parse the share
    let share: ShareBackup = TEST_SHARES_1_OF_1[0].parse().expect("Share should parse");

    // Verify index
    assert_eq!(TryInto::<u32>::try_into(share.index()).unwrap(), 1);

    let expected_secret = Scalar::<Secret, Zero>::from_str(
        "0101010101010101010101010101010101010101010101010101010101010101",
    )
    .unwrap();

    // Test recovery with single share
    let recovered = recovery::recover_secret(&[share], Fingerprint::default())
        .expect("Recovery should succeed");
    assert_eq!(
        recovered.secret, expected_secret,
        "Should recover the correct secret"
    );
}

/// Test vectors with hardcoded shares for a 2-of-3 scheme
/// These were generated with a known secret and should always produce the same result
#[test]
fn test_specification_2_of_3() {
    // Parse all shares
    let shares: Vec<ShareBackup> = TEST_SHARES_2_OF_3
        .iter()
        .enumerate()
        .map(|(i, share_str)| {
            share_str
                .parse()
                .unwrap_or_else(|_| panic!("Share {} should parse", i + 1))
        })
        .collect();

    // Verify indices
    for (i, share) in shares.iter().enumerate() {
        assert_eq!(
            TryInto::<u32>::try_into(share.index()).unwrap(),
            (i + 1) as u32
        );
    }
    let expected_secret = Scalar::<Secret, Zero>::from_str(
        "0101010101010101010101010101010101010101010101010101010101010101",
    )
    .unwrap();

    // Test all possible combinations of 2 shares from 3
    let mut first_polynomial = None;

    for combo in Combinations::new(3, 2) {
        let images: Vec<_> = combo
            .iter()
            .map(|&i| shares[i].share_image().unwrap())
            .collect();
        let shared_key = SharedKey::from_share_images(images);

        // Verify all combinations produce the same polynomial
        match first_polynomial {
            None => first_polynomial = Some(shared_key.point_polynomial().to_vec()),
            Some(ref first) => assert_eq!(
                first,
                &shared_key.point_polynomial().to_vec(),
                "All share combinations should produce the same polynomial"
            ),
        }

        // Test that this combination recovers the correct secret
        let selected_shares: Vec<ShareBackup> = combo.iter().map(|&i| shares[i].clone()).collect();
        let recovered = recovery::recover_secret(&selected_shares, Fingerprint::default())
            .expect("Recovery should succeed");
        assert_eq!(
            recovered.secret, expected_secret,
            "Combination {:?} should recover the correct secret",
            combo
        );
    }

    // The polynomial is the listed commitment
    let commitment: Vec<_> = first_polynomial
        .unwrap()
        .iter()
        .map(|point| point.to_string())
        .collect();
    assert_eq!(
        commitment,
        [
            "031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f",
            "0214ec06c944abcb9245902e82e37a23896a4069fd86ad7ae2f9cb91c59109109a",
        ]
    );
}

/// Test vectors for a 3-of-5 scheme
#[test]
fn test_specification_3_of_5() {
    // Parse all shares
    let shares: Vec<ShareBackup> = TEST_SHARES_3_OF_5
        .iter()
        .enumerate()
        .map(|(i, share_str)| {
            share_str
                .parse()
                .unwrap_or_else(|_| panic!("Share {} should parse", i + 1))
        })
        .collect();

    // Verify indices
    for (i, share) in shares.iter().enumerate() {
        assert_eq!(
            TryInto::<u32>::try_into(share.index()).unwrap(),
            (i + 1) as u32
        );
    }

    let expected_secret = Scalar::<Secret, Zero>::from_str(
        "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
    )
    .unwrap();

    // Generate all possible combinations of 3 shares from 5
    let mut first_polynomial = None;

    for combo in Combinations::new(5, 3) {
        let images: Vec<_> = combo
            .iter()
            .map(|&i| shares[i].share_image().unwrap())
            .collect();
        let shared_key = SharedKey::from_share_images(images);

        // Verify all combinations produce the same polynomial
        match first_polynomial {
            None => first_polynomial = Some(shared_key.point_polynomial().to_vec()),
            Some(ref first) => assert_eq!(
                first,
                &shared_key.point_polynomial().to_vec(),
                "All share combinations should produce the same polynomial"
            ),
        }

        // Test that this combination recovers the correct secret
        let selected_shares: Vec<ShareBackup> = combo.iter().map(|&i| shares[i].clone()).collect();
        let recovered = recovery::recover_secret(&selected_shares, Fingerprint::default())
            .expect("Recovery should succeed");
        assert_eq!(
            recovered.secret,
            expected_secret,
            "Combination {:?} (shares {},{},{}) should recover the correct secret",
            combo,
            combo[0] + 1,
            combo[1] + 1,
            combo[2] + 1
        );
    }

    // The polynomial is the listed commitment
    let commitment: Vec<_> = first_polynomial
        .unwrap()
        .iter()
        .map(|point| point.to_string())
        .collect();
    assert_eq!(
        commitment,
        [
            "02c6b754b20826eb925e052ee2c25285b162b51fdca732bcf67e39d647fb6830ae",
            "020edcb4850d4cd9a57009e9900bf54acdb240a2087e6071a3616808d73c8424f1",
            "029cf188eac4090ee194ca695133762e1ad22e709cf2a7e16d5494bfcfd245e25d",
        ]
    );
}

/// Test vectors for a 4-of-4 scheme: A_3 carries no fingerprint bits, so
/// recovery only succeeds if the fingerprint check respects its 36-bit cap
#[test]
fn test_specification_4_of_4() {
    let shares: Vec<ShareBackup> = TEST_SHARES_4_OF_4
        .iter()
        .map(|share_str| share_str.parse().expect("Share should parse"))
        .collect();

    let expected_secret = Scalar::<Secret, Zero>::from_str(
        "0202020202020202020202020202020202020202020202020202020202020202",
    )
    .unwrap();

    let recovered =
        recovery::recover_secret(&shares, Fingerprint::default()).expect("Recovery should succeed");
    assert_eq!(recovered.secret, expected_secret);
    assert_eq!(recovered.shared_key.point_polynomial().len(), 4);

    // The polynomial is the listed commitment
    let commitment: Vec<_> = recovered
        .shared_key
        .point_polynomial()
        .iter()
        .map(|point| point.to_string())
        .collect();
    assert_eq!(
        commitment,
        [
            "024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d0766",
            "020313fcdea366087be6d6e3f6dda8e94206ab2ecf4bc9c0579fabe53a8be6133a",
            "02a1870d54e1745899c36a2b5ad8d0d3954d7fe8a596bf5c1c2b6f8d5f66b1e179",
            "03ffa2679439054ac606a004ca7a304f0870d6d2e76a2be48f9c033046e96c39b6",
        ]
    );
}

/// Test that a set whose polynomial lacks the fingerprint is rejected even
/// though every checksum passes
#[test]
fn test_specification_fingerprint_failure() {
    let shares: Vec<ShareBackup> = INVALID_SET_NO_FINGERPRINT
        .iter()
        .map(|share_str| share_str.parse().expect("Share should parse"))
        .collect();

    assert!(recovery::recover_secret(&shares, Fingerprint::default()).is_err());
    assert!(recovery::recover_secret_fuzzy(&shares, Fingerprint::default(), None).is_none());

    let expected_secret = Scalar::<Secret, Zero>::from_str(
        "0303030303030303030303030303030303030303030303030303030303030303",
    )
    .unwrap();
    let recovered = recovery::recover_secret(&shares, Fingerprint::NONE)
        .expect("Recovery without a fingerprint should succeed");
    assert_eq!(recovered.secret, expected_secret);

    // Both polynomial checksums verified against the listed commitment
    let commitment: Vec<_> = recovered
        .shared_key
        .point_polynomial()
        .iter()
        .map(|point| point.to_string())
        .collect();
    assert_eq!(
        commitment,
        [
            "02531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe337",
            "03462779ad4aad39514614751a71085f2f10e1c7a593e4e030efb5b8721ce55b0b",
        ]
    );
}

/// Test that parsing and Display are inverses
#[test]
fn test_specification_roundtrip() {
    // Use a valid share from the test vectors
    let test_share = TEST_SHARES_2_OF_3[0];

    let share: ShareBackup = test_share.parse().expect("Should parse");
    let formatted = share.to_string();

    // The formatted string should parse back to the same share
    let reparsed: ShareBackup = formatted.parse().expect("Should parse formatted string");
    assert_eq!(
        TryInto::<u32>::try_into(share.index()).unwrap(),
        TryInto::<u32>::try_into(reparsed.index()).unwrap()
    );
    assert_eq!(share.to_words(), reparsed.to_words());
}

/// A `#0` backup encodes the secret itself and round-trips through the text format
#[test]
fn test_specification_secret_backup_roundtrip() {
    let secret = Scalar::<Secret, Zero>::from_str(
        "0101010101010101010101010101010101010101010101010101010101010101",
    )
    .unwrap();
    let (backups, _) = ShareBackup::generate_shares(
        secret.non_zero().unwrap(),
        1,
        1,
        Fingerprint::default(),
        &mut rand::thread_rng(),
    );
    let backup = backups[0].clone();

    let formatted = backup.to_string();
    assert_eq!(formatted, TEST_SECRET_BACKUP);

    // Strict recovery accepts a lone #0
    let recovered =
        recovery::recover_secret(core::slice::from_ref(&backup), Fingerprint::default())
            .expect("Recovery should succeed");
    assert_eq!(recovered.secret, secret);

    // The polynomial commitment is the public key alone
    let commitment: Vec<_> = recovered
        .shared_key
        .point_polynomial()
        .iter()
        .map(|point| point.to_string())
        .collect();
    assert_eq!(
        commitment,
        ["031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f"]
    );

    let parsed: ShareBackup = formatted.parse().expect("#0 backup should parse");
    assert!(parsed.index().is_zero());
    assert!(parsed.share_image().is_none());
    assert_eq!(parsed, backup);

    // A #0 backup is not a share
    assert!(matches!(
        parsed.extract_secret(&recovered.shared_key),
        Err(ShareBackupError::NotAShare)
    ));
}

/// The encoded scalar must be less than the group order; it is rejected, not reduced
#[test]
fn test_specification_scalar_below_group_order() {
    // All-ones scalar (every word is ZOO, index 2047) is >= n
    let result = ShareBackup::from_words(1, ["ZOO"; 25]);
    assert!(matches!(result, Err(ShareBackupError::InvalidScalar)));

    // The scalar N itself with a valid words checksum: rejected for its range,
    // so it was not reduced to 0 before the words checksum was checked
    let result = INVALID_SHARE_SCALAR.parse::<ShareBackup>();
    assert!(matches!(result, Err(ShareBackupError::InvalidScalar)));
}

/// Test that checksums are actually verified
#[test]
fn test_specification_checksum_validation() {
    // Valid share from test vectors
    let valid_share = TEST_SHARES_2_OF_3[0];

    let valid = valid_share.parse::<ShareBackup>();
    assert!(valid.is_ok(), "Valid share should parse");

    let invalid = INVALID_SHARE_CHECKSUM.parse::<ShareBackup>();
    assert!(
        matches!(invalid, Err(ShareBackupError::WordsChecksumFailed)),
        "Invalid checksum should fail"
    );
}

/// A share whose polynomial checksum does not match decodes, but fails
/// verification against the 2-of-3 polynomial commitment
#[test]
fn test_specification_poly_checksum_failure() {
    let share: ShareBackup = INVALID_SHARE_POLY_CHECKSUM
        .parse()
        .expect("Words checksum should pass");

    let commitment = [
        "031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f",
        "0214ec06c944abcb9245902e82e37a23896a4069fd86ad7ae2f9cb91c59109109a",
    ]
    .map(|hex| Point::<Normal, Public, Zero>::from_str(hex).unwrap());
    let shared_key = SharedKey::from_poly(commitment.to_vec());

    // It lies on the polynomial, so only the checksum can reject it
    let image = share.share_image().unwrap();
    assert_eq!(shared_key.share_image(image.index), image);
    assert!(matches!(
        share.extract_secret(&shared_key),
        Err(ShareBackupError::PolyChecksumFailed)
    ));

    // The search finds the polynomial from #1 and #2, and must still reject
    // the set, whichever copy of #1 comes first
    let valid: Vec<ShareBackup> = TEST_SHARES_2_OF_3
        .iter()
        .map(|share_str| share_str.parse().unwrap())
        .collect();
    let invalid: ShareBackup = INVALID_SHARE_POLY_CHECKSUM.parse().unwrap();
    for backups in [
        [valid[0].clone(), valid[1].clone(), invalid.clone()],
        [invalid.clone(), valid[0].clone(), valid[1].clone()],
    ] {
        assert!(recovery::recover_secret_fuzzy(&backups, Fingerprint::default(), None).is_none());
    }
}

/// A `#0` backup whose polynomial checksum does not match decodes, but fails
/// verification against its own public key
#[test]
fn test_specification_secret_backup_poly_checksum_failure() {
    let backup: ShareBackup = INVALID_SECRET_BACKUP_POLY_CHECKSUM
        .parse()
        .expect("Words checksum should pass");
    assert!(backup.index().is_zero());
    assert!(matches!(
        recovery::recover_secret(&[backup], Fingerprint::default()),
        Err(recovery::RecoveryError::SecretExtractionFailed)
    ));
}

/// The root xprv and first addresses of the 0x0101…01 secret under the
/// descriptor `tr(xprv/0/0/0/0/<0;1>/*)`
#[test]
fn test_specification_wallet_derivation() {
    let secret = Scalar::<Secret, NonZero>::from_str(
        "0101010101010101010101010101010101010101010101010101010101010101",
    )
    .unwrap();

    let xprv = generate_xpriv(&secret, bitcoin::NetworkKind::Main);
    assert_eq!(
        xprv.to_string(),
        "xprv9s21ZrQH143K24Mfq5zL5MhWK9hUhhGbd45hLXo2Pq2oqzMMo63oStZzF93yjHmmfwkTW7jWmaf7X9aF3GP9D3mXSChQcm2zAZG6kerWdMw"
    );

    let secp = bitcoin::secp256k1::Secp256k1::new();
    for (path, expected_address) in [
        // First receive address
        (
            "m/0/0/0/0/0/0",
            "bc1pep252ktfxrz05jxzk4ts93jpxz6hurx2nz82yac4slxyzk2r06xq86fjw2",
        ),
        // First change address
        (
            "m/0/0/0/0/1/0",
            "bc1pdx4z22rx7pe9gps3mal0naczs2aqprms9upfpul506hrl5nknsdsqec4e3",
        ),
    ] {
        let path = bitcoin::bip32::DerivationPath::from_str(path).unwrap();
        let derived = xprv.derive_priv(&secp, &path).unwrap();
        let pubkey = derived.to_keypair(&secp).x_only_public_key().0;
        let address = bitcoin::Address::p2tr(&secp, pubkey, None, bitcoin::Network::Bitcoin);
        assert_eq!(address.to_string(), expected_address);
    }
}
