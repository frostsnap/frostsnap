# BIP-322 Proof of Funds — implementation plan (Phase 2)

Status: **plan only, not implemented.** Phase 1 (single-address BIP-322 "simple"
signing + verify) is implemented and shipped behind developer mode.

## What it is

"Simple"/"full" BIP-322 prove control of the key behind **one** address. **Proof
of Funds** (the `pof` variant) proves control of an **arbitrary set of UTXOs** —
e.g. a whole wallet's balance — in a single signature object. This is what
"proof of reserves" features use.

## How the construction works (BIP-322 spec)

Reference: <https://github.com/bitcoin/bips/blob/master/bip-0322.mediawiki>

A proof of funds is a single `to_sign` virtual transaction:

- **`to_sign` input 0** spends the `to_spend` output, exactly like simple/full.
  `to_spend.vout[0].scriptPubKey = message_challenge` — i.e. the scriptPubKey of
  one chosen "challenge" address you control. Because `to_spend`'s txid commits
  to the tagged `message_hash` (in `to_spend`'s input scriptSig), signing input 0
  commits to the message. Input 0 **is** signed/witnessed, under the challenge
  address's key.
- **`to_sign` inputs 1..n** are the **real on-chain UTXOs** you want to prove.
  "All outputs that the signer wishes to demonstrate control of are included as
  additional inputs of `to_sign`", with "their witness and scriptSig data … set
  as though these outputs were actually being spent." Each carries its
  Witness/Non-Witness UTXO field.
- **Output**: a single value-0 `OP_RETURN`.
- **Encoding**: the whole `to_sign` is a **finalized PSBT**, base64-encoded with
  the `pof` prefix. (`to_spend` is also represented as a finalized PSBT.)

To prove the entire wallet: pick any wallet address as the challenge address
(input 0), then add **every wallet UTXO** as inputs 1..n. Each input is signed by
the key controlling its address — so a multi-address wallet produces one PSBT
with one signature per input.

**Verification** is two-part and the spec is explicit about the split: a verifier
checks every input's witness cryptographically, but "validators of a proof of
funds need access to the current UTXO set, to learn that the claimed inputs exist
on the blockchain and remain unspent. An offline validator therefore can only
attest to the cryptographic validity of the additional inputs' witness stack, but
not its blockchain state." So the recipient must separately confirm the UTXOs are
real and unspent (e.g. against their own node/Electrum).

## Why this fits Frostsnap cleanly

Frostsnap **already signs multiple taproot inputs in one FROST session**:
`TransactionTemplate::iter_sighashes_of_locally_owned_inputs()`
(`frostsnap_core/src/bitcoin_transaction.rs`) yields a `(LocalSpk, TapSighash)`
per owned input, and the device produces a signature share for each via the
existing `sign_items()` pipeline. A proof of funds is, mechanically, a multi-input
taproot signing where:

- every input is taproot key-path, signed with `AppTweak::Bitcoin(path)` (already
  the case for normal inputs), under **SIGHASH_ALL** (the form we adopted for
  Phase 1 — see `frostsnap_core/src/bip322.rs::SIGHASH_TYPE`);
- the taproot key-spend sighash already commits to **all** prevouts via
  `Prevouts::All` (`iter_sighash`), which is exactly what BIP-322 needs.

So the cryptography and the FROST flow are already there. The new work is
**constructing the right virtual transaction** and **emitting the PSBT**.

## Implementation outline

### 1. Core: a proof-of-funds builder (`frostsnap_core`)

Extend the `bip322` module (or a new `bip322_pof` module), no_std:

- Build `to_spend` for the challenge address (reuse Phase-1
  `build_to_spend(challenge_spk, message)`).
- Build `to_sign` as a `TransactionTemplate` (or a thin wrapper) whose:
  - **input 0** is a *synthetic* owned input: outpoint = `to_spend:0`, value = 0,
    owner = `LocalSpk { master_appkey, challenge_path }`, prevout scriptPubKey =
    `challenge_spk`;
  - **inputs 1..n** are the real owned UTXOs (`outpoint`, `value`,
    `LocalSpk { path }`) — these come straight from the wallet's UTXO set;
  - single `OP_RETURN` output, value 0; version 0, locktime 0, sequences 0.
- `TransactionTemplate` already computes per-input taproot sighashes over
  `Prevouts::All` — verify it produces identical sighashes to a reference impl
  for input 0 (synthetic) and the real inputs. The main new bit is letting input
  0 carry a synthetic prevout (value 0, the `to_spend` output) rather than a real
  wallet UTXO; check whether `push_imaginary_owned_input` + a custom outpoint
  suffices or a small extension is needed.

A new `WireSignTask::Bip322ProofOfFunds { message, challenge_path, utxos }`
variant (or reuse `BitcoinTransaction` with a flag) drives `check()`/`sign_items()`
the same way as Phase 1.

### 2. Coordinator (std): PSBT assembly + encoding

After FROST returns one 64-byte signature per input:

- Build a `bitcoin::Psbt` from `to_sign`; for each input set `witness_utxo`
  (input 0 → the `to_spend` output; inputs 1..n → the real prevout `TxOut`) and
  `final_script_witness` = the 65-byte `SIGHASH_ALL` witness element
  (`frostsnap_core::bip322::witness_element`).
- Serialize the finalized PSBT, base64-encode, prepend `pof`.
- This lives next to `bip322_signature_to_string` in
  `frostsnapp/rust/src/api/signing.rs`.

### 3. Device display

Mirror Coldcard's proof-of-reserves UX: show **"Proof of funds: N inputs,
<total> BTC"**, the message, and the OP_RETURN output, with a single hold-to-
confirm. (Coldcard shows e.g. *"21 inputs, 1 output, 0.20000000 BTC"*;
<https://github.com/Coldcard/firmware/blob/master/docs/proof-of-reserves-bip-322.md>.)
The device must recompute every input's sighash from the wire task (as today) so
it never trusts coordinator-supplied prevouts.

### 4. Flutter UX

- Entry: a "Prove funds" action (likely under More → Sign data, dev-gated), and/or
  from the Addresses view.
- Default to **all UTXOs** (whole-wallet proof); optionally let advanced users
  select a subset. Show total amount being proven.
- Output: the `pof` base64 string with copy, plus the message and the challenge
  address.

## Risks / open questions

- **No Rust verification oracle.** The `bip322` crate (v0.0.10) implements only
  simple/full, **not** `pof`, and `verify_full` handles a single address. So we
  can't cross-check `pof` output against it the way we did in Phase 1. We'll need
  a different reference to validate against — candidates: Coldcard firmware,
  Bitcoin Core's BIP-322 branch (`verifymessage`), or a JS/Python implementation.
  **This is the biggest correctness risk** and should be resolved before coding.
- **No spec test vectors for PoF.** The BIP ships `basic-` and `generated-test-
  vectors.json` for hashing/simple/full only — none for proof of funds. We must
  generate our own cross-impl vectors.
- **Synthetic input 0 in `TransactionTemplate`.** The template assumes real
  prevouts/owners; representing the value-0 `to_spend` output as input 0 may need
  a small, careful extension (and must not leak into normal tx fee/`net_value`
  logic). Confirm `fee()`/`effect()` paths aren't invoked on a PoF template.
- **PSBT finalization details.** Getting `witness_utxo` per input and the
  finalized-PSBT serialization byte-exact is fiddly; the recipient's verifier is
  strict. Validate against the chosen reference impl with multiple inputs.
- **Privacy.** A whole-wallet proof of funds reveals every UTXO/address and links
  them publicly to the proof recipient. The UX should warn about this before
  signing.
- **UX scope.** Selecting "the whole wallet" is easy; arbitrary subset selection
  is a bigger UI. Recommend shipping whole-wallet (all UTXOs) first.

## Recommended first step

Before writing code, build a tiny cross-implementation harness: produce a `pof`
signature for a known key/UTXO set with one reference implementation (e.g.
bitcoinjs or Bitcoin Core's branch) and capture it as a test vector, then drive
the Frostsnap implementation to reproduce and verify it. Without that oracle,
`pof` correctness can't be trusted.
