use core::{convert::TryInto, str::FromStr};
use frost_backup::*;
use schnorr_fun::frost::SharedKey;
use secp256kfun::{marker::*, Scalar};

mod common;
use common::TEST_SHARES_3_OF_5;

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

/// Generate all combinations of k elements from n elements
fn generate_combinations(n: usize, k: usize) -> Combinations {
    Combinations::new(n, k)
}

/// Test vectors with hardcoded shares for a 2-of-3 scheme
/// These were generated with a known secret and should always produce the same result
#[test]
fn test_specification_2_of_3() {
    // Hardcoded test shares for a 2-of-3 scheme
    // Secret used: 0x0101010101010101010101010101010101010101010101010101010101010101
    // Generated with fingerprint: 18-bit "frost-v0"
    const SHARE_1: &str = "#1 MENU ENROLL ETERNAL RELEASE ALONE ARREST LOTTERY BELOW DESIGN NORTH IRON RECALL RIDGE QUANTUM ENTRY BUYER OVER DIRECT FURNACE SUBMIT KIDNEY SEARCH NUMBER CAKE BADGE";
    const SHARE_2: &str = "#2 BEST MIXTURE FOOT HABIT WORLD OBSERVE ADVICE ANNUAL ISSUE CAUSE PROPERTY GUESS RETURN HURDLE WEASEL CUP ONCE NOVEL MARCH VALVE BLIND TRIGGER CHAIR ACTOR MONTH";
    const SHARE_3: &str = "#3 PANDA SPHERE HAIR BRAVE VIRUS CATTLE LOOP WRAP RAMP READY TIP BODY GIANT OYSTER DIZZY CRUSH DANGER SNOW PLANET SHOVE LIQUID CLAW RICE AMONG JOB";

    // Parse the shares
    let share1: ShareBackup = SHARE_1.parse().expect("Share 1 should parse");
    let share2: ShareBackup = SHARE_2.parse().expect("Share 2 should parse");
    let share3: ShareBackup = SHARE_3.parse().expect("Share 3 should parse");

    // Verify indices
    assert_eq!(TryInto::<u32>::try_into(share1.index()).unwrap(), 1);
    assert_eq!(TryInto::<u32>::try_into(share2.index()).unwrap(), 2);
    assert_eq!(TryInto::<u32>::try_into(share3.index()).unwrap(), 3);

    let shares = vec![share1, share2, share3];
    let expected_secret = Scalar::<Secret, Zero>::from_str(
        "0101010101010101010101010101010101010101010101010101010101010101",
    )
    .unwrap();

    // Test all possible combinations of 2 shares from 3
    let mut first_polynomial = None;

    for combo in generate_combinations(3, 2) {
        let images: Vec<_> = combo.iter().map(|&i| shares[i].share_image()).collect();
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
        let recovered = recovery::recover_secret(&selected_shares, FINGERPRINT)
            .expect("Recovery should succeed");
        assert_eq!(
            recovered.secret, expected_secret,
            "Combination {:?} should recover the correct secret",
            combo
        );
    }
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

    for combo in generate_combinations(5, 3) {
        let images: Vec<_> = combo.iter().map(|&i| shares[i].share_image()).collect();
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
        let recovered = recovery::recover_secret(&selected_shares, FINGERPRINT)
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
}

/// Test that parsing and Display are inverses
#[test]
fn test_specification_roundtrip() {
    // Use a valid share from the test vectors
    const TEST_SHARE: &str = "#1 MENU ENROLL ETERNAL RELEASE ALONE ARREST LOTTERY BELOW DESIGN NORTH IRON RECALL RIDGE QUANTUM ENTRY BUYER OVER DIRECT FURNACE SUBMIT KIDNEY SEARCH NUMBER CAKE BADGE";

    let share: ShareBackup = TEST_SHARE.parse().expect("Should parse");
    let formatted = share.to_string();

    // The formatted string should parse back to the same share
    let reparsed: ShareBackup = formatted.parse().expect("Should parse formatted string");
    assert_eq!(
        TryInto::<u32>::try_into(share.index()).unwrap(),
        TryInto::<u32>::try_into(reparsed.index()).unwrap()
    );
    assert_eq!(share.to_words(), reparsed.to_words());
}

/// Test that checksums are actually verified
#[test]
fn test_specification_checksum_validation() {
    // Valid share 1 from 2-of-3 scheme above
    const VALID_SHARE: &str = "#1 MENU ENROLL ETERNAL RELEASE ALONE ARREST LOTTERY BELOW DESIGN NORTH IRON RECALL RIDGE QUANTUM ENTRY BUYER OVER DIRECT FURNACE SUBMIT KIDNEY SEARCH NUMBER CAKE BADGE";

    // Same share but with last word changed (breaks checksum)
    const INVALID_SHARE: &str = "#1 MENU ENROLL ETERNAL RELEASE ALONE ARREST LOTTERY BELOW DESIGN NORTH IRON RECALL RIDGE QUANTUM ENTRY BUYER OVER DIRECT FURNACE SUBMIT KIDNEY SEARCH NUMBER CAKE ABANDON";

    let valid = VALID_SHARE.parse::<ShareBackup>();
    assert!(valid.is_ok(), "Valid share should parse");

    let invalid = INVALID_SHARE.parse::<ShareBackup>();
    assert!(invalid.is_err(), "Invalid checksum should fail");
}
