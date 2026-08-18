# signature-input-mapping
# Put a signature on the input it was made for

## The bug

A signing session produces **one signature per locally owned input**, in template order —
`sign_items()` (`frostsnap_core/src/sign_task.rs:163`) maps over
`iter_sighashes_of_locally_owned_inputs()`, which filters to inputs the template owns.

Two consumers pair that list against **every** input in the transaction:

```rust
// frostsnapp/rust/src/api/signing.rs:204  UnsignedTx::complete
for (txin, signature) in tx.input.iter_mut().zip(signatures) {

// frostsnapp/rust/src/api/bitcoin.rs:219  Transaction::fill_signatures
for (txin, signature) in self.inner.input.iter_mut().zip(signatures) {
```

When every input is ours the two orders coincide and nothing is wrong. That is why the
app's own send flow has never shown this: `send.rs` only ever spends our own coins.

A PSBT is the case where they diverge, because `push_foreign_input` exists. Reproduced
against a template whose input 0 is foreign and input 1 is ours:

```
tx inputs   = 2
sign_items  = 1
zip assigns the signature to input index 0   <- the foreign input
the only signable input is index 1
```

So the signature is written onto an input we do not own, and the input we do own is left
with an empty witness. The result is a transaction that cannot be valid. `fill_signatures`
feeds the unbroadcasted-tx list, so this is the broadcast path, not a display path.

## The architectural cause, which is the thing to fix

`TransactionTemplate` already knows the answer:

```rust
pub fn iter_locally_owned_inputs(&self) -> impl Iterator<Item = (usize, &Input, &LocalSpk)>
```

It yields the input index. Nothing uses it for this. Instead, three call sites each invent
their own idea of which inputs the signatures belong to, and all three disagree with the
one that produced them:

1. `UnsignedTx::complete` — positional zip over all inputs. Wrong with any foreign input.
2. `Transaction::fill_signatures` — the same positional zip. Wrong the same way.
3. `Transaction::attach_signatures_to_psbt` (`bitcoin.rs:251`) — walks
   `owned_input_indices()`, which asks whether a prevout's spk is in `is_mine`. That is a
   different question from "did the template own this input". `is_mine` is built from
   owned input spks **and owned output spks** (`bitcoin.rs:195`), so an input paying an
   address we also send to counts as ours, and an input the template skipped (already
   signed, no derivation) can too. This one usually fails safe — the length check returns
   `None` — but it fails safe by accident, and a compensating pair of errors maps
   signatures onto the wrong inputs.

Do not fix these three in three places. **The order in which signatures are produced is a
fact about `TransactionTemplate`, so the mapping back belongs next to `sign_items` in
`frostsnap_core`, defined once.** Give the template the method — something that takes the
signature list and either yields `(input_index, signature)` pairs or applies them — and
make all three call sites use it. A consumer must not be able to express the pairing
itself, which is what let three different wrong answers exist.

While there, delete `owned_input_indices` rather than leaving a second way to ask the
question.

**Correction, found during implementation:** this plan originally also called for removing
the output spks from `is_mine`. That is wrong. `is_mine` is what classifies *scripts* as
ours for display and totals — `recipients()` (`bitcoin.rs:410`) and both `_sum_inputs` and
`_sum_outputs` depend on it covering outputs. Its output entries are load-bearing, not
contamination. The defect was only ever `owned_input_indices` asking `is_mine` a question
it cannot answer: "is this spk ours" is not "did the template own this input".

## Verify the count, do not assume it

The template knows how many signatures it expects. A mismatch between that and the list
handed in is a programming error or a malicious coordinator response, and it must not
silently truncate — which is exactly what `zip` does today. Make it explicit and loud.

## Testing

The bug is invisible to any test where all inputs are owned, so every test here needs a
foreign input present.

Cover at least:
- a template with a foreign input **before** an owned one — the signature lands on the
  owned input and the foreign input keeps an empty witness
- foreign inputs both before and after two owned inputs, so a single off-by-one cannot
  pass
- an all-owned template — the existing behaviour is unchanged
- too few and too many signatures — rejected, not truncated
- `attach_signatures_to_psbt` over a PSBT with a foreign input, asserting `tap_key_sig`
  lands on the right PSBT input index
- an input whose prevout spk equals one of our own output spks, which is what makes
  `is_mine` answer wrongly today

Use the real Core fixtures in `frostsnap_coordinator/tests/fixtures/` as a base where it
helps; `core_stranger.psbt` already has a foreign output, and a foreign *input* fixture can
be produced the same way (fund a second Core wallet and let both contribute).

Every test must fail if you revert the behaviour it covers. Check by actually reverting.

## Constraints

- **No WHAT comments.** Only WHY, and only where the why is not obvious.
- **No outward actions.** Do not push a branch, do not open or comment on a PR.
- `attach_signatures_to_psbt` must stay on `Transaction`. Moving it to `UnsignedTx` looks
  tidier but breaks `SigningMode.restore`, where `TxSigningParams.unsignedTx` is `None`
  (`wallet_tx_details.dart:36`) while the PSBT-attach path still runs. Instead `Transaction`
  carries the `TransactionTemplate` it was built from, so the mapping travels with the
  object and the Dart API is untouched.
- `frostsnapp/rust/src/frb_generated.rs` is generated and gitignored; run
  `just gen` if the API surface changes, and do not commit `pubspec.lock` churn.
- If the Dart-facing API changes shape, say so explicitly in the report — `psbt.dart` and
  `wallet_tx_details.dart` are the callers.
