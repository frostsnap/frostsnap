# use-first-seen-for-timeline
# Use BDK `first_seen` for timeline ordering

## Context

`runner-emits-tx-correlation-hints` shipped a chat timeline that
orders wallet-observed tx events by `last_seen` (for Mempool kind)
and `confirmation_time.time` (for Confirmed kind). Two real bugs:

1. **`last_seen` shifts forward on re-observation.** BDK updates
   it on every unconfirmed sighting. Using it as a timeline anchor
   means the Mempool item moves later in the timeline every time
   the wallet pings the network. The chat moment is "when we first
   learned" — that's `first_seen`, not `last_seen`.

2. **`confirmation_time.time` is miner-controlled** with ±2h fudge.
   Confirmed pills can sort EARLIER than the Mempool entries they
   confirm. Real screenshot: receive-share at 16:11, confirmed pill
   at 16:16, mempool "Receiving" card at 16:18 — confirmed appears
   to happen before mempool.

The model needs the first-observation moment for both kinds, with
`confirmation_time.time` only used when it's later than that.

## Changes

### `frostsnap_coordinator::wallet::Transaction`

Add a `first_seen: Option<u64>` field. Populated from
`canonical_tx.tx_node.first_seen` in `build_transaction`. The
existing `last_seen: Option<u64>` STAYS — useful for future UI
(e.g., "tx still seen at relay 12 min ago" mempool-freshness
indicator).

### App FRB `Transaction`

Mirror the new field. `From<wallet::Transaction> for Transaction`
populates it.

### `frostsnap_nostr` runner

`ObservedTx.last_seen` field renamed to `first_seen` (still
`Option<u64>`). Sourced from the WalletTx's `first_seen`.

`handle_notify_tx_observed` emits:
- **Mempool**: only when `first_seen.is_some()` AND
  `mempool_emitted == false`. Timestamp = `first_seen.unwrap()`.
  (A tx the wallet only ever observed as confirmed — `first_seen`
  is None — gets no Mempool emit. No chat moment for it being
  unconfirmed because we never saw it that way.)
- **Confirmed**: when `confirmation_time.is_some()` AND
  `confirmation_emitted == false`. Timestamp = `max(block.time,
  first_seen.unwrap_or(0) + 1)`. The +1 guarantees the confirmed
  pill sorts after the mempool entry when both exist; the `max`
  uses block.time as the floor (normal case where tx hit mempool
  then was confirmed in a later block).

### App-FRB `NostrClient::notify_tx_observed`

The `WalletTx` reconstruction inside the FRB method already
clones the chain-state fields — extend to also clone `first_seen`.

## Files

- `frostsnap_coordinator/src/bitcoin/wallet.rs`: add `first_seen`
  field to `Transaction`. Populate in `build_transaction`.
- `frostsnap_coordinator/src/bitcoin/wallet.rs` — sanity-check
  other constructors of `Transaction` for the new field.
- `frostsnapp/rust/src/api/bitcoin.rs`: add `pub first_seen:
  Option<u64>` to FRB `Transaction`. Update all construction
  sites:
  - `Transaction::from_template` (set `first_seen: None`).
  - `From<wallet::Transaction> for Transaction` impl (read from
    the source).
- `frostsnapp/rust/src/api/signing.rs`: two other `Transaction`
  struct literals also need the new field:
  - `WireSignTaskExt::signing_details` arm for
    `WireSignTask::BitcoinTransaction(tx_temp)` — set
    `first_seen: None` (template-built display tx, never observed).
  - `UnsignedTx::details(...)` — same, `first_seen: None`.
- `frostsnap_nostr/src/signing/mod.rs`:
  - `ObservedTx.last_seen` → `first_seen`.
  - `handle_notify_tx_observed` reads `tx.first_seen` instead of
    `tx.last_seen` for the Mempool emission.
  - Confirmed timestamp uses `max(confirmation_time.time,
    first_seen.unwrap_or(0) + 1)`.
- `frostsnapp/rust/src/api/nostr/mod.rs`:
  `NostrClient::notify_tx_observed` clones `first_seen` into the
  WalletTx it builds for the channel handle.
- `frostsnapp/lib/nostr_chat/chat_page.dart`: no code changes —
  the Dart side already takes `timestamp: u64` from the event.
- `just gen` after Rust changes.

## Verification

- `cargo check --workspace` clean.
- `flutter analyze lib` clean.
- Manual:
  1. The screenshot scenario: share an address, send funds, watch
     it confirm. Mempool card appears at the moment we first saw
     the tx (NOT moving forward on re-observation). Confirmed pill
     appears no earlier than the mempool card, even when block.time
     is fudged earlier.
  2. Restart the app — mempool card timestamp stays put (doesn't
     drift to a new `last_seen`).
  3. A tx we only ever observed as confirmed (e.g., synced after
     it was already in a block) gets only a Confirmed pill, no
     Mempool card. Confirmed timestamp = block.time.

## Out of scope

- The future "mempool-freshness" UI that displays `last_seen`
  ("last relayed N min ago"). `last_seen` is kept on the
  `Transaction` struct for that purpose but not used in this plan.
- Replacing the separate Confirmed pill with an in-place patch on
  the Mempool card. Considered, deferred — the user wanted the
  chat-moment artifact for confirmation. With first_seen + max()
  the ordering is no longer broken.
- Eviction handling (tx vanishes from snapshot). Not handled
  today either.
