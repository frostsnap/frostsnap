# fuzzy_recovery_no_hint
# "I don't know the threshold" recovery never completes: None became Some(0)

User-confirmed bug: a remote recovery lobby created with a specific
threshold works; one created with "I don't know" never enables
Recover, no matter how many shares land.

Root cause — one expression, two call sites in
`frostsnap_nostr/src/recovery/lobby.rs` (`recompute_current_recovery`
~line 743, and the leader's `Finish` verification ~line 685):

```rust
RecoveringAccessStructure::new(
    &shares,
    Some(state.metadata.threshold_hint.unwrap_or(0)),
    fingerprint,
)
```

`threshold_hint = None` ("I don't know") becomes `Some(0)` — "the
threshold is KNOWN to be zero", not "unknown". Core's
`effective_threshold()` returns `starting_threshold` verbatim when
set, so `frost_backup::recovery::find_valid_subset` gets a pinned
threshold of 0 and only tries zero-share subsets — it never runs
its threshold-inference mode (which only activates on `None`), so
`shared_key` stays `None`, `current_recovery` never populates, and
the Recover button never enables. With a real hint `Some(N)` the
pinned mode is correct, which is why that path works.

This also explains the earlier walkthrough failure ("recovered two
devices at threshold 2 but still can't recover") if that lobby was
created via "I don't know".

## Fix

Delete the wrapper at both sites — the field is already the right
type:

```rust
RecoveringAccessStructure::new(
    &shares,
    state.metadata.threshold_hint,
    fingerprint,
)
```

No other changes: the wire format already cooperates
(`SharePost::to_recover_share` deliberately sets
`held_share.threshold = None`, so nothing else pins the threshold,
and core's `or_else` fallback finds nothing and lets inference
run).

## Tests

- Unit (fold-level, in `lobby.rs` tests): fold Share messages into
  a state whose metadata has `threshold_hint: None` and assert
  `current_recovery` becomes `Some` once a reconstructing set is
  present — this is the regression the reviewers and existing
  suites missed (every existing case passes `Some(2)`).
- e2e (`tests/recovery_live.rs`): a lobby created with
  `threshold_hint: None` over MockRelay; participants post shares;
  assert `RecoveryAvailable` fires and leader `Finish` verifies —
  covering the second call site too.

## Acceptance

- Both new tests fail before the fix, pass after.
- `cargo test -p frostsnap_nostr` fully green; no FRB surface
  change (threshold_hint types are untouched).
- Manual (user): "I don't know" lobby recovers once threshold-many
  shares are posted.
