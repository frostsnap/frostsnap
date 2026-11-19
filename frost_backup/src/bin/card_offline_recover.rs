//! Offline recovery for the 64-word Frostsnap card (spec PK.8 escape hatch, owner B13 as amended
//! 2 Nov).
//!
//! Takes all 64 card words on one trusted machine, reads the card with its masks as information
//! only, and prints the recovered share (its index and secret value), what the masks said and
//! whether it was checked against the wallet. The trusted machine is where the restore ends.
//! Words are read from a file or stdin, never from the command line, which shells keep in their
//! history and `ps` shows to other users.

use clap::Parser;
use frost_backup::verifiable_card::{
    layout::WORDS,
    offline::{self, Failure, Found, KeySource, Recovered, UNRECOVERABLE},
};
use frost_backup::{ShareBackup, ShareBackupError};
use std::io::{IsTerminal, Read, Write};
use std::process::ExitCode;
use std::str::FromStr;

/// The share was recovered and printed.
const EXIT_RECOVERED: u8 = 0;
/// The card can't be recovered.
const EXIT_UNRECOVERABLE: u8 = 1;
/// The input couldn't be taken (words, polynomial, backups or files).
const EXIT_INPUT: u8 = 2;

#[derive(Parser)]
#[command(name = "card_offline_recover")]
#[command(about = "Recover the key share on a 64-word Frostsnap card, offline")]
#[command(
    long_about = "Recover the key share on a 64-word Frostsnap card, offline.\n\n\
Reads the 64 words from stdin (or --words FILE), one card, numbered or not. Type ? for a \
word you can't read. The card's masks are reported but don't stop the recovery.\n\n\
Prints the share on stdout (its index, and its secret value in hex) and the report on \
stderr. The wallet's polynomial (--poly) or your other share backups (--backups) are \
optional: each checks the share against the wallet, and allows repairing more wrong words.\n\n\
Exit codes: 0 share recovered; 1 card can't be recovered; 2 bad input."
)]
struct Cli {
    /// File with the 64 card words. Reads stdin if omitted or "-".
    #[arg(long, value_name = "FILE")]
    words: Option<String>,

    /// Optional file with the wallet's polynomial: its coefficients as compressed points in hex,
    /// the public key first. Checks the share and allows repairing up to 3 wrong words.
    #[arg(long, value_name = "FILE")]
    poly: Option<String>,

    /// Optional file with your other share backups of this wallet, one per line ("#2 WORD
    /// ..."). The wallet's polynomial is found from them to check the share.
    #[arg(long, value_name = "FILE", conflicts_with = "poly")]
    backups: Option<String>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    eprintln!("Frostsnap offline card recovery");
    eprintln!("Use this only on a computer you trust, offline. It puts your whole key share on");
    eprintln!("this machine, and anyone who sees the words or the share it prints can use it.");
    eprintln!("Use only Frostsnap's own tool. No one from Frostsnap will ask for your words.");
    eprintln!();
    ExitCode::from(run(&cli))
}

fn run(cli: &Cli) -> u8 {
    let key = match &cli.poly {
        Some(path) => match read_file(path).map(|text| offline::parse_poly(&text)) {
            Ok(Ok(key)) => Some(key),
            Ok(Err(e)) => return input_error(&format!("The polynomial in {path}: {e}")),
            Err(e) => return input_error(&e),
        },
        None => None,
    };
    let mut backups = match &cli.backups {
        Some(path) => match read_file(path).map(|text| {
            let backups = parse_backups(&text);
            offline::wipe_text(text);
            backups
        }) {
            Ok(Ok(backups)) => backups,
            Ok(Err(e)) => return input_error(&format!("The backups in {path}: {e}")),
            Err(e) => return input_error(&e),
        },
        None => Vec::new(),
    };
    let text = match read_words(cli.words.as_deref()) {
        Ok(text) => text,
        Err(e) => return input_error(&e),
    };
    let parsed = offline::parse_words(&text);
    offline::wipe_text(text);
    let mut entries = match parsed {
        Ok(entries) => entries,
        Err(e) => return input_error(&format!("The card words: {e}")),
    };
    eprintln!("Reading the card. If many words are wrong, the search can take a minute.");
    let result = offline::recover(&entries, key.as_ref(), &backups);
    offline::wipe_entries(&mut entries);
    backups.iter_mut().for_each(ShareBackup::wipe);
    match result {
        Ok(recovered) => report(&recovered),
        Err(failure) => {
            // A card that reads but belongs elsewhere wasn't written wrong (as B11's
            // `WrongWallet` in the app), so it gets its own message alone.
            if explain(failure) {
                eprintln!();
                eprintln!("{UNRECOVERABLE}");
                eprintln!("Treat the Frostsnap device that wrote it as suspect.");
            }
            EXIT_UNRECOVERABLE
        }
    }
}

fn input_error(message: &str) -> u8 {
    eprintln!("Error: {message}");
    EXIT_INPUT
}

fn read_file(path: &str) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("Couldn't read {path}: {e}"))
}

fn read_words(path: Option<&str>) -> Result<String, String> {
    match path {
        Some(path) if path != "-" => read_file(path),
        _ => {
            let stdin = std::io::stdin();
            if stdin.is_terminal() {
                eprintln!("Type or paste the {WORDS} card words, then press Ctrl-D on a new line.");
            }
            // Sized for any card, so the buffer never grows: a grown buffer would free a copy of
            // the words it held, unwiped (review OT-4).
            let mut text = String::with_capacity(8192);
            stdin
                .lock()
                .read_to_string(&mut text)
                .map_err(|e| format!("Couldn't read the words from stdin: {e}"))?;
            Ok(text)
        }
    }
}

/// The other share backups, one per line. An error names the line and the word's number, never
/// the word: a misspelt word is usually one letter off a word of another share (review OT-2).
/// The uppercased copy of each line is wiped.
fn parse_backups(text: &str) -> Result<Vec<ShareBackup>, String> {
    let mut backups = Vec::new();
    for (n, line) in text.lines().map(str::trim).enumerate() {
        if line.is_empty() {
            continue;
        }
        let upper = line.to_uppercase();
        let parsed = ShareBackup::from_str(&upper);
        offline::wipe_text(upper);
        match parsed {
            Ok(backup) => backups.push(backup),
            Err(e) => {
                backups.iter_mut().for_each(ShareBackup::wipe);
                return Err(match e {
                    ShareBackupError::InvalidBip39Word { word_index, word } => {
                        offline::wipe_text(word);
                        format!("line {}, word {} isn't a BIP39 word", n + 1, word_index + 1)
                    }
                    e => format!("line {}: {e}", n + 1),
                });
            }
        }
    }
    Ok(backups)
}

/// Word numbers in a mask, bit j for word j + 1.
fn word_numbers(mask: u64) -> String {
    (0..WORDS)
        .filter(|j| (mask >> j) & 1 == 1)
        .map(|j| (j + 1).to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn pass(ok: bool) -> &'static str {
    if ok {
        "passed"
    } else {
        "FAILED"
    }
}

fn report(recovered: &Recovered) -> u8 {
    let decoded = &recovered.decoded;
    let codeword = &decoded.codeword;
    let key = recovered.key.as_ref().map(|(key, _)| key);
    eprintln!("Card read.");
    eprintln!("  Share index: #{}", decoded.index);
    let wallet_threshold = key.map(|key| key.threshold());
    let threshold = match decoded.threshold {
        Some(0) => "not recorded".to_string(),
        Some(t) => t.to_string(),
        None => "doesn't read (the device that wrote the card may be faulty)".to_string(),
    };
    match wallet_threshold {
        Some(w) => eprintln!("  Threshold on the card: {threshold} (the wallet's: {w})"),
        None => eprintln!("  Threshold on the card: {threshold}"),
    }
    match recovered.found {
        Found::Unique if codeword.corrected == 0 => eprintln!("  Every word fits."),
        Found::Unique => eprintln!("  Corrected word: {}", word_numbers(codeword.corrected)),
        Found::Repaired { .. } => {
            eprintln!("  Repaired words: {}", word_numbers(codeword.corrected))
        }
    }
    if codeword.erased != 0 {
        eprintln!("  Unreadable words: {}", word_numbers(codeword.erased));
    }
    eprintln!("  Masks (information only; this tool doesn't need them):");
    eprintln!("    self-checking masks: {}", pass(decoded.masks.self_ok));
    match decoded.masks.wallet {
        Some(ok) => eprintln!("    wallet-bound masks: {}", pass(ok)),
        None => eprintln!("    wallet-bound masks: not checked (needs the wallet's polynomial)"),
    }
    if let Some(canonical) = decoded.canonical {
        eprintln!(
            "    written as an honest device writes it: {}",
            if canonical.is_canonical() {
                "yes"
            } else {
                "no"
            }
        );
    }
    if !decoded.masks.passed() {
        eprintln!("    The masks failed, so this card doesn't restore through the app.");
        eprintln!("    Treat the Frostsnap device that wrote it as suspect.");
    }
    if decoded.reduced {
        eprintln!("  The card's scalar was written out of range and was reduced.");
    }
    let checked = match recovered.key.as_ref().map(|(_, source)| source) {
        Some(KeySource::Given) => "yes, the share is on the wallet polynomial you gave",
        Some(KeySource::Backups) => "yes, the share is on the polynomial of your other backups",
        Some(KeySource::BackupsWithCard) => {
            "yes, the share fits your other backups (by the wallet fingerprint)"
        }
        None => match recovered.found {
            Found::Repaired { .. } => {
                "no (no wallet polynomial or other backups given); the self-checking masks \
                 confirmed the repair"
            }
            Found::Unique if decoded.masks.self_ok => {
                "no (no wallet polynomial or other backups given); the words alone decide \
                 the share"
            }
            // Review OT-1: no repair passed the masks, so this share was checked by one spare
            // word at most, an 11-bit check. Whether to refuse it is the owner's call.
            Found::Unique => {
                "no (no wallet polynomial or other backups given). The masks failed, so one \
                 spare word at most checked this share: a few misread words can fit another \
                 share. Check every word, and check the share against your other backups or \
                 the wallet before you rely on it"
            }
        },
    };
    eprintln!("  Checked against the wallet: {checked}");

    let mut secret = decoded.share.share.to_bytes();
    // Sized up front so the string never grows and frees a partial copy (review OT-4).
    let mut text = String::with_capacity(96);
    text.push_str("index: ");
    text.push_str(&decoded.index.to_string());
    text.push_str("\nsecret: ");
    for &byte in &secret {
        text.push(char::from(b"0123456789abcdef"[usize::from(byte >> 4)]));
        text.push(char::from(b"0123456789abcdef"[usize::from(byte & 0xf)]));
    }
    text.push('\n');
    secret.iter_mut().for_each(|b| *b = 0);
    core::hint::black_box(&mut secret);
    let mut stdout = std::io::stdout();
    let written = stdout
        .write_all(text.as_bytes())
        .and_then(|()| stdout.flush())
        .map_err(|e| format!("Couldn't print the share: {e}"));
    offline::wipe_text(text);
    if let Err(e) = written {
        return input_error(&e);
    }
    eprintln!();
    eprintln!(
        "Share #{} recovered: its index and secret value are printed above.",
        decoded.index
    );
    eprintln!("Anyone who sees the secret value can use it. Keep it on this machine only.");
    EXIT_RECOVERED
}

/// Explains a failure, and says whether the card is past repair (B14's step 4).
fn explain(failure: Failure) -> bool {
    match failure {
        Failure::TooManyUnreadable => {
            eprintln!("More than 3 words are marked unreadable. The card can rebuild at most 3.");
            true
        }
        Failure::Unrecoverable { had_key: true } => {
            eprintln!("No share fits these words with the wallet's polynomial or your other");
            eprintln!("backups, even with up to 3 words wrong or unreadable. Check every word");
            eprintln!("against the card.");
            true
        }
        // Not past repair yet: more input can repair more (app review APP-6), so the hint
        // stands alone, with no verdict and no suspect line.
        Failure::Unrecoverable { had_key: false } => {
            eprintln!("These words don't read as a card. On their own the words decide the share");
            eprintln!("only with at most one wrong word (or a card whose masks pass). With the");
            eprintln!("wallet's polynomial (--poly) or your other backups (--backups) up to 3");
            eprintln!("wrong words can be repaired. Check every word against the card, then try");
            eprintln!("again with one of them.");
            false
        }
        Failure::Ambiguous => {
            eprintln!("More than one share fits these words, so the card can't be decided.");
            true
        }
        Failure::NotThisWallet { index, self_masks } => {
            eprintln!("The words read as card #{index}, but its share isn't on the wallet");
            eprintln!("polynomial you gave. Check that it's this wallet's card and polynomial.");
            if !self_masks {
                eprintln!("Its self-checking masks also failed: some words may be wrong.");
            }
            false
        }
        Failure::DoesntFitBackups { index } => {
            eprintln!("The words read as card #{index}, but its share doesn't fit your other");
            eprintln!("backups. Check that they're all this wallet's, and every word of the card.");
            false
        }
    }
}
