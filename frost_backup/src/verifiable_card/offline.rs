//! The offline recovery tool for the 64-word card (spec PK.8 escape hatch, owner B13 as amended
//! 2 Nov).
//!
//! On one trusted machine with all 64 words, the masks are information only: with no phone in
//! the loop the oracle that made the app's restore require them (owner B7) doesn't arise. The
//! trusted machine is where the restore ends, so a card is recovered as its share (index and
//! secret value), with what the masks said and whether it was checked against the wallet.
//!
//! The wallet's polynomial F and the user's other share backups are optional: each is an extra
//! check, and allows repair beyond unique decoding. Without either the share is still uniquely
//! determined by the words, or the tool says it isn't.
//!
//! What it accepts, by what checks the result:
//! - **Against F** (given, or found from the other backups alone): unique decoding, then a
//!   repair of up to 3 wrong or unreadable words, every candidate checked by s·G = F(i) at its
//!   own index, whatever its masks say. A wrong share passes with probability about 2^-256.
//! - **With no F:** unique decoding when the self-checking masks pass; otherwise a repair the
//!   self-checking masks confirm (so only an honest writer's card) comes first, since a spare
//!   word is an 11-bit check and the masks are 2^-256 (review OT-1): a few misread words can
//!   land on another codeword's unique decoding. Only when no repair passes the masks is a
//!   unique decode whose masks failed taken, and only if a spare word still checked it (at most
//!   one wrong word, or up to two unreadable words with none wrong): the share of a lying
//!   writer's card, checked by that one spare word alone. Anything else isn't uniquely
//!   determined by the words, and the tool says so.
//! - **With other backups and no F from them alone:** the card is read with no F as above, and
//!   then F is found from the other backups' images and the card's (the fingerprint), and the
//!   card's share must lie on it.

use super::codec::{
    self, read, read_ignoring_masks, wipe_decoded, Codeword, Decoded, Entries, OfflineCheck,
    RepairStats,
};
use super::layout::{COEFFS, PARITY, WORDS};
use crate::bip39_words::{words_with_prefix, BIP39_WORDS};
use crate::recovery::find_valid_subset;
use crate::ShareBackup;
use alloc::{string::String, vec::Vec};
use core::{ops::RangeInclusive, str::FromStr};
use schnorr_fun::{
    frost::{ShareImage, SharedKey},
    fun::prelude::*,
};

/// What [`recover`] prints when a card can't be recovered: B14's step 4.
pub const UNRECOVERABLE: &str = "This card can't be recovered. It may have been written wrong. \
You can still restore the wallet if you have enough other backups.";

/// Why the words couldn't be taken. Positions are word numbers, 1 to 64. No error repeats what
/// was typed, since it's a word of the secret.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// Word `position` isn't a BIP39 word, or a unique 4-letter start of one.
    NotAWord { position: usize },
    /// There weren't 64 words.
    WordCount(usize),
    /// Some words were numbered and others weren't.
    MixedNumbering,
    /// A number that isn't 1 to 64, or one used twice.
    BadNumber(usize),
    /// A number with no word after it.
    NumberWithoutWord(usize),
}

impl core::fmt::Display for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ParseError::NotAWord { position } => write!(
                f,
                "word {position} isn't a BIP39 word. Fix it, or type ? for a word you can't read."
            ),
            ParseError::WordCount(n) => write!(f, "expected 64 words, found {n}."),
            ParseError::MixedNumbering => {
                write!(
                    f,
                    "some words are numbered and some aren't. Number all of them or none."
                )
            }
            ParseError::BadNumber(n) => {
                write!(
                    f,
                    "word number {n} is used twice or isn't between 1 and 64."
                )
            }
            ParseError::NumberWithoutWord(n) => write!(f, "number {n} has no word after it."),
        }
    }
}

/// The 64 card words from text, case-insensitive. Words are separated by whitespace, commas or
/// semicolons; each may be numbered ("1", "1.", "1)", "1:", "1-", or "1.word" with no space),
/// and numbered words go where their numbers say, so a card copied column by column reads
/// right. A word is a BIP39 word or a unique start of one of at least 4 letters, and `?` marks a
/// word that can't be read.
pub fn parse_words(text: &str) -> Result<Entries, ParseError> {
    // (number, entry) for each word, in the order typed.
    let mut words: Vec<(Option<usize>, Option<u16>)> = Vec::with_capacity(WORDS);
    let mut pending: Option<usize> = None;
    for token in text.split(|c: char| c.is_whitespace() || c == ',' || c == ';') {
        let digits = token.bytes().take_while(u8::is_ascii_digit).count();
        let (number, rest) = token.split_at(digits);
        if !number.is_empty() {
            let n = number.parse::<usize>().unwrap_or(usize::MAX);
            if let Some(previous) = pending.replace(n) {
                return Err(ParseError::NumberWithoutWord(previous));
            }
        }
        let rest = rest.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '?');
        if rest.is_empty() {
            continue;
        }
        let position = pending.unwrap_or(words.len() + 1);
        let entry = parse_word(rest).ok_or(ParseError::NotAWord { position })?;
        words.push((pending.take(), entry));
    }
    if let Some(n) = pending {
        return Err(ParseError::NumberWithoutWord(n));
    }
    if words.len() != WORDS {
        return Err(ParseError::WordCount(words.len()));
    }
    let mut entries: Entries = [None; WORDS];
    let numbered = words.iter().filter(|(number, _)| number.is_some()).count();
    if numbered == 0 {
        for (entry, (_, word)) in entries.iter_mut().zip(&words) {
            *entry = *word;
        }
    } else if numbered == WORDS {
        let mut seen = [false; WORDS];
        for &(number, word) in &words {
            let n = number.expect("all numbered");
            match n.checked_sub(1).filter(|&j| j < WORDS && !seen[j]) {
                Some(j) => {
                    seen[j] = true;
                    entries[j] = word;
                }
                None => return Err(ParseError::BadNumber(n)),
            }
        }
    } else {
        return Err(ParseError::MixedNumbering);
    }
    words
        .iter_mut()
        .for_each(|word| codec::wipe(word, (None, None)));
    Ok(entries)
}

/// A word's index, `Some(None)` for `?`, or `None` if it isn't a word.
fn parse_word(token: &str) -> Option<Option<u16>> {
    if !token.is_empty() && token.bytes().all(|b| b == b'?') {
        return Some(None);
    }
    // The longest BIP39 word has 8 letters.
    let mut upper = [0u8; 8];
    if token.len() > upper.len() || !token.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    for (u, b) in upper.iter_mut().zip(token.bytes()) {
        *u = b.to_ascii_uppercase();
    }
    let word = core::str::from_utf8(&upper[..token.len()]).expect("ASCII");
    let index = match BIP39_WORDS.binary_search(&word) {
        Ok(index) => Some(index),
        Err(_) if word.len() >= 4 => match words_with_prefix(word) {
            [only] => BIP39_WORDS.binary_search(only).ok(),
            _ => None,
        },
        Err(_) => None,
    };
    codec::wipe(&mut upper, [0; 8]);
    index.map(|i| Some(i as u16))
}

/// Why a polynomial couldn't be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PolyError {
    /// Coefficient `n` (from 1) isn't a 33-byte point in hex.
    NotAPoint(usize),
    /// No coefficients, or a zero public key.
    Empty,
}

impl core::fmt::Display for PolyError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PolyError::NotAPoint(n) => {
                write!(
                    f,
                    "coefficient {n} isn't a compressed point (66 hex characters)."
                )
            }
            PolyError::Empty => write!(f, "no polynomial, or its public key is zero."),
        }
    }
}

/// The wallet polynomial F from text: its coefficients as compressed points in hex (66
/// characters each), the public key first, separated by whitespace or commas. These are
/// [`SharedKey::point_polynomial`] in order.
pub fn parse_poly(text: &str) -> Result<SharedKey, PolyError> {
    let points = text
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|token| !token.is_empty())
        .enumerate()
        .map(|(n, token)| {
            Point::<Normal, Public, Zero>::from_str(token).map_err(|_| PolyError::NotAPoint(n + 1))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if points.is_empty() {
        return Err(PolyError::Empty);
    }
    SharedKey::from_poly(points)
        .non_zero()
        .ok_or(PolyError::Empty)
}

/// Where the polynomial a recovered share was checked against came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeySource {
    /// The polynomial the user gave.
    Given,
    /// Found from the other backups alone (the fingerprint search), then the card checked
    /// against it by s·G.
    Backups,
    /// Found from the other backups together with this card's share image. Only the
    /// fingerprint (18 bits for each coefficient past the first, up to 36) confirms it.
    BackupsWithCard,
}

/// How the card's codeword was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Found {
    /// Unique decoding: every readable word fits, or one wrong word was corrected.
    Unique,
    /// A repair past unique decoding, trying `wrong` words as wrong.
    Repaired { wrong: usize },
}

/// A recovered card. It wipes the share when it's dropped.
pub struct Recovered {
    pub decoded: Decoded,
    pub found: Found,
    /// The polynomial the share was checked against, when there is one.
    pub key: Option<(SharedKey, KeySource)>,
}

impl Drop for Recovered {
    fn drop(&mut self) {
        wipe_decoded(&mut self.decoded);
    }
}

impl core::fmt::Debug for Recovered {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Recovered")
            .field("decoded", &self.decoded)
            .field("found", &self.found)
            .field("key_source", &self.key.as_ref().map(|(_, source)| source))
            .finish()
    }
}

/// Why a card couldn't be recovered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    /// More than 3 words are marked unreadable.
    TooManyUnreadable,
    /// No share fits the words within the code's reach (and the polynomial, when there is one).
    /// `had_key`: the wallet's polynomial or other backups were given, so nothing more can help.
    Unrecoverable { had_key: bool },
    /// More than one share fits the words.
    Ambiguous,
    /// The words read as a card on their own, but its share isn't on the polynomial given.
    NotThisWallet { index: u8, self_masks: bool },
    /// The words read as a card on their own, but its share doesn't fit the other backups.
    DoesntFitBackups { index: u8 },
}

/// The numbers of wrong words a repair tries past unique decoding when `unreadable` words
/// couldn't be read: up to what the 3 spare words allow, with no cap for time.
pub fn repair_levels(unreadable: usize) -> RangeInclusive<usize> {
    let room = PARITY.saturating_sub(unreadable);
    room / 2 + 1..=room
}

/// Recovers a card from all 64 words, ignoring its masks (spec PK.8 escape hatch). `key` is
/// the wallet polynomial if the user has it; `backups` are the user's other share backups of
/// the same wallet. See the module docs for what each can repair.
pub fn recover(
    entries: &Entries,
    key: Option<&SharedKey>,
    backups: &[ShareBackup],
) -> Result<Recovered, Failure> {
    let unreadable = codec::erasures(entries).count_ones() as usize;
    if unreadable > PARITY {
        return Err(Failure::TooManyUnreadable);
    }
    let mut stats = RepairStats::default();
    let unique = WipedCodeword(codec::decode_unique(entries).ok());
    let unique = unique.0.as_ref();
    let images: Vec<ShareImage> = backups.iter().map(ShareBackup::share_image).collect();

    let mut keys: Vec<(SharedKey, KeySource)> = Vec::new();
    if let Some(key) = key {
        keys.push((key.clone(), KeySource::Given));
    }
    if key.is_none() {
        if let Some(found) = find_valid_subset(&images, crate::FINGERPRINT, None)
            .and_then(|(_, found)| found.non_zero())
        {
            keys.push((found, KeySource::Backups));
        }
    }
    for (key, source) in keys {
        let poly = key.point_polynomial();
        if let Some(decoded) =
            unique.and_then(|codeword| read_ignoring_masks(codeword, Some(poly)).ok())
        {
            return Ok(Recovered {
                decoded,
                found: Found::Unique,
                key: Some((key, source)),
            });
        }
        for wrong in repair_levels(unreadable) {
            let hits = search(entries, wrong, OfflineCheck::Polynomial(poly), &mut stats)?;
            if let Some(decoded) = hits {
                return Ok(Recovered {
                    decoded,
                    found: Found::Repaired { wrong },
                    key: Some((key, source)),
                });
            }
        }
    }

    // No polynomial has the share: read the words on their own.
    let had_key = key.is_some() || !backups.is_empty();
    let unreachable = Failure::Unrecoverable { had_key };
    let mut cold = unique.and_then(|codeword| read(codeword, None, None).ok());
    let (mut decoded, found) = match cold.take() {
        Some(decoded) if decoded.masks.self_ok => (decoded, Found::Unique),
        mut unique_cold => {
            // Review OT-1: a repair the self-checking masks confirm beats a unique decode whose
            // masks failed, which at most one spare word (11 bits) checked.
            let mut repaired = None;
            for wrong in repair_levels(unreadable) {
                match search(entries, wrong, OfflineCheck::SelfMasks, &mut stats) {
                    Ok(Some(decoded)) => {
                        repaired = Some((decoded, Found::Repaired { wrong }));
                        break;
                    }
                    Ok(None) => {}
                    Err(failure) => {
                        if let Some(decoded) = unique_cold.as_mut() {
                            wipe_decoded(decoded);
                        }
                        return Err(failure);
                    }
                }
            }
            match (repaired, unique_cold) {
                (Some(repaired), mut unique_cold) => {
                    if let Some(decoded) = unique_cold.as_mut() {
                        wipe_decoded(decoded);
                    }
                    repaired
                }
                (None, Some(decoded)) if !decoded.codeword.no_spare_check => {
                    (decoded, Found::Unique)
                }
                (None, mut unique_cold) => {
                    if let Some(decoded) = unique_cold.as_mut() {
                        wipe_decoded(decoded);
                    }
                    return Err(unreachable);
                }
            }
        }
    };
    // A card whose masks failed and that needed words corrected or filled in, and still isn't
    // on the wallet: most likely words misread past repair, not another wallet's card (review
    // OT-1). Only a card whose words all fit, or whose masks pass, reads as a card of its own.
    let read_as_a_card = decoded.masks.self_ok
        || (found == Found::Unique
            && decoded.codeword.corrected == 0
            && decoded.codeword.erased == 0);
    if key.is_some() {
        let failure = if read_as_a_card {
            Failure::NotThisWallet {
                index: decoded.index,
                self_masks: decoded.masks.self_ok,
            }
        } else {
            unreachable
        };
        wipe_decoded(&mut decoded);
        return Err(failure);
    }
    if backups.is_empty() {
        return Ok(Recovered {
            decoded,
            found,
            key: None,
        });
    }
    let image = decoded.share.share_image();
    let mut with_card = images;
    with_card.push(image);
    match find_valid_subset(&with_card, crate::FINGERPRINT, None)
        .filter(|(compatible, _)| compatible.contains(&image))
        .and_then(|(_, key)| key.non_zero())
    {
        Some(key) => {
            // Read again against F for the wallet-bound masks and the canonical check. The
            // share is on F, so this can't fail; if it did, keep the cold read.
            if let Ok(with_key) =
                read_ignoring_masks(&decoded.codeword, Some(key.point_polynomial()))
            {
                wipe_decoded(&mut decoded);
                decoded = with_key;
            }
            Ok(Recovered {
                decoded,
                found,
                key: Some((key, KeySource::BackupsWithCard)),
            })
        }
        None => {
            let failure = if read_as_a_card {
                Failure::DoesntFitBackups {
                    index: decoded.index,
                }
            } else {
                unreachable
            };
            wipe_decoded(&mut decoded);
            Err(failure)
        }
    }
}

/// A codeword that's wiped when it's dropped.
struct WipedCodeword(Option<Codeword>);

impl Drop for WipedCodeword {
    fn drop(&mut self) {
        if let Some(codeword) = &mut self.0 {
            codec::wipe(&mut codeword.coeffs, [0; COEFFS]);
        }
    }
}

/// The one share a repair level finds, `None` if it finds none, and [`Failure::Ambiguous`] if
/// it finds more than one.
fn search(
    entries: &Entries,
    wrong: usize,
    check: OfflineCheck,
    stats: &mut RepairStats,
) -> Result<Option<Decoded>, Failure> {
    // The levels keep within the spare words, and parsed entries are always words, so the
    // search refuses nothing; if it did, this level has nothing to offer.
    let Ok(mut hits) = codec::offline_repair_search(entries, wrong, check, stats) else {
        return Ok(None);
    };
    if hits.len() > 1 {
        hits.iter_mut().for_each(wipe_decoded);
        return Err(Failure::Ambiguous);
    }
    Ok(hits.pop())
}

/// Overwrites the entries.
pub fn wipe_entries(entries: &mut Entries) {
    codec::wipe(entries, [None; WORDS]);
}

/// Overwrites text that held words or a share, and frees it.
pub fn wipe_text(text: String) {
    let mut bytes = text.into_bytes();
    bytes.iter_mut().for_each(|b| *b = 0);
    core::hint::black_box(&mut bytes);
}
