//! The offline recovery tool for the 64-word card (spec PK.8 escape hatch, owner B13 as amended
//! 2 Nov), end to end: card words in, the recovered share out, equal to the share the card was
//! written from.
//!
//! (a) an honest card round-trips, with or without the wallet polynomial or other backups;
//! (b) a lying writer chip's card (non-KDF masks) is recovered although the app's restore
//! refuses it; (c) repairs, with and without the wallet polynomial; (d) a card past repair
//! fails with B14's step-4 message and a non-zero exit; (e) the CLI takes words robustly.

use frost_backup::{
    bip39_words::BIP39_WORDS,
    verifiable_card::{
        codec::{self, Entries},
        layout::{self, FREE, HALF, WORDS},
        offline::{self, Failure, Found, KeySource, ParseError, UNRECOVERABLE},
    },
    ShareBackup, FINGERPRINT,
};
use frostsnap_core::device::card_restore::{
    CardEntries, RestoreFailure, RestoreJob, RestoreOutcome, RESTORE_DECODE_MS,
};
use rand::{seq::SliceRandom, Rng};
use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;
use schnorr_fun::{
    frost::{SecretShare, SharedKey},
    fun::prelude::*,
};
use std::io::Write;
use std::process::{Command, Output, Stdio};
use std::str::FromStr;

struct Wallet {
    key: SharedKey,
    backups: Vec<ShareBackup>,
}

impl Wallet {
    fn new(threshold: usize, n: usize, rng: &mut ChaCha20Rng) -> Self {
        let secret = Scalar::random(rng);
        let (backups, key) = ShareBackup::generate_shares(secret, threshold, n, FINGERPRINT, rng);
        Self { key, backups }
    }

    /// Share `i` (from 0).
    fn share(&self, i: usize) -> SecretShare {
        self.backups[i].clone().extract_secret(&self.key).unwrap()
    }

    /// Every backup but share `i`'s.
    fn others(&self, i: usize) -> Vec<ShareBackup> {
        let mut others = self.backups.clone();
        others.remove(i);
        others
    }

    fn card(&self, i: usize) -> [u16; WORDS] {
        codec::encode(&self.share(i), &self.key).unwrap().words
    }

    fn poly_text(&self) -> String {
        self.key
            .point_polynomial()
            .iter()
            .map(|point| point.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// A lying writer chip's card for share `i`: the 311 committed bits are the share's, so the
/// words encode its s and index, but the phone half (masks m5..m24 and the wallet-bound masks)
/// is any 32 words the chip picked, as in the audit PoC (`a_phone_half_that_isnt_the_masks_
/// doesnt_restore`). With `free`, the 55 free bits (the hidden random values: the threshold
/// and m1..m4) are random too (`free_bits_are_unbound_and_only_the_masks_notice`). Every
/// proof and opening passes for such a card.
fn lying_card(wallet: &Wallet, i: usize, free: bool, rng: &mut ChaCha20Rng) -> [u16; WORDS] {
    let share = wallet.share(i);
    let card = codec::encode(&share, &wallet.key).unwrap();
    let mut bits = layout::committed_bits(&card.values);
    if free {
        for v in 0..FREE {
            bits.set(v, rng.gen());
        }
    }
    let phone: [u16; HALF] = core::array::from_fn(|_| rng.gen_range(0..2048));
    let index = u8::try_from(u32::try_from(share.index).unwrap()).unwrap();
    layout::words_from_bits(&bits, index, &phone)
}

fn entries(words: &[u16; WORDS]) -> Entries {
    words.map(Some)
}

fn text(words: &[u16; WORDS]) -> String {
    words
        .iter()
        .map(|&w| BIP39_WORDS[w as usize].to_lowercase())
        .collect::<Vec<_>>()
        .join(" ")
}

/// `n` distinct word positions.
fn positions(n: usize, rng: &mut ChaCha20Rng) -> Vec<usize> {
    let mut all: Vec<usize> = (0..WORDS).collect();
    all.shuffle(rng);
    all.truncate(n);
    all
}

/// The entries with `wrong` words changed and `unreadable` words erased, at distinct places.
fn damage(words: &[u16; WORDS], wrong: usize, unreadable: usize, rng: &mut ChaCha20Rng) -> Entries {
    let mut entries = entries(words);
    let places = positions(wrong + unreadable, rng);
    for &j in &places[..wrong] {
        entries[j] = Some((words[j] + rng.gen_range(1..2048)) % 2048);
    }
    for &j in &places[wrong..] {
        entries[j] = None;
    }
    entries
}

/// The share the CLI printed on stdout: "index: N" and "secret: HEX", and nothing else.
fn printed_share(output: &Output) -> SecretShare {
    let out = String::from_utf8(output.stdout.clone()).unwrap();
    let mut lines = out.lines();
    let index: u32 = lines
        .next()
        .and_then(|line| line.strip_prefix("index: "))
        .unwrap_or_else(|| panic!("{out}"))
        .parse()
        .unwrap();
    let secret = lines
        .next()
        .and_then(|line| line.strip_prefix("secret: "))
        .unwrap_or_else(|| panic!("{out}"));
    assert_eq!(secret.len(), 64, "{out}");
    assert!(lines.next().is_none(), "{out}");
    SecretShare {
        index: Scalar::<Public, Zero>::from(index).non_zero().unwrap(),
        share: Scalar::from_str(secret).unwrap(),
    }
}

fn run_tool(args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_card_offline_recover"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

/// A test's own directory for the files it hands the tool, removed when it's dropped.
struct TempDir(std::path::PathBuf);

impl TempDir {
    fn new(test: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "card_offline_recover_{}_{test}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn file(&self, name: &str, contents: &str) -> std::path::PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, contents).unwrap();
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

// (a) An honest card round-trips.

#[test]
fn honest_card_round_trips_with_the_polynomial() {
    let mut rng = ChaCha20Rng::from_seed([1; 32]);
    for (threshold, n) in [(1, 1), (2, 3), (3, 5)] {
        let wallet = Wallet::new(threshold, n, &mut rng);
        for i in 0..n {
            let recovered =
                offline::recover(&entries(&wallet.card(i)), Some(&wallet.key), &[]).unwrap();
            assert_eq!(recovered.found, Found::Unique);
            assert_eq!(recovered.decoded.share, wallet.share(i));
            assert!(recovered.decoded.masks.passed());
            assert_eq!(recovered.decoded.masks.wallet, Some(true));
            assert!(recovered.decoded.canonical.unwrap().is_canonical());
            assert_eq!(recovered.decoded.threshold, Some(threshold as u8));
            assert_eq!(recovered.key.as_ref().unwrap().1, KeySource::Given);
        }
    }
}

#[test]
fn honest_card_round_trips_with_other_backups() {
    let mut rng = ChaCha20Rng::from_seed([2; 32]);
    let wallet = Wallet::new(3, 5, &mut rng);
    // t - 1 other backups: the polynomial needs the card's image, and the fingerprint checks it.
    let two = &wallet.others(1)[..2];
    let recovered = offline::recover(&entries(&wallet.card(1)), None, two).unwrap();
    assert_eq!(
        recovered.key.as_ref().unwrap().1,
        KeySource::BackupsWithCard
    );
    // Read again against the F found, so the wallet-bound masks are reported.
    assert_eq!(recovered.decoded.masks.wallet, Some(true));
    assert!(recovered.decoded.canonical.unwrap().is_canonical());
    assert_eq!(recovered.decoded.share, wallet.share(1));
    // t others: the polynomial comes from them alone, and s·G checks the card.
    let recovered = offline::recover(&entries(&wallet.card(1)), None, &wallet.others(1)).unwrap();
    assert_eq!(recovered.key.as_ref().unwrap().1, KeySource::Backups);
    assert_eq!(recovered.decoded.share, wallet.share(1));
}

#[test]
fn the_cli_prints_the_cards_share() {
    let mut rng = ChaCha20Rng::from_seed([3; 32]);
    let wallet = Wallet::new(2, 3, &mut rng);
    let dir = TempDir::new("the_cli_prints_the_cards_share");
    let poly = dir.file("poly_a.txt", &wallet.poly_text());
    let others: String = wallet.others(2).iter().map(|b| format!("{b}\n")).collect();
    let backups = dir.file("backups_a.txt", &others);
    let words = text(&wallet.card(2));

    // Card -> tool -> the share the card was written from: with the words alone, with the
    // wallet polynomial, and with the other backups. Each says whether it checked the wallet.
    let cases = [
        (vec![], "Checked against the wallet: no"),
        (
            vec!["--poly", poly.to_str().unwrap()],
            "Checked against the wallet: yes, the share is on the wallet polynomial you gave",
        ),
        (
            vec!["--backups", backups.to_str().unwrap()],
            "Checked against the wallet: yes",
        ),
    ];
    for (args, checked) in cases {
        let output = run_tool(&args, &words);
        assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
        assert_eq!(printed_share(&output), wallet.share(2));
        let err = stderr(&output);
        assert!(err.contains("trust"), "{err}");
        assert!(err.contains("Share index: #3"), "{err}");
        assert!(err.contains("self-checking masks: passed"), "{err}");
        assert!(err.contains(checked), "{err}");
        assert!(err.contains("Share #3 recovered"), "{err}");
    }
    let output = run_tool(&["--poly", poly.to_str().unwrap()], &words);
    let err = stderr(&output);
    assert!(
        err.contains("Threshold on the card: 2 (the wallet's: 2)"),
        "{err}"
    );
    assert!(err.contains("wallet-bound masks: passed"), "{err}");
    // With the words alone the wallet-bound masks can't be checked.
    let output = run_tool(&[], &words);
    assert!(stderr(&output).contains("wallet-bound masks: not checked"));
    // There's no backup output any more.
    let output = run_tool(&["-o", "share.txt"], &words);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}

#[test]
fn without_a_polynomial_the_share_is_still_recovered() {
    let mut rng = ChaCha20Rng::from_seed([4; 32]);
    let wallet = Wallet::new(2, 3, &mut rng);
    let recovered = offline::recover(&entries(&wallet.card(0)), None, &[]).unwrap();
    assert_eq!(recovered.decoded.share, wallet.share(0));
    assert_eq!(recovered.found, Found::Unique);
    assert!(recovered.key.is_none());
    assert_eq!(recovered.decoded.masks.wallet, None);
}

// (b) A lying writer chip's card.

#[test]
fn a_lying_chips_card_is_recovered_though_the_app_refuses_it() {
    let mut rng = ChaCha20Rng::from_seed([5; 32]);
    let wallet = Wallet::new(2, 3, &mut rng);
    for free in [false, true] {
        let words = lying_card(&wallet, 1, free, &mut rng);
        let card = CardEntries(entries(&words));

        // The app's restore refuses it: against F the masks fail before s·G and the repair
        // finds nothing; read cold the masks fail and the device's gate (B7) refuses.
        let mut job = RestoreJob::new(&card, Some(&wallet.key), None, None);
        // Slices of a simulated 30 ms until T_dec, when the outcome comes out (owner R2-1).
        let outcome = (0..=RESTORE_DECODE_MS / 30 + 1)
            .find_map(|k| job.step(k * 30))
            .unwrap();
        assert!(matches!(
            outcome,
            RestoreOutcome::Failed(RestoreFailure::DontFit)
        ));
        let cold = codec::decode(&card.0, None, None).unwrap();
        assert!(!cold.masks.passed());

        // The tool recovers it with F, and with the other backups.
        let recovered = offline::recover(&card.0, Some(&wallet.key), &[]).unwrap();
        assert!(!recovered.decoded.masks.self_ok);
        assert_eq!(recovered.decoded.masks.wallet, Some(false));
        assert!(!recovered.decoded.canonical.unwrap().is_canonical());
        assert_eq!(recovered.decoded.share, wallet.share(1));
        let recovered = offline::recover(&card.0, None, &wallet.others(1)[..1]).unwrap();
        assert_eq!(
            recovered.key.as_ref().unwrap().1,
            KeySource::BackupsWithCard
        );
        assert_eq!(recovered.decoded.masks.wallet, Some(false));
        assert_eq!(recovered.decoded.share, wallet.share(1));
    }

    // The CLI says the masks failed and still prints the share.
    let words = lying_card(&wallet, 1, true, &mut rng);
    let dir = TempDir::new("a_lying_chips_card_is_recovered_though_the_app_refuses_it");
    let poly = dir.file("poly_b.txt", &wallet.poly_text());
    let output = run_tool(&["--poly", poly.to_str().unwrap()], &text(&words));
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let err = stderr(&output);
    assert!(err.contains("self-checking masks: FAILED"), "{err}");
    assert!(err.contains("wallet-bound masks: FAILED"), "{err}");
    assert!(err.contains("doesn't restore through the app"), "{err}");
    assert_eq!(printed_share(&output), wallet.share(1));
}

// (c) Repairs.

#[test]
fn one_wrong_word_is_corrected_without_a_target() {
    let mut rng = ChaCha20Rng::from_seed([6; 32]);
    let wallet = Wallet::new(2, 3, &mut rng);
    for liar in [false, true] {
        let words = if liar {
            lying_card(&wallet, 0, false, &mut rng)
        } else {
            wallet.card(0)
        };
        let entries = damage(&words, 1, 0, &mut rng);
        let recovered = offline::recover(&entries, None, &[]).unwrap();
        assert_eq!(recovered.found, Found::Unique);
        assert_eq!(recovered.decoded.codeword.corrected.count_ones(), 1);
        assert_eq!(recovered.decoded.share, wallet.share(0));
        assert_eq!(recovered.decoded.masks.self_ok, !liar);
        // And checked against the other backups.
        let recovered = offline::recover(&entries, None, &wallet.others(0)).unwrap();
        assert!(recovered.key.is_some());
        assert_eq!(recovered.decoded.share, wallet.share(0));
    }
}

/// With no target the words alone decide the share only while a spare word checks it, or the
/// self masks confirm it: an honest card past unique decoding repairs on the masks; a lying
/// chip's card doesn't, and the tool says it can't decide rather than guess.
#[test]
fn without_a_target_only_a_determined_share_is_taken() {
    let mut rng = ChaCha20Rng::from_seed([7; 32]);
    let wallet = Wallet::new(2, 3, &mut rng);
    let honest = wallet.card(2);
    let liar = lying_card(&wallet, 2, false, &mut rng);
    for (wrong, unreadable) in [(0, 3), (1, 1), (2, 0), (2, 1)] {
        let entries = damage(&honest, wrong, unreadable, &mut rng);
        let recovered = offline::recover(&entries, None, &[]).unwrap();
        assert_eq!(
            recovered.decoded.share,
            wallet.share(2),
            "{wrong} {unreadable}"
        );
        assert!(recovered.decoded.masks.self_ok);

        let entries = damage(&liar, wrong, unreadable, &mut rng);
        assert_eq!(
            offline::recover(&entries, None, &[]).unwrap_err(),
            Failure::Unrecoverable { had_key: false },
            "{wrong} {unreadable}"
        );
    }
    // Up to two unreadable words and nothing wrong still leave a spare word to check.
    let entries = damage(&liar, 0, 2, &mut rng);
    let recovered = offline::recover(&entries, None, &[]).unwrap();
    assert_eq!(recovered.decoded.share, wallet.share(2));
}

#[test]
fn up_to_the_repair_limit_is_repaired_with_the_polynomial() {
    let mut rng = ChaCha20Rng::from_seed([8; 32]);
    let wallet = Wallet::new(3, 5, &mut rng);
    let honest = wallet.card(3);
    let liar = lying_card(&wallet, 3, true, &mut rng);
    let cases = [(1, 0), (2, 0), (3, 0), (2, 1), (1, 2), (0, 3), (1, 1)];
    for words in [&honest, &liar] {
        for (wrong, unreadable) in cases {
            let entries = damage(words, wrong, unreadable, &mut rng);
            let recovered = offline::recover(&entries, Some(&wallet.key), &[])
                .unwrap_or_else(|e| panic!("{wrong} wrong, {unreadable} unreadable: {e:?}"));
            assert_eq!(recovered.decoded.share, wallet.share(3));
            assert_eq!(
                recovered.decoded.codeword.corrected.count_ones() as usize,
                wrong
            );
            assert_eq!(
                recovered.decoded.codeword.erased.count_ones() as usize,
                unreadable
            );
            let unique = 2 * wrong + unreadable <= 3;
            assert_eq!(recovered.found == Found::Unique, unique);
        }
    }
    // The other backups give the same polynomial, so the same repairs (here t of them).
    let entries = damage(&liar, 2, 1, &mut rng);
    let recovered = offline::recover(&entries, None, &wallet.others(3)).unwrap();
    assert_eq!(recovered.key.as_ref().unwrap().1, KeySource::Backups);
    assert_eq!(recovered.found, Found::Repaired { wrong: 2 });
    assert_eq!(recovered.decoded.share, wallet.share(3));
}

// (d) Past repair.

#[test]
fn a_card_past_repair_fails_with_the_step_4_message() {
    let mut rng = ChaCha20Rng::from_seed([9; 32]);
    let wallet = Wallet::new(2, 3, &mut rng);
    let words = wallet.card(0);
    let four_wrong = damage(&words, 4, 0, &mut rng);
    assert_eq!(
        offline::recover(&four_wrong, Some(&wallet.key), &[]).unwrap_err(),
        Failure::Unrecoverable { had_key: true }
    );
    let too_many = damage(&words, 0, 4, &mut rng);
    assert_eq!(
        offline::recover(&too_many, Some(&wallet.key), &[]).unwrap_err(),
        Failure::TooManyUnreadable
    );
    // Another wallet's card reads on its own but isn't on this F.
    let other = Wallet::new(2, 3, &mut rng);
    assert_eq!(
        offline::recover(&entries(&other.card(0)), Some(&wallet.key), &[]).unwrap_err(),
        Failure::NotThisWallet {
            index: 1,
            self_masks: true
        }
    );
    assert_eq!(
        offline::recover(&entries(&other.card(0)), None, &wallet.others(0)).unwrap_err(),
        Failure::DoesntFitBackups { index: 1 }
    );

    let dir = TempDir::new("a_card_past_repair_fails_with_the_step_4_message");
    let poly = dir.file("poly_d.txt", &wallet.poly_text());
    let mut typed: [u16; WORDS] = words;
    for (j, entry) in four_wrong.iter().enumerate() {
        typed[j] = entry.unwrap();
    }
    let output = run_tool(&["--poly", poly.to_str().unwrap()], &text(&typed));
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(
        stderr(&output).contains(UNRECOVERABLE),
        "{}",
        stderr(&output)
    );
    // App review APP-6: with the words alone more input can still repair it, so the tool says
    // so and gives no verdict and no suspect line.
    let output = run_tool(&[], &text(&typed));
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let err = stderr(&output);
    assert!(err.contains("--poly") && err.contains("--backups"), "{err}");
    assert!(!err.contains(UNRECOVERABLE), "{err}");
    assert!(!err.contains("suspect"), "{err}");
    // Another wallet's card wasn't written wrong: its own message, without step 4's.
    let output = run_tool(&["--poly", poly.to_str().unwrap()], &text(&other.card(0)));
    assert_eq!(output.status.code(), Some(1));
    let err = stderr(&output);
    assert!(err.contains("isn't on the wallet"), "{err}");
    assert!(!err.contains(UNRECOVERABLE), "{err}");
    // More than 3 unreadable words, with no polynomial.
    let mut unreadable = text(&words)
        .split(' ')
        .map(String::from)
        .collect::<Vec<_>>();
    for j in [0, 10, 20, 30] {
        unreadable[j] = "?".into();
    }
    let output = run_tool(&[], &unreadable.join(" "));
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains(UNRECOVERABLE));
    assert_eq!(
        UNRECOVERABLE,
        "This card can't be recovered. It may have been written wrong. You can still restore \
         the wallet if you have enough other backups."
    );
}

// (e) Robust input.

#[test]
fn words_are_parsed_robustly() {
    let mut rng = ChaCha20Rng::from_seed([10; 32]);
    let wallet = Wallet::new(2, 3, &mut rng);
    let words = wallet.card(1);
    let names: Vec<&str> = words.iter().map(|&w| BIP39_WORDS[w as usize]).collect();
    let want = entries(&words);

    // Case, spacing, tabs, blank lines, commas.
    let messy: String = names
        .iter()
        .enumerate()
        .map(|(j, w)| {
            let w = if j % 2 == 0 {
                w.to_lowercase()
            } else {
                w.to_string()
            };
            match j % 4 {
                0 => format!("  {w}\t"),
                1 => format!("{w},"),
                2 => format!("{w}\n\n"),
                _ => format!("{w} ; "),
            }
        })
        .collect();
    assert_eq!(offline::parse_words(&messy).unwrap(), want);

    // Every numbering style, in two columns (1, 33, 2, 34, ...) as a card is copied.
    let styles = ["{n}.", "{n})", "{n}:", "{n} -", "0{n}", "{n}.{w}"];
    let mut numbered = String::new();
    for row in 0..HALF {
        for j in [row, row + HALF] {
            let style = styles[j % styles.len()];
            let n = (j + 1).to_string();
            let w = names[j].to_lowercase();
            if style.contains("{w}") {
                numbered += &style.replace("{n}", &n).replace("{w}", &w);
            } else {
                numbered += &format!("{} {w}", style.replace("{n}", &n));
            }
            numbered += "   ";
        }
        numbered += "\n";
    }
    assert_eq!(offline::parse_words(&numbered).unwrap(), want);

    // A unique start of at least 4 letters, and ? for an unreadable word.
    let mut short: Vec<String> = names.iter().map(|w| w.to_string()).collect();
    let long = short.iter().position(|w| w.len() > 4).unwrap();
    short[long].truncate(4);
    short[5] = "?".into();
    let mut want_short = want;
    want_short[5] = None;
    assert_eq!(offline::parse_words(&short.join(" ")).unwrap(), want_short);

    // Errors name the word number and never repeat what was typed.
    let mut bad = names.clone();
    bad[16] = "notaword";
    assert_eq!(
        offline::parse_words(&bad.join(" ")),
        Err(ParseError::NotAWord { position: 17 })
    );
    assert!(!ParseError::NotAWord { position: 17 }
        .to_string()
        .contains("notaword"));
    assert_eq!(
        offline::parse_words(&names[..63].join(" ")),
        Err(ParseError::WordCount(63))
    );
    assert_eq!(
        offline::parse_words(&format!("{} abandon", names.join(" "))),
        Err(ParseError::WordCount(65))
    );
    assert_eq!(
        offline::parse_words(&format!("1. {}", names.join(" "))),
        Err(ParseError::MixedNumbering)
    );
    let mut twice: Vec<String> = names
        .iter()
        .enumerate()
        .map(|(j, w)| format!("{}. {w}", j + 1))
        .collect();
    twice[1] = format!("1. {}", names[1]);
    assert_eq!(
        offline::parse_words(&twice.join(" ")),
        Err(ParseError::BadNumber(1))
    );
    twice[1] = format!("65. {}", names[1]);
    assert_eq!(
        offline::parse_words(&twice.join(" ")),
        Err(ParseError::BadNumber(65))
    );
    assert_eq!(
        offline::parse_words(&format!("{} 65.", names.join(" "))),
        Err(ParseError::NumberWithoutWord(65))
    );
    // "aban" is a unique start; "abs" (absent, absorb, ...) is too short to be one.
    assert_eq!(
        offline::parse_words(&format!("abs {}", names[1..].join(" "))),
        Err(ParseError::NotAWord { position: 1 })
    );
}

#[test]
fn the_cli_takes_words_from_a_file_or_stdin_but_never_argv() {
    let mut rng = ChaCha20Rng::from_seed([11; 32]);
    let wallet = Wallet::new(2, 3, &mut rng);
    let dir = TempDir::new("the_cli_takes_words_from_a_file_or_stdin_but_never_argv");
    let poly = dir.file("poly_e.txt", &wallet.poly_text());
    let numbered: String = text(&wallet.card(0))
        .split(' ')
        .enumerate()
        .map(|(j, w)| format!("{}. {}\n", j + 1, w.to_uppercase()))
        .collect();
    let words = dir.file("words_e.txt", &numbered);
    let from_file = run_tool(
        &[
            "--poly",
            poly.to_str().unwrap(),
            "--words",
            words.to_str().unwrap(),
        ],
        "",
    );
    let from_stdin = run_tool(
        &["--poly", poly.to_str().unwrap(), "--words", "-"],
        &numbered,
    );
    assert_eq!(from_file.status.code(), Some(0), "{}", stderr(&from_file));
    assert_eq!(from_file.stdout, from_stdin.stdout);
    assert_eq!(printed_share(&from_file), wallet.share(0));

    // Words on the command line are refused by the parser, not read.
    let output = run_tool(&["abandon"], "");
    assert_eq!(output.status.code(), Some(2));
    // Bad words, a bad polynomial: exit 2, with the reason.
    let output = run_tool(&["--poly", poly.to_str().unwrap()], "abandon ability");
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("expected 64 words, found 2"));
    let bad_poly = dir.file("bad_poly_e.txt", "02abcd");
    let output = run_tool(&["--poly", bad_poly.to_str().unwrap()], &numbered);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("coefficient 1"));
}

// Pre-PR review fixes (2 Nov).

/// Review OT-1 (b): three misread words on an honest card, chosen so the typed words are one
/// word from another codeword. Unique decoding "corrects" to that codeword, whose masks fail;
/// the masks-confirmed repair finds the card three words away, and wins.
#[test]
fn a_masks_confirmed_repair_beats_a_failed_mask_unique_decode() {
    let mut rng = ChaCha20Rng::seed_from_u64(9);
    let wallet = Wallet::new(2, 3, &mut rng);
    let share = wallet.share(1);
    let words = wallet.card(1);
    let pos = positions(4, &mut rng);
    let (a, b, c, x) = (pos[0], pos[1], pos[2], pos[3]);
    // Another codeword that agrees with the card except at a, b, c and x.
    let mut probe: Entries = words.map(Some);
    probe[a] = Some((words[a] + rng.gen_range(1..2048)) % 2048);
    probe[b] = None;
    probe[c] = None;
    probe[x] = None;
    let other = codec::decode_unique(&probe).unwrap();
    assert_ne!(other.word(x), words[x]);
    // Typed: the other codeword at a, b and c, the card's own word at x.
    let mut typed: Entries = words.map(Some);
    for j in [a, b, c] {
        typed[j] = Some(other.word(j));
    }
    assert_eq!(
        (0..WORDS).filter(|&j| typed[j] != Some(words[j])).count(),
        3
    );
    let unique = codec::decode_unique(&typed).expect("one word from the other codeword");
    assert_ne!(
        unique.coeffs,
        codec::decode_unique(&entries(&words)).unwrap().coeffs
    );

    let recovered = offline::recover(&typed, None, &[]).unwrap();
    assert_eq!(recovered.found, Found::Repaired { wrong: 3 });
    assert!(recovered.decoded.masks.self_ok);
    assert_eq!(recovered.decoded.share, share);
}

/// Review OT-1 (a): two unreadable words and two misread, chosen so the 62 readable words are
/// another codeword: past the code's reach. With no polynomial no repair passes the masks, so
/// the tool falls back to that unique decode (refusing it is the owner's call), and says the
/// masks failed and only a spare word checked it, never that the words alone decide the share.
/// With the polynomial or the other backups it's the step-4 failure, not another wallet's card.
#[test]
fn a_failed_mask_decode_past_the_reach_is_flagged_and_not_another_wallets_card() {
    let mut rng = ChaCha20Rng::seed_from_u64(7);
    let wallet = Wallet::new(2, 3, &mut rng);
    let share = wallet.share(0);
    let words = wallet.card(0);
    let pos = positions(4, &mut rng);
    let (e1, e2, a, b) = (pos[0], pos[1], pos[2], pos[3]);
    let mut typed: Entries = words.map(Some);
    typed[e1] = None;
    typed[e2] = None;
    typed[a] = Some((words[a] + rng.gen_range(1..2048)) % 2048);
    // The misread of word b that makes the readable words a codeword (1 in 2047 of them).
    let mut probe = typed;
    probe[b] = None;
    typed[b] = Some(codec::decode_unique(&probe).unwrap().word(b));
    assert_ne!(typed[b], Some(words[b]));

    let recovered = offline::recover(&typed, None, &[]).unwrap();
    assert_eq!(recovered.found, Found::Unique);
    assert!(!recovered.decoded.masks.self_ok);
    assert_ne!(
        recovered.decoded.share, share,
        "the residual the owner decides on"
    );
    assert_eq!(
        offline::recover(&typed, Some(&wallet.key), &[]).unwrap_err(),
        Failure::Unrecoverable { had_key: true }
    );
    assert_eq!(
        offline::recover(&typed, None, &wallet.others(0)).unwrap_err(),
        Failure::Unrecoverable { had_key: true }
    );

    let mut line: Vec<String> = typed
        .iter()
        .map(|entry| match entry {
            Some(word) => BIP39_WORDS[*word as usize].to_lowercase(),
            None => "?".into(),
        })
        .collect();
    let output = run_tool(&[], &line.join(" "));
    assert_eq!(output.status.code(), Some(0));
    let err = stderr(&output);
    assert!(
        err.contains("The masks failed, so one spare word at most checked"),
        "{err}"
    );
    assert!(!err.contains("the words alone decide"), "{err}");
    line.iter_mut().for_each(|word| word.clear());
}

/// Review OT-2: a misspelt word in the other backups is named by its line and number, never
/// echoed: it's usually one letter off a word of another share.
#[test]
fn a_misspelt_backup_word_isnt_echoed() {
    let mut rng = ChaCha20Rng::from_seed([11; 32]);
    let wallet = Wallet::new(2, 3, &mut rng);
    let dir = TempDir::new("a_misspelt_backup_word_isnt_echoed");
    let good = wallet.backups[1].to_string();
    let mut words: Vec<&str> = good.split_whitespace().collect();
    let misspelt = format!("{}X", words[3]);
    words[3] = &misspelt;
    let backups = dir.file(
        "bk.txt",
        &format!("{}\n{}\n", wallet.backups[2], words.join(" ")),
    );
    let output = run_tool(
        &["--backups", backups.to_str().unwrap()],
        &text(&wallet.card(0)),
    );
    assert_eq!(output.status.code(), Some(2));
    let err = stderr(&output);
    assert!(err.contains("line 2, word 3 isn't a BIP39 word"), "{err}");
    assert!(!err.contains(&misspelt), "{err}");
    assert!(
        !err.to_uppercase().contains(&misspelt.to_uppercase()),
        "{err}"
    );
}
