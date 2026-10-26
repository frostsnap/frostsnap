use alloc::{string::String, vec::Vec};
use bitcoin::{
    bech32::{
        primitives::{decode::CheckedHrpstring, gf32::Fe32, iter::ByteIterExt},
        Bech32m, Hrp,
    },
    Network,
};

use schnorr_fun::fun::Point;

use super::SilentPaymentRecipient;

impl SilentPaymentRecipient {
    /// The BIP352 address of this recipient on `network`: `sp1...` on mainnet, `sprt1...` on
    /// regtest and `tsp1...` on the test networks.
    pub fn address(&self, network: Network) -> Option<String> {
        let hrp = match network {
            Network::Bitcoin => "sp",
            Network::Regtest => "sprt",
            _ => "tsp",
        };
        let data = [self.scan_key.to_bytes(), self.spend_key.to_bytes()].concat();
        let bytes = bitcoin::bech32::primitives::encode::Encoder::<_, Bech32m>::new(
            data.into_iter().bytes_to_fes(),
            &Hrp::parse(hrp).ok()?,
        )
        .with_witness_version(Fe32::Q)
        .bytes()
        .collect::<Vec<u8>>();
        String::from_utf8(bytes).ok()
    }

    /// The recipient whose BIP352 address on `network` is `address`. `None` if `address` does not
    /// decode or is for another network.
    pub fn from_address(address: &str, network: Network) -> Option<Self> {
        let mut checked = CheckedHrpstring::new::<Bech32m>(address).ok()?;
        checked.remove_witness_version()?;
        let bytes: Vec<u8> = checked.byte_iter().collect();
        if bytes.len() != 66 {
            return None;
        }
        let recipient = SilentPaymentRecipient {
            scan_key: Point::from_slice(&bytes[..33])?,
            spend_key: Point::from_slice(&bytes[33..])?,
        };
        (recipient.address(network)? == address.to_lowercase()).then_some(recipient)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::Network;
    use schnorr_fun::fun::{g, marker::*, Scalar, G};

    fn rec(scan: u8, spend: u8) -> SilentPaymentRecipient {
        let scan_s = Scalar::<Secret, Zero>::from_bytes_mod_order([scan; 32])
            .non_zero()
            .unwrap();
        let spend_s = Scalar::<Secret, Zero>::from_bytes_mod_order([spend; 32])
            .non_zero()
            .unwrap();
        SilentPaymentRecipient {
            scan_key: g!(scan_s * G).normalize().non_zero().unwrap(),
            spend_key: g!(spend_s * G).normalize().non_zero().unwrap(),
        }
    }

    #[test]
    fn from_address_round_trips() {
        let r = rec(1, 2);
        let addr = r.address(Network::Signet).unwrap();
        let parsed = SilentPaymentRecipient::from_address(&addr, Network::Signet).unwrap();
        assert_eq!(parsed, r);
    }

    #[test]
    fn from_address_wrong_network_rejected() {
        let r = rec(1, 2);
        let addr = r.address(Network::Signet).unwrap();
        assert!(SilentPaymentRecipient::from_address(&addr, Network::Bitcoin).is_none());
    }

    #[test]
    fn from_address_bad_checksum_rejected() {
        let addr = rec(1, 2).address(Network::Bitcoin).unwrap();
        let mut bad = addr.into_bytes();
        bad[10] = b'q'; // tamper a char; bech32 checksum will now fail
        let bad = String::from_utf8(bad).unwrap();
        assert!(SilentPaymentRecipient::from_address(&bad, Network::Bitcoin).is_none());
    }

    #[test]
    fn from_address_mainnet_sp1_on_bitcoin() {
        let r = rec(3, 4);
        let addr = r.address(Network::Bitcoin).unwrap();
        assert!(addr.starts_with("sp1"));
        let parsed = SilentPaymentRecipient::from_address(&addr, Network::Bitcoin).unwrap();
        assert_eq!(parsed, r);
    }

    #[test]
    fn from_address_regtest_sprt() {
        let r = rec(5, 6);
        let addr = r.address(Network::Regtest).unwrap();
        assert!(addr.starts_with("sprt1"));
        let parsed = SilentPaymentRecipient::from_address(&addr, Network::Regtest).unwrap();
        assert_eq!(parsed, r);
        assert!(SilentPaymentRecipient::from_address(&addr, Network::Signet).is_none());
    }
}
