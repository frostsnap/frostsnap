# psbt-pure-transform
# Read a PSBT using only the PSBT and the key, and test it properly

## Where this stands

The transform has been extracted (`frostsnap_coordinator/src/bitcoin/psbt_template.rs`),
`push_owned_output_checked` exists in core, and there are 11 tests that need no wallet.
What is wrong is the output *source*: it still asks the BDK indexer which key derives a
script pubkey, passed in as an `impl Fn(&Script) -> Option<LocalSpk>`.

That has to go. This plan replaces it.

## The approach

A PSBT carries the derivation for every key it expects a signer to recognise —
`PSBT_IN_TAP_BIP32_DERIVATION` for inputs, `PSBT_OUT_TAP_BIP32_DERIVATION` for outputs.
Assume it is there. The input loop already works this way. Make the output loop
symmetric and delete the indexer entirely.

For each output: read `tap_internal_key`, look it up in `tap_key_origins`, require the
fingerprint to be ours, convert the path, and hand the resulting `LocalSpk` to
`push_owned_output_checked`, which derives the spk from the `master_appkey` and rejects
the claim if it does not match the output's actual script. Anything that fails a step is
a foreign output.

The two loops then differ only in where the txout comes from, so factor the shared part
into one helper — `tap_internal_key` → origin → fingerprint → `BitcoinBip32Path` — and
call it from both. Do not leave two copies of that ladder.

### What this buys, and why it beats the index

- **The oracle argument disappears.** The signature becomes
  `psbt_to_tx_template(&Psbt, MasterAppkey) -> Result<TransactionTemplate, PsbtValidationError>`.
  A function of exactly its arguments: no closure, no BDK type, nothing to inject in
  tests.
- **#552 stops being a question the code can answer wrongly.** `index_of_spk` searches
  every wallet loaded in the app, and that cross-wallet knowledge is the whole reason an
  output could be attributed to the wrong key. A PSBT-sourced path carries a fingerprint,
  and the fingerprint check already rejects a sibling wallet's output. The bug goes away
  with the lookup, not with a guard bolted on top of it.
- **The lookahead ceiling goes away.** `index_of_spk` only knows `revealed + 50`
  (`wallet_persist.rs:33`), so an output past that was reported foreign even when it was
  ours — while an input at the same depth was signable, because inputs already read the
  PSBT. That asymmetry disappears with the source.

### The trust question, answered once

The claimed path is untrusted and is *verified*, not believed:
`push_owned_output_checked` derives the spk from `(master_appkey, bip32_path)` and
compares it with the output's actual script. A PSBT cannot make us attribute an output to
a key that does not produce it. The residual risk is a producer that *omits* the field,
which costs a "To Self" annotation and shows our own output as a recipient. That is the
accepted trade. Do not reintroduce the index as a hedge against it.

## Evidence already gathered — keep this in the plan

A Sparrow-produced PSBT (1 input, 2 outputs, self-send), read field by field:

| | path | origins keyed by its own `tap_internal_key` | key-path tweak matches spk |
|---|---|---|---|
| input 0 | `0/0/1/17` | yes | yes |
| output 0 (1 BTC, the payment) | `0/0/1/0` | yes | yes |
| output 1 (0.21996195, change) | `0/0/1/36` | yes | yes |

All three carry the same fingerprint and are 4 segments — the shape
`BitcoinBip32Path::from_u32_slice` accepts. Sparrow annotates the payment, not only the
change. Every path is on the internal keychain because the PSBT was made by paying the
wallet's own change address.

## Files

- `frostsnap_coordinator/src/bitcoin/psbt_template.rs` — drop the `owned_spk` parameter,
  add the shared origin→path helper, rewrite the output loop.
- `frostsnap_coordinator/src/bitcoin/wallet.rs` — `psbt_to_tx_template` no longer needs
  the wallet at all. Delete the method rather than leave a passthrough.
- `frostsnapp/rust/src/api/super_wallet.rs` — `psbt_to_unsigned_tx` calls the free
  function directly and stops locking the wallet. The Dart-facing API must not change.

## Testing

Keep the existing input tests. The output tests must be rewritten against the new source
— an spk map no longer exists.

Cover at least:
- an output that is ours, annotated the way Sparrow annotates one
- an output with no `tap_internal_key`, and one with no origin for it — both foreign
- an output whose origin fingerprint is another wallet's — foreign, and the #552 case:
  assert the signed transaction still pays exactly what the PSBT asked for
- an output whose claimed path derives a different spk — rejected by
  `push_owned_output_checked`
- an output at an index far past any plausible lookahead — recognised as ours, which the
  index-based code could not do
- the existing input cases, unchanged in behaviour

Every test must fail if you revert the behaviour it covers. Check by actually reverting,
not by inspection; the previous round used a mutation script over every branch and found
no gaps, so hold this round to that standard.

## Constraints

- **No WHAT comments.** Only WHY, and only where the why is not obvious. Delete the
  comment: if the code still says everything it said, leave it deleted.
- **No outward actions.** Do not push a branch, do not open or comment on a PR.
- Behaviour is preserved except where this plan says to change it. If you find another
  bug, report it; do not fold an unrelated fix into the refactor.
- PR #552 (`psbt-fix`) fixes the same output-labelling bug against the index. This plan
  supersedes that approach; leave the PR alone and say so in the report.
