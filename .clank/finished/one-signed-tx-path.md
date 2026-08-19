# one-signed-tx-path
# Finish the signature mapping, and collapse the recipes that let it drift

## Why there is a second plan

`signature-input-mapping` centralised the signature→input pairing and fixed three
consumers. It missed a fourth, and the one it missed is the one that broadcasts.

That is the finding to take seriously: the previous plan enumerated consumers by reading,
not by grepping every use of `EncodedSignature`. Doing that grep now closes the set — there
is exactly one positional zip left in application code:

```
frostsnapp/rust/src/api/bitcoin.rs:402   for (txin, signature) in tx.input.iter_mut().zip(signatures)
```

## The live bugs

### 1. `Transaction::with_signatures` still signs the wrong input

`bitcoin.rs:400`. Identical to the bug already fixed elsewhere: signatures are produced per
*owned* input, and this pairs them against every input. `wallet_tx_details.dart:772` calls
it inside `broadcast()`, so this builds the transaction that goes to the node. Sign a PSBT
with a foreign input today and the witness lands on the foreign input while ours stays
empty.

The machinery to fix it exists — `TransactionTemplate::to_signed_rust_bitcoin_tx`. The
template is already on `Transaction`, so this becomes a delegation. It must become
fallible: a `Transaction` from wallet history has no template, and a signature count that
disagrees must not be papered over. Decide deliberately what the Dart caller does with the
failure, and say so in the report — broadcasting nothing is correct, broadcasting a
mis-witnessed transaction is not.

### 2. `UnsignedTx::details` classifies scripts with the wallet index

`signing.rs:185` builds `is_mine` from `super_wallet.spk_index`, which is `index_of_spk`
bounded by lookahead (`revealed + 50`, `wallet_persist.rs:33`). The other two sites build
it from the template. `is_mine` feeds `recipients()` (`bitcoin.rs:392`), and `psbt.dart:82`
uses `details()` for the PSBT review screen — so an output past the lookahead is displayed
as a stranger's even though the template owns it. This is the same defect
`psbt-pure-transform` removed from the transform, left behind in the display path.

`details()` already holds `self.template_tx` three lines above. Use it.

### 3. Check whether `details()` also has the wrong prevouts

Same function sources `prevouts` from `super_wallet.get_prevouts`, i.e. the wallet's own
graph, whereas `Transaction::from_template` sources them from the template's inputs.

**Verified, and it is confirmed.** `get_prevouts` (`wallet.rs:110`) `filter_map`s away
outpoints the wallet cannot resolve, so a PSBT's foreign inputs are silently absent.
`_sum_inputs` (`bitcoin.rs`) returns `None` the moment *any* prevout is missing, and
`fee()`, `feerate()` and `balance_delta()` all go through it. So on master, opening a PSBT
that spends an input the wallet has never seen gives a review screen with no fee and no
feerate.

The template does not have this problem: it carries the prevout of every input the PSBT
declared, foreign ones included. `a_template_from_a_real_psbt_carries_every_prevout` holds
that against all three Bitcoin Core fixtures, and
`fee_survives_an_input_the_wallet_has_never_seen` covers the resulting `Transaction`. This
is therefore in scope, and building from the template fixes it as a consequence rather than
as a separate patch.

## The structural fix, which is the point

Three sites assemble a `Transaction` field by field — `bitcoin.rs:199`, `signing.rs:116`,
`signing.rs:185` — and two of them are byte-identical while the third quietly differs. That
duplication is *why* a divergent recipe survived, and why a fourth signature consumer could
exist unnoticed.

Give `Transaction` one constructor from a template and have all three use it. After that a
`Transaction` built from a signing session cannot have a different idea of what is ours
than the template it came from, because there is nowhere left to express one.

**Do not delete `UnsignedTx::complete`.** An earlier draft of this plan called for that on
the grounds it has no caller. That is true of master and false of the work in flight — see
below.

## What PR #509 (`nostr-taipei`, draft) already does here

Checked before implementing, because it rewrites this exact area. It is converging on the
same conclusion from the other side, which changes two items above.

- It introduces a typed `SignedTx`, produced by `unsignedTx.complete(signatures)`, and
  drives broadcast off that value.
- It moves PSBT attachment to `SignedTx.toSignedPsbt(psbt:)`, reasoning in its own comment
  that the artifact "carries the sigs internally, so the call site doesn't have to pair raw
  sigs with the PSBT" — the same conclusion this plan reached independently.
- It **removes** the `withSignatures` call from `broadcast()`.
- It reworks `is_mine` to `(BitcoinAccountKeychain, u32)`, marks it `#[frb(ignore)]`, and
  makes `_sum_inputs`/`_sum_outputs` generic over the map's value type.

What it does **not** do is fix the bug. `frostsnapp/rust/src/api/signing.rs` is
`status=modified, +0 −0` on that PR, and `signing.rs:273` on `nostr-taipei` is still
`for (txin, signature) in tx.input.iter_mut().zip(signatures)`. So #509 routes broadcast
*and* PSBT export through `complete()`, which is the positional zip. There the defect is
not one of four paths, it is the only path.

Consequences, which override the corresponding items above:

1. **`complete` stays.** It is master-dead and #509-central. The fix already made in
   `signature-input-mapping` is exactly what that branch needs and lacks.
2. **`with_signatures` is still fixed here, but it is terminal** — #509 deletes its only
   caller. Master needs it correct until then; do not spend effort preserving it beyond
   that.
3. **`details()` is untouched by #509**, so the `spk_index` and prevout items are wholly
   this plan's and conflict with nothing.

**Assumption, stated because it was not confirmed:** keep `is_mine` keyed to `u32` here.
Adopting #509's `(keychain, index)` would reduce the merge conflict but requires changing
`recipients()` semantics, which is that PR's feature work — this plan will not front-run
it. The consolidation must therefore keep `from_template` as the single builder, which is
the shape #509 also keeps, so the conflict is confined to the value type.

## Testing

Every test needs a foreign input; that is the only thing that separates the two orderings.

- `with_signatures` over a template with a foreign input before an owned one — the witness
  is on the owned input, the foreign input is untouched
- `with_signatures` with a mismatched signature count — refused, not truncated
- `with_signatures` on a `Transaction` with no template — refused
- an owned output past any plausible lookahead is `is_mine` in `recipients()`
- a transaction with an input the wallet has never seen still reports a fee
- ~~the three construction sites produce the same `is_mine`~~ — there is one site after the
  consolidation, so agreement is structural and a test asserting it would be vacuous

**Correction: the app crate can host tests.** An earlier draft of this plan asserted it
could not, because `frb_generated.rs` is gitignored. That is wrong. `.github/workflows/test.yml`
runs `./.github/actions/generate-frb` before `just test-ordinary` — its own comment says
"Restore bridge files just so we can test/lint native" — so `cargo test` covers
`rust_lib_frostsnapp` in CI like any other crate. Test API semantics where the API lives;
move code to `frostsnap_core` or `frostsnap_coordinator` when that is the right home for it,
not to buy testability that was never missing.

Every test must fail if you revert the behaviour it covers. Check by actually reverting.

## Constraints

- **No WHAT comments.** Only WHY, and only where the why is not obvious.
- **No outward actions.** Do not push a branch, do not open or comment on a PR.
- `frostsnapp/rust/src/frb_generated.rs` is generated and gitignored; run `just gen` when
  the API surface changes and do not commit `pubspec.lock` churn.
- Report every Dart-visible change. `wallet_tx_details.dart`, `psbt.dart` and
  `wallet_send.dart` are the callers of the functions this plan touches.
