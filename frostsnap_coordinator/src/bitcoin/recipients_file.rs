//! Recipients for a send, read from a CSV file with one `address,amount_sat[,label]` line per
//! recipient.

use bdk_chain::bitcoin::{self, Network};
use frostsnap_core::silent_payments::SilentPaymentRecipient;
use std::str::FromStr;

/// A recipient line from a recipients file. Its address is not checked until `validate`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecipientEntry {
    /// The 1-based line the recipient was read from.
    pub line: usize,
    pub address: String,
    pub amount_sat: u64,
    pub label: Option<String>,
}

/// Parses `address,amount_sat[,label]` lines. A first row whose address field is `address` is a
/// header and is skipped, as are blank lines and lines starting with `#`. Every malformed line is
/// reported with its line number.
pub fn parse_csv(contents: &str) -> Result<Vec<RecipientEntry>, String> {
    let mut entries = Vec::new();
    let mut errors = Vec::new();
    let mut first_row = true;

    for (i, line) in contents.lines().enumerate() {
        let line_no = i + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let is_first_row = first_row;
        first_row = false;

        let fields: Vec<&str> = trimmed
            .splitn(3, ',')
            .map(|field| field.trim().trim_matches('"'))
            .collect();
        if is_first_row && fields[0].eq_ignore_ascii_case("address") {
            continue;
        }
        if fields.len() < 2 {
            errors.push(format!(
                "Line {line_no}: expected 'address,amount_sat[,label]'"
            ));
            continue;
        }

        match fields[1].parse::<u64>() {
            Ok(amount_sat) => entries.push(RecipientEntry {
                line: line_no,
                address: fields[0].to_string(),
                amount_sat,
                label: fields
                    .get(2)
                    .filter(|label| !label.is_empty())
                    .map(|label| label.to_string()),
            }),
            Err(_) => errors.push(format!("Line {line_no}: invalid amount '{}'", fields[1])),
        }
    }

    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }
    if entries.is_empty() {
        return Err("No recipients found in CSV file".to_string());
    }
    Ok(entries)
}

/// Checks each address on `network` and each amount against the dust limit, as the send builder
/// does for a single recipient. Every failing line is reported with its line number.
pub fn validate(entries: &[RecipientEntry], network: Network) -> Result<(), String> {
    let mut errors = Vec::new();

    for entry in entries {
        let line = entry.line;
        let min_non_dust =
            if SilentPaymentRecipient::from_address(&entry.address, network).is_some() {
                silent_payment_min_non_dust()
            } else {
                let address = bitcoin::Address::from_str(&entry.address)
                    .ok()
                    .and_then(|address| address.require_network(network).ok());
                match address {
                    Some(address) => address.script_pubkey().minimal_non_dust().to_sat(),
                    None => {
                        errors.push(format!(
                            "Line {line}: invalid address \"{}\"",
                            entry.address
                        ));
                        continue;
                    }
                }
            };

        if entry.amount_sat < min_non_dust {
            errors.push(format!(
                "Line {line}: amount {} sat is below dust threshold ({min_non_dust} sat)",
                entry.amount_sat
            ));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

/// The dust limit of a silent payment output. Its script is not known until the signers' ECDH
/// shares are in, but it is always P2TR, so a P2TR script with a zero key has the same limit.
pub fn silent_payment_min_non_dust() -> u64 {
    let mut script = vec![0x51, 0x20];
    script.extend_from_slice(&[0u8; 32]);
    bitcoin::ScriptBuf::from_bytes(script)
        .minimal_non_dust()
        .to_sat()
}

#[cfg(test)]
mod tests {
    use super::*;
    use frostsnap_core::schnorr_fun::fun::Point;

    const TB1_ADDR: &str = "tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx";

    fn sp_address() -> String {
        let scan = [
            0x02, 0x20, 0xbc, 0xfa, 0xc5, 0xb9, 0x9e, 0x04, 0xad, 0x1a, 0x06, 0xdd, 0xfb, 0x01,
            0x6e, 0xe1, 0x35, 0x82, 0x60, 0x9d, 0x60, 0xb6, 0x29, 0x1e, 0x98, 0xd0, 0x1a, 0x9b,
            0xc9, 0xa1, 0x6c, 0x96, 0xd4,
        ];
        let spend = [
            0x02, 0x5c, 0xc9, 0x85, 0x6d, 0x6f, 0x83, 0x75, 0x35, 0x0e, 0x12, 0x39, 0x78, 0xda,
            0xac, 0x20, 0x0c, 0x26, 0x0c, 0xb5, 0xb5, 0xae, 0x83, 0x10, 0x6c, 0xab, 0x90, 0x48,
            0x4d, 0xcd, 0x8f, 0xcf, 0x36,
        ];
        let recipient = SilentPaymentRecipient {
            scan_key: Point::from_slice(&scan).unwrap(),
            spend_key: Point::from_slice(&spend).unwrap(),
        };
        recipient.address(Network::Testnet).unwrap()
    }

    fn entry(line: usize, address: &str, amount_sat: u64) -> RecipientEntry {
        RecipientEntry {
            line,
            address: address.to_string(),
            amount_sat,
            label: None,
        }
    }

    #[test]
    fn parse_skips_header_blanks_and_comments() {
        let csv = format!(
            "address,amount_sat,label\n\n# Payroll\n{TB1_ADDR},50000,Bob\n{TB1_ADDR},75000\n"
        );
        let entries = parse_csv(&csv).unwrap();
        assert_eq!(
            entries,
            vec![
                RecipientEntry {
                    line: 4,
                    address: TB1_ADDR.to_string(),
                    amount_sat: 50_000,
                    label: Some("Bob".to_string()),
                },
                entry(5, TB1_ADDR, 75_000),
            ]
        );
    }

    #[test]
    fn parse_without_header_and_with_quoted_fields() {
        let csv = format!("\"{TB1_ADDR}\", \"1000\" ,\"\"\n{TB1_ADDR},2000,a, b\n");
        let entries = parse_csv(&csv).unwrap();
        assert_eq!(entries[0], entry(1, TB1_ADDR, 1_000));
        assert_eq!(entries[1].label, Some("a, b".to_string()));
    }

    #[test]
    fn parse_reports_every_malformed_line() {
        let csv = format!("{TB1_ADDR}\n{TB1_ADDR},lots\n{TB1_ADDR},5\n");
        let err = parse_csv(&csv).unwrap_err();
        assert_eq!(
            err,
            "Line 1: expected 'address,amount_sat[,label]'\nLine 2: invalid amount 'lots'"
        );
    }

    #[test]
    fn parse_rejects_empty_file() {
        assert_eq!(
            parse_csv("# nothing here\n\n").unwrap_err(),
            "No recipients found in CSV file"
        );
    }

    #[test]
    fn validate_accepts_addresses_and_silent_payments_above_dust() {
        let entries = vec![entry(1, TB1_ADDR, 50_000), entry(2, &sp_address(), 50_000)];
        validate(&entries, Network::Testnet).unwrap();
    }

    #[test]
    fn validate_reports_bad_address_with_line() {
        let entries = vec![entry(3, "badaddress123", 50_000)];
        let err = validate(&entries, Network::Testnet).unwrap_err();
        assert_eq!(err, "Line 3: invalid address \"badaddress123\"");
    }

    #[test]
    fn validate_rejects_a_corrupted_checksum() {
        for address in [TB1_ADDR.to_string(), sp_address()] {
            let mut corrupted = address.clone();
            let last = corrupted.pop().unwrap();
            corrupted.push(if last == 'q' { 'p' } else { 'q' });
            let err = validate(&[entry(4, &corrupted, 50_000)], Network::Testnet).unwrap_err();
            assert_eq!(err, format!("Line 4: invalid address \"{corrupted}\""));
        }
    }

    #[test]
    fn validate_rejects_address_from_another_network() {
        let mainnet = "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4";
        let err = validate(&[entry(1, mainnet, 50_000)], Network::Testnet).unwrap_err();
        assert_eq!(err, format!("Line 1: invalid address \"{mainnet}\""));
    }

    #[test]
    fn validate_reports_dust_with_line() {
        let entries = vec![entry(1, TB1_ADDR, 10), entry(2, &sp_address(), 10)];
        let err = validate(&entries, Network::Testnet).unwrap_err();
        // P2WPKH dust is 294 sat, P2TR (silent payment) dust is 330 sat.
        assert_eq!(
            err,
            "Line 1: amount 10 sat is below dust threshold (294 sat)\n\
             Line 2: amount 10 sat is below dust threshold (330 sat)"
        );
    }
}
