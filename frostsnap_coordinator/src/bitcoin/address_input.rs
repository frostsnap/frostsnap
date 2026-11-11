use bdk_chain::bitcoin::{address::NetworkUnchecked, base58, Address, AddressType, Network};
use bech32::{
    primitives::decode::{ChecksumError, UncheckedHrpstring},
    Bech32, Bech32m, Checksum, CorrectableError, Fe32,
};

const BECH32_CHARSET: &str = "qpzry9x8gf2tvdw0s3jn54khce6mua7l";
const BASE58_ALPHABET: &str = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
const URI_SCHEME: &str = "bitcoin:";

/// Text entered as "an address", judged against one wallet's network.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddressInput {
    pub status: AddressInputStatus,
    /// The characters known to be wrong, as UTF-16 offsets into the entered text because that is
    /// how the app's text field counts.
    pub bad_chars: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AddressInputStatus {
    Empty,
    /// A taproot address on the wallet's network: one the wallet could have derived.
    Valid(Address),
    /// A valid address on the wallet's network that no Frostsnap wallet derives, since they only
    /// use taproot.
    NotTaproot,
    WrongNetwork {
        address_network: AddressNetwork,
    },
    InvalidCharacters,
    MixedCase,
    /// Too short or too long to be an address of its kind.
    WrongLength,
    BadChecksum,
    NotAnAddress,
}

/// The networks as addresses tell them apart: testnet3, testnet4 and signet share an encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddressNetwork {
    Mainnet,
    Testnet,
    Regtest,
}

impl AddressNetwork {
    fn of_address(address: &Address<NetworkUnchecked>) -> Self {
        if address.is_valid_for_network(Network::Bitcoin) {
            AddressNetwork::Mainnet
        } else if address.is_valid_for_network(Network::Testnet) {
            AddressNetwork::Testnet
        } else {
            AddressNetwork::Regtest
        }
    }
}

impl AddressInput {
    /// Accepts a bare address or a `bitcoin:` URI, with surrounding whitespace.
    pub fn parse(text: &str, network: Network) -> Self {
        let trimmed = text.trim();
        let parsed = match trimmed.parse::<bip21::Uri<NetworkUnchecked>>() {
            Ok(uri) => Some(uri.address),
            Err(_) => trimmed.parse::<Address<NetworkUnchecked>>().ok(),
        };
        if let Some(address) = parsed {
            return AddressInput {
                status: judge(address, network),
                bad_chars: vec![],
            };
        }
        // bip21 rejects a URI whose address is invalid, so finding the bad characters needs the
        // address text dug out by hand.
        let (offset, candidate) = strip_uri(text);
        let (status, bad_bytes) = classify(candidate);
        let bad_chars = bad_bytes
            .into_iter()
            .map(|byte| text[..offset + byte].encode_utf16().count() as u32)
            .collect();
        AddressInput { status, bad_chars }
    }
}

/// Returns the address part of `text` and its byte offset within `text`.
fn strip_uri(text: &str) -> (usize, &str) {
    let trimmed = text.trim_start();
    let mut offset = text.len() - trimmed.len();
    let mut rest = trimmed;
    if rest
        .get(..URI_SCHEME.len())
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case(URI_SCHEME))
    {
        offset += URI_SCHEME.len();
        rest = &rest[URI_SCHEME.len()..];
    }
    let end = rest.find('?').unwrap_or(rest.len());
    (offset, rest[..end].trim_end())
}

/// Returns the status and the byte offsets of bad characters within `candidate`.
fn classify(candidate: &str) -> (AddressInputStatus, Vec<usize>) {
    if candidate.is_empty() {
        return (AddressInputStatus::Empty, vec![]);
    }
    let lower = candidate.to_ascii_lowercase();
    if let Some(hrp) = ["bc", "tb", "bcrt"]
        .into_iter()
        .find(|hrp| lower.starts_with(&format!("{hrp}1")))
    {
        diagnose_bech32(candidate, hrp)
    } else if candidate.starts_with(['1', '3', 'm', 'n', '2']) {
        diagnose_base58(candidate)
    } else {
        (AddressInputStatus::NotAnAddress, vec![])
    }
}

fn judge(address: Address<NetworkUnchecked>, network: Network) -> AddressInputStatus {
    if !address.is_valid_for_network(network) {
        return AddressInputStatus::WrongNetwork {
            address_network: AddressNetwork::of_address(&address),
        };
    }
    let address = address.assume_checked();
    if address.address_type() == Some(AddressType::P2tr) {
        AddressInputStatus::Valid(address)
    } else {
        AddressInputStatus::NotTaproot
    }
}

fn diagnose_bech32(candidate: &str, hrp: &str) -> (AddressInputStatus, Vec<usize>) {
    let data_start = hrp.len() + 1;
    let invalid: Vec<usize> = candidate[data_start..]
        .char_indices()
        .filter(|(_, c)| !BECH32_CHARSET.contains(c.to_ascii_lowercase()))
        .map(|(i, _)| data_start + i)
        .collect();
    if !invalid.is_empty() {
        return (AddressInputStatus::InvalidCharacters, invalid);
    }

    let minority_case = minority_case(candidate);
    if !minority_case.is_empty() {
        return (AddressInputStatus::MixedCase, minority_case);
    }

    let Ok(unchecked) = UncheckedHrpstring::new(candidate) else {
        return (AddressInputStatus::NotAnAddress, vec![]);
    };
    let data_len = candidate.len() - data_start;
    match unchecked.witness_version() {
        // A 20 or 32 byte program.
        Some(Fe32::Q) if [39, 59].contains(&data_len) => {
            locate_checksum_errors::<Bech32>(&unchecked, candidate.len(), data_start)
        }
        // Taproot's 32 byte program.
        Some(version) if version != Fe32::Q && data_len == 59 => {
            locate_checksum_errors::<Bech32m>(&unchecked, candidate.len(), data_start)
        }
        Some(_) => (AddressInputStatus::WrongLength, vec![]),
        None => (AddressInputStatus::NotAnAddress, vec![]),
    }
}

fn locate_checksum_errors<Ck: Checksum>(
    unchecked: &UncheckedHrpstring,
    len: usize,
    data_start: usize,
) -> (AddressInputStatus, Vec<usize>) {
    let residue_error = match unchecked.validate_checksum::<Ck>() {
        // The checksum holds, so rust-bitcoin rejected what it encodes.
        Ok(()) => return (AddressInputStatus::NotAnAddress, vec![]),
        Err(error @ ChecksumError::InvalidResidue(_)) => error,
        Err(_) => return (AddressInputStatus::BadChecksum, vec![]),
    };
    // bech32 can only locate a single substitution, and enough typos can look like a different
    // single one, so the position is shown as a hint and never used to fix the address.
    let position = |from_end: usize| len.checked_sub(from_end + 1).filter(|&i| i >= data_start);
    let corrector = residue_error.correction_context::<Ck>();
    let mut located: Vec<usize> = corrector
        .as_ref()
        .and_then(|corrector| corrector.bch_errors())
        .and_then(|errors| errors.map(|(from_end, _)| position(from_end)).collect())
        .unwrap_or_default();
    located.sort_unstable();
    (AddressInputStatus::BadChecksum, located)
}

/// The letters in whichever case is rarer, or in upper case on a tie; empty if there is one case.
fn minority_case(candidate: &str) -> Vec<usize> {
    let letters_where = |is_case: fn(&char) -> bool| -> Vec<usize> {
        candidate
            .char_indices()
            .filter(|(_, c)| is_case(c))
            .map(|(i, _)| i)
            .collect()
    };
    let upper = letters_where(char::is_ascii_uppercase);
    let lower = letters_where(char::is_ascii_lowercase);
    if upper.is_empty() || lower.is_empty() {
        vec![]
    } else if upper.len() <= lower.len() {
        upper
    } else {
        lower
    }
}

fn diagnose_base58(candidate: &str) -> (AddressInputStatus, Vec<usize>) {
    let invalid: Vec<usize> = candidate
        .char_indices()
        .filter(|(_, c)| !BASE58_ALPHABET.contains(*c))
        .map(|(i, _)| i)
        .collect();
    if !invalid.is_empty() {
        return (AddressInputStatus::InvalidCharacters, invalid);
    }
    // A 25 byte payload encodes to 26 to 35 characters.
    if !(26..=35).contains(&candidate.len()) {
        return (AddressInputStatus::WrongLength, vec![]);
    }
    match base58::decode_check(candidate) {
        Err(base58::Error::IncorrectChecksum(_)) => (AddressInputStatus::BadChecksum, vec![]),
        _ => (AddressInputStatus::NotAnAddress, vec![]),
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use AddressInputStatus::*;

    const MAINNET_TAPROOT: &str = "bc1p5cyxnuxmeuwuvkwfem96lqzszd02n6xdcjrs20cac6yqjjwudpxqkedrcr";
    const TESTNET_TAPROOT: &str = "tb1pqqqqp399et2xygdj5xreqhjjvcmzhxw4aywxecjdzew6hylgvsesf3hn0c";
    const MAINNET_SEGWIT_V0: &str = "bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq";
    const MAINNET_P2PKH: &str = "1BvBMSEYstWetqTFn5Au4m4GFg7xJaNVN2";

    fn parse(text: &str) -> AddressInput {
        AddressInput::parse(text, Network::Bitcoin)
    }

    fn with_char_at(address: &str, index: usize, c: char) -> String {
        let mut chars: Vec<char> = address.chars().collect();
        chars[index] = c;
        chars.into_iter().collect()
    }

    fn a_different_bech32_char(c: char) -> char {
        BECH32_CHARSET.chars().find(|&other| other != c).unwrap()
    }

    #[test]
    fn empty_and_whitespace_are_empty() {
        assert_eq!(parse("").status, Empty);
        assert_eq!(parse("  \n ").status, Empty);
        assert_eq!(parse("bitcoin:").status, Empty);
    }

    #[test]
    fn surrounding_whitespace_and_uri_are_stripped() {
        for text in [
            format!("  {MAINNET_TAPROOT}\n"),
            format!("bitcoin:{MAINNET_TAPROOT}"),
            format!("bitcoin:{MAINNET_TAPROOT}?amount=0.1&label=x"),
            format!("BITCOIN:{}", MAINNET_TAPROOT.to_uppercase()),
        ] {
            assert!(matches!(parse(&text).status, Valid(_)), "{text}");
        }
    }

    #[test]
    fn a_uri_bip21_rejects_is_not_valid() {
        assert_eq!(
            parse(&format!("bitcoin:{MAINNET_TAPROOT}?")).status,
            NotAnAddress
        );
    }

    #[test]
    fn mixed_case_highlights_the_rarer_case() {
        let mut text = MAINNET_TAPROOT.to_string();
        text.replace_range(10..12, &text[10..12].to_uppercase());
        let input = parse(&text);
        assert_eq!(input.status, MixedCase);
        let upper: Vec<u32> = text
            .char_indices()
            .filter(|(_, c)| c.is_ascii_uppercase())
            .map(|(i, _)| i as u32)
            .collect();
        assert_eq!(input.bad_chars, upper);

        let mut text = MAINNET_TAPROOT.to_uppercase();
        text.replace_range(10..12, &text[10..12].to_lowercase());
        let input = parse(&text);
        assert_eq!(input.status, MixedCase);
        assert_eq!(input.bad_chars, vec![10, 11]);
    }

    #[test]
    fn wrong_network_names_the_address_network() {
        assert_eq!(
            parse(TESTNET_TAPROOT).status,
            WrongNetwork {
                address_network: AddressNetwork::Testnet
            }
        );
        assert_eq!(
            AddressInput::parse(MAINNET_TAPROOT, Network::Signet).status,
            WrongNetwork {
                address_network: AddressNetwork::Mainnet
            }
        );
        assert!(matches!(
            AddressInput::parse(TESTNET_TAPROOT, Network::Testnet4).status,
            Valid(_)
        ));
    }

    #[test]
    fn non_taproot_addresses_are_not_ours() {
        assert_eq!(parse(MAINNET_SEGWIT_V0).status, NotTaproot);
        assert_eq!(parse(MAINNET_P2PKH).status, NotTaproot);
    }

    #[test]
    fn bad_checksum_locates_a_single_substituted_character() {
        for i in [4, 20, 55, 61] {
            let c = MAINNET_TAPROOT.chars().nth(i).unwrap();
            let text = with_char_at(MAINNET_TAPROOT, i, a_different_bech32_char(c));
            let input = parse(&text);
            assert_eq!(input.status, BadChecksum, "{text}");
            assert_eq!(input.bad_chars, vec![i as u32], "{text}");
        }
    }

    #[test]
    fn bad_checksum_on_segwit_v0_uses_bech32() {
        let text = with_char_at(MAINNET_SEGWIT_V0, 10, 'q');
        let input = parse(&text);
        assert_eq!(input.status, BadChecksum);
        assert_eq!(input.bad_chars, vec![10]);
    }

    #[test]
    fn highlights_are_offsets_into_the_entered_text() {
        let text = with_char_at(MAINNET_TAPROOT, 20, a_different_bech32_char('n'));
        let input = parse(&format!("  bitcoin:{text}?amount=1"));
        assert_eq!(input.status, BadChecksum);
        assert_eq!(input.bad_chars, vec![20 + 10]);
    }

    #[test]
    fn highlights_count_utf16_units_before_the_address() {
        let text = with_char_at(MAINNET_TAPROOT, 20, 'b');
        let input = parse(&format!("\u{3000}{text}"));
        assert_eq!(input.status, InvalidCharacters);
        assert_eq!(input.bad_chars, vec![21]);
    }

    #[test]
    fn characters_outside_bech32_are_highlighted() {
        let text = with_char_at(&with_char_at(MAINNET_TAPROOT, 8, 'o'), 30, 'i');
        let input = parse(&text);
        assert_eq!(input.status, InvalidCharacters);
        assert_eq!(input.bad_chars, vec![8, 30]);
    }

    #[test]
    fn truncated_or_extended_bech32_is_the_wrong_length() {
        assert_eq!(parse(&MAINNET_TAPROOT[..50]).status, WrongLength);
        assert_eq!(parse(&format!("{MAINNET_TAPROOT}q")).status, WrongLength);
        assert_eq!(parse("bc1").status, NotAnAddress);
    }

    #[test]
    fn base58_errors() {
        let typo = with_char_at(MAINNET_P2PKH, 10, 'X');
        assert_eq!(parse(&typo).status, BadChecksum);
        let invalid = with_char_at(MAINNET_P2PKH, 10, '0');
        let input = parse(&invalid);
        assert_eq!(input.status, InvalidCharacters);
        assert_eq!(input.bad_chars, vec![10]);
        assert_eq!(parse(&MAINNET_P2PKH[..20]).status, WrongLength);
    }

    #[test]
    fn other_text_is_not_an_address() {
        assert_eq!(parse("hello world").status, NotAnAddress);
    }
}
