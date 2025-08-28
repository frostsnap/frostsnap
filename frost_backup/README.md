# Frost Backup Scheme

Mnemonic encoding for [secp256k1] [Shamir secret shares].

## Overview

A Shamir secret sharing backup scheme for secp256k1 scalars that encodes shares as 25 [BIP39] words. This scheme is intended for use with [FROST] signature implementations so users can back up their individual FROST keys (which are just shamir secret shares). The only thing that distinguishes the scheme slightly from a ordinary Shamir secret sharing backup is the small *[polynomial checksum](#polynomial-checksum)* which is meant to discourage an attacker trying to trick a recovered device into thinking it's part of a different key that it was generated with.

## Format Specification

### Structure

```
#<share_index> <25 BIP39 words>
```

#### `share_index`

The share index is a decimal integer prefixed `#` with in the format. The index is never `0`. 

#### Words Bit Layout

The 25 BIP39 words encode 275 bits (11 bits per word):

| Bits     | Length | Words    | Content                |
|----------|--------|----------|------------------------|
| 0-255    | 256    | 1-24     | Secret share scalar    |
| 256-263  | 8      | 24       | [Polynomial checksum](#polynomial-checksum)    |
| 264-274  | 11     | 25       | [Words checksum](#words-checksum)         |

## Checksums

The checksums are [SHA256] hashes of the input data.

- `share_index` (32 bytes): although share indices are usually small numbers they are encoded in the hash as 32-byte secp256k1 scalar (big endian encoded 256-bit integer).
- `secret_scalar` (32 bytes): the secret share encoded as a 32-byte secp256k1 scalar

### Polynomial Checksum

For the purpose of the polynomial checksum see below.

- `polynomial_commitment` (`t` * 33 bytes): The polynomial commitment is the public version of the Shamir secret sharing polynomial. It's created by multiplying each coefficient of the polynomial by the secp256k1 generator point G. It is encoded as a simple concatenation of the coefficient points, where each point is encoded as a 33-byte compressed secp256k1 point. For a threshold-t scheme, there are t coefficient points, resulting in `33*t` bytes total.

```
hash: [u8;32] = SHA256(share_index || secret_scalar || polynomial_commitment)
poly_checksum: u8 = hash[0]  // First byte (8 bits)
```
Used to fill the last 8 bits of the 24th word (along with 3 bits from the scalar).

### Words Checksum

The words checksum detects errors in the other data of the backup.

The first 11 bits of a hash of all the other components of the backup.

- `polynomial_checksum` - the single byte checksum from the above section

```
Hash: [u8;32] = SHA256(share_index || secret_scalar || polynomial_checksum)
words_checksum: u16 = ((hash[0] as u16) << 3) | ((hash[1] as u16) >> 5)  // First 11 bits
```

Converted directly into a word index and appended as the 25th word.

## Implementation Notes

- `no_std` with `alloc`
- Supports all thresholds including 1-of-1

## Purpose of the Polynomial Checksum

This is a general Shamir secret sharing scheme for secp256k1 secret keys. The format includes an 8-bit polynomial checksum that helps detect when shares are being associated with an incorrect public key after restoration.

The polynomial checksum reduces the risk of public key substitution attacks in threshold signing devices by making it highly likely that such attacks will be detected.

**Attack scenario:**
1. Multiple devices hold shares of the same secret key with associated public key
2. After restoring from a backup, a device knows its share but not the public key
3. Malicious coordinator provides incorrect public key for the share
4. Device derives and displays incorrect addresses
5. User sends funds to attacker-controlled addresses

The attack requires specific timing (user receiving funds immediately after restoration) and is detectable through any spending operation. If successful, the attacker who controls the false public key can steal any funds sent to the incorrect addresses.

**Mitigation:**

The polynomial checksum provides automated verification with 1/256 false positive rate. On validation failure, devices MUST:
- Alert user to malicious coordinator
- Terminate coordinator connection permanently
- Require manual intervention to continue

Single-attempt enforcement prevents brute-force attacks against the 8-bit checksum.

*Note: Legacy multisig requires manual verification that all restored devices display identical xpubs.*


## Fingerprint Grinding and Share Discovery

The frostsnap key generation algorithm uses "[fingerprint grinding][fingerprint-grinding]" to embed a checksum in the polynomial coefficients, allowing shares to be validated without external information. This enables automatic discovery of compatible shares from a mixed collection. Without it you would have to look on-chain to see if the set of backups contained funds actual on-chain funds to validate them.
We recommend that implementations of this backup scheme also use a compatible implementation of fingerprint grinding in their key generation scheme to ensure backups can be recovered across systems.

## CLI

```bash
alias frost_backup="cargo run --release"
# or cargo install it
cargo install

# Generate shares
frost_backup generate <threshold> <number-of-shares> [secret-hex]  # secret optional - will generate random if not provided

# Reconstruct secret
frost_backup reconstruct [threshold]  # threshold optional - will auto-discover if not provided
```

Shares written to/read from `./shares/` directory.

[secp256k1]: https://en.bitcoin.it/wiki/Secp256k1
[Shamir secret shares]: https://en.wikipedia.org/wiki/Shamir%27s_secret_sharing
[BIP39]: https://github.com/bitcoin/bips/blob/master/bip-0039.mediawiki
[FROST]: https://datatracker.ietf.org/doc/draft-irtf-cfrg-frost/
[SHA256]: https://en.wikipedia.org/wiki/SHA-2
[fingerprint-grinding]: https://github.com/BlockstreamResearch/bip-frost-dkg#fingerprinting-key-generation
