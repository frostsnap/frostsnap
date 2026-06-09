# runner-emits-tx-correlation-hints
# Runner emits tx-correlation hints (txid + chat context)

## Context

`tx-quotes-receive-share` shipped a Dart-side correlation: when an
incoming tx pays to an address that was previously announced via a
`ReceiveAddress` chat message, the mempool tx card carries a quote
header pointing at that message.

The implementation uses `_resolveReceiveQuoteFor` which walks
`_timeline.reversed` to resolve the quote at tx-insertion time —
**timeline list as substitute for fold state**. Couples rendering
order to lookup correctness, makes the chat timeline an ad-hoc
merge of two event streams (nostr channel events + wallet tx
state) maintained entirely on the Dart side.

Move the merge onto the channel runner. The runner already folds
nostr events; extend it to also fold wallet-tx observations (Dart
pumps them in) and emit correlation hints. Dart just consumes the
hints and renders.

**Chronology, not processing order.** Correlations only fire when
the chat message announcing the address (real-world chronology)
preceded the tx arriving. If the tx arrived before the share
message existed, no quote — there's nothing to retroactively
point to. The runner resolves correlations at the moment of the
tx observation; it doesn't back-patch already-emitted observations
when later announcements arrive.

## Shape

The runner emits **identifiers + chat-side correlations only** —
not tx data. Dart looks up tx data on demand via a small FFI
call.

```rust
ChannelEvent::TxObservation {
    txid: String,
    kind: ObservationKind,
    /// For ordering in the chat timeline. `last_seen` for Mempool,
    /// `confirmation_time.time` for Confirmed.
    timestamp: u64,
    address_reveal_event: Option<EventId>,
    signing_start_event: Option<EventId>,
}

enum ObservationKind { Mempool, Confirmed }
```

No cross-crate tx-type issues. No bespoke FRB wrapper enum. The
auto-mirror keeps working — the variant carries only primitives
+ existing EventId.

## Runner state

```rust
struct ActivityState {
    /// Derivation index → the receive-address message id that
    /// announced it. Folded from `ChannelEvent::ReceiveAddress`.
    address_announcements: BTreeMap<u32, EventId>,

    /// Txid → the chat `SigningEvent::Request` event id that
    /// proposed signing this tx. Folded from request events whose
    /// `sign_task` is `WireSignTask::BitcoinTransaction`. No
    /// entry for txs signed outside chat.
    signing_starts_by_txid: BTreeMap<String, EventId>,

    /// Txid → observation snapshot from the wallet stream.
    /// Populated by `notify_tx_observed` calls from Dart.
    observed_txs: BTreeMap<String, ObservedTx>,
}

struct ObservedTx {
    mine_output_indices: Vec<u32>,
    last_seen: Option<u64>,
    confirmation_time: Option<u64>,
    /// Resolved correlations, set once at first notify_tx_observed
    /// and never updated. If the announcement wasn't known yet,
    /// these stay None — no back-patching.
    address_reveal_event: Option<EventId>,
    signing_start_event: Option<EventId>,
    /// Set once we've emitted `kind: Mempool` for this tx so
    /// re-notifies don't re-emit duplicate Mempool events.
    mempool_emitted: bool,
    /// Set once we've emitted `kind: Confirmed` for this tx so
    /// re-folds don't re-emit pills.
    confirmation_emitted: bool,
}
```

The nostr-derived maps (`address_announcements`,
`signing_starts_by_txid`) are replay-derived — same on every
participant from the same nostr stream. `observed_txs` is driven
by local wallet snapshots and isn't bitwise-identical across
participants per-snap. Convergent in the limit. Not persisted —
recreated on runner restart.

## Bridge from BDK

### Pump direction (Dart → runner)

Pass `frostsnap_coordinator::bitcoin::wallet::Transaction`
("WalletTx") through the runner. `frostsnap_nostr` already
depends on `frostsnap_coordinator`. Embracing WalletTx as the
canonical bitcoin-tx type at upstream boundaries (rather than
extracting primitives) lets the wallet-tx model push out the
other tx representations over time.

```rust
// frostsnap_nostr - ChannelHandle
impl ChannelHandle {
    pub async fn notify_tx_observed(
        &self,
        tx: frostsnap_coordinator::bitcoin::wallet::Transaction,
    ) -> Result<()>;
}

// frostsnapp/rust/src/api/nostr/mod.rs
impl NostrClient {
    pub async fn notify_tx_observed(
        &self,
        access_structure_id: AccessStructureId,
        tx: Transaction,                    // app FRB opaque
    ) -> Result<()>;
}
```

The FRB layer builds a `WalletTx` from the app `Transaction`
fields (today's app `Transaction` has nearly the same shape:
`inner: bitcoin::Transaction` vs WalletTx's
`inner: Arc<bitcoin::Transaction>`, etc. — wrap inner in Arc,
recompute `txid` via `inner.compute_txid()`, clone the rest):

```rust
let wallet_tx = WalletTx {
    inner: std::sync::Arc::new(tx.inner.clone()),
    txid: tx.inner.compute_txid(),
    confirmation_time: tx.confirmation_time.clone(),
    last_seen: tx.last_seen,
    prevouts: tx.prevouts.clone(),
    is_mine: tx.is_mine.clone(),
};
```

The runner extracts what it needs internally from the WalletTx:
- `tx.txid` for keying
- mine-output derivation indices: iterate `tx.inner.output`, look
  up each output's `script_pubkey` against `tx.is_mine`. (NOT
  `tx.is_mine` values directly — that map mixes owned-output
  and owned-input-prevout scripts.)
- `tx.last_seen` (presence)
- `tx.confirmation_time.as_ref().map(|c| c.time)`

### Lookup direction (Dart pulls when rendering)

Add a new FRB sync method to `SuperWallet`:

```rust
// frostsnapp/rust/src/api/super_wallet.rs
#[frb(sync)]
pub fn get_tx(&self, master_appkey: MasterAppkey, txid: String) -> Option<Transaction>;
```

Implementation:
- Parse `txid: String` → `bitcoin::Txid`.
- In the coordinator (`frostsnap_coordinator::bitcoin::wallet`),
  add `get_transaction(master_appkey, txid) -> Option<Transaction>`
  that does the same per-tx build that `list_transactions`'s
  filter_map does (prevouts + is_mine + confirmation + last_seen).
  Or inline it in the app `SuperWallet::get_tx`.
- Return as the app `Transaction` opaque.

Cheap — single tx lookup, no full snapshot walk.

### Snapshot pump

`SuperWallet::sub_tx_state` still emits full `TxState` snapshots
(unchanged). On each snapshot, Dart iterates per tx and calls
`_client.notifyTxObserved(accessStructureId, tx)`. Runner no-ops
when nothing changed.

Eviction (tx vanishes from snapshot) isn't handled today either;
punt.

## Fold mechanics

Three inputs converge on `observed_txs`.

**1. `notify_tx_observed(WalletTx)`** — only emission source for
`TxObservation`:
- Compute mine-output indices from the WalletTx (iterate
  `tx.inner.output`, look up scripts against `tx.is_mine`).
- Look up `address_announcements` for first matching mine index →
  `address_reveal_event` (lowest index for determinism). If
  unmatched, stays `None` forever for this tx.
- Look up `signing_starts_by_txid[txid]` → `signing_start_event`.
- Upsert `observed_txs[txid]`. Resolved correlation fields are
  set ONCE on first insert and never updated by later notifies.
- Emit:
  - `TxObservation { kind: Mempool, timestamp: last_seen.unwrap(), ... }`
    if `last_seen.is_some()` AND `mempool_emitted == false`. Set
    `mempool_emitted = true`.
  - `TxObservation { kind: Confirmed, timestamp: confirmation_time.unwrap(), ... }`
    if `confirmation_time.is_some()` AND `confirmation_emitted ==
    false`. Set `confirmation_emitted = true`.

(A tx observed already-confirmed by the wallet — e.g., on
startup-sync — may never get a Mempool emit. That's correct: no
mempool moment occurred in the user's session.)

**2. `ReceiveAddress` event** (extends existing handler):
- Insert into `address_announcements`. That's it. Future
  `notify_tx_observed` calls will resolve correlations against
  the updated map.
- **No walk over `observed_txs`. No back-patching.** Real-world
  chronology says: if a tx was already observed before this
  announcement, it can't quote it.

**3. `SigningEvent::Request` event** (extends existing handler):
- When `sign_task` is `WireSignTask::BitcoinTransaction`, derive
  the txid from the template, insert
  `signing_starts_by_txid[txid] = request_event_id`.
- **No walk. No back-patching.** Same chronology rule.

### Startup-time interleave

Because there's no back-patching, processing order at startup
matters: if the wallet snapshot pumps a tx BEFORE the channel
runner has folded the cached `ReceiveAddress` events (because
the cache replay happens in parallel-ish), correlations get
missed even though real-world chronology says they shouldn't.

**For this plan: accept that risk and see how it behaves in
practice.** The cache-replay phase in the runner
(`channel_runner.rs:274`) is local lmdb-only — should be fast
relative to the wallet snapshot pump. If startup misses become
a real problem, follow-up: add a `ChannelEvent::CacheReplayComplete`
signal from the runner and defer `notify_tx_observed` in Dart
until it arrives.

All updates idempotent.

## Dart side

### Event handling

- `_handleTxState`: iterate snapshot, per tx call
  `_client.notifyTxObserved(accessStructureId, tx)`. **Drop the
  existing `_applyTxState` diff logic** — runner does it.
- `_txTimelineState: Map<String, TxTimelineKind>` — **remove**.
  Per-tx state lives on the runner.
- `_resolveReceiveQuoteFor`, `_receiveCardByDerivationIndex` —
  **remove**. Runner does correlation.
- New `TimelineTxObservation extends TimelineItem` (replaces
  `TimelineTransaction` for the wallet-observed cases). Carries:
  - `txid: String`
  - `kind: ObservationKind`
  - `timestamp: DateTime` (from event's `timestamp_secs`)
  - `addressRevealEvent: EventId?`
  - `signingStartEvent: EventId?`
- New `ChannelEvent_TxObservation` handler in `_processEvent`:
  - Maintain
    `_txItemByKey: Map<(String txid, ObservationKind), TimelineTxObservation>`.
    Same `(txid, kind)` → patch existing item's correlation
    fields (and re-sort if timestamp changed — shouldn't, but
    defensive). New `(txid, kind)` → insert at the event's
    timestamp.
  - **On the first Mempool observation for a `txid`, remove any
    `TimelineSignedTxReady` (the pre-broadcast card) for that
    txid.** Today's `_applyTxState` does this via
    `_removeTxTimelineItem(txid, TxTimelineKind.needsBroadcast)`;
    we preserve the behavior under the new event shape.

### Mempool ↔ Confirmed: coexistence

Both can exist simultaneously in the timeline:

- The Mempool item sits at the time the wallet first saw the tx
  (with its quote header, if any).
- The Confirmed pill is a separate timeline entry at
  confirmation time.

This is the UX shift from today (where mempool gets replaced by
confirmed). Rationale: the mempool moment is a chat artifact (it
carries the "received money for the dinner share" context); the
confirmed pill is a later "+0.001 BTC confirmed" indicator.
Removing the mempool card on confirmation would lose the chat
moment.

No removal between the two. Each lives at its own timestamp.

### Rendering — `getTx` lookup at build

- `_TransactionCard` calls
  `walletCtx.superWallet.getTx(masterAppkey, txid)` at build time
  to fetch the opaque `Transaction`. Renders
  `tx.balanceDelta()`, `tx.recipients()`, fee, time, etc.
- **Null case**: render a minimal placeholder card showing the
  txid prefix + a "syncing…" subtitle, sized to roughly match the
  filled card so the timeline doesn't jump on rebuild. The
  parent `StatefulWidget` already rebuilds on each `_handleTxState`
  snapshot tick — at the next wallet update the lookup will
  succeed and the card fills in.
- Quote header when `addressRevealEvent` is set — same
  `_QuoteHeader` widget as today, source the receive-address
  message from `_receiveCardById[addressRevealEvent]`.
- `signingStartEvent` link — display polish; can be deferred to
  follow-up.

### Per-build FFI cost

`getTx` runs on every `_TransactionCard` build. For a 50-item
chat scroll, that's 50 sync FFI calls per frame. Each is cheap
(coordinator HashMap lookup + small build), but the aggregate
could matter on lower-end devices.

For initial impl: **just call getTx per build**. If profiling
shows it, add a `Map<txid, Transaction>` cache on chat state
populated by the same `_handleTxState` loop that pumps
`notifyTxObserved` — single point of cache invalidation. Don't
pre-optimize.

### What goes away from today's `TimelineTransaction`

- The `kind: needsBroadcast` case → renamed/migrated to its own
  Dart-side type `TimelineSignedTxReady` (or kept as a restricted
  `TimelineTransaction` variant — name TBD during impl).
- The mempool/confirmed cases → replaced by `TimelineTxObservation`.
- `quotedReceiveMessageId` field — replaced by
  `addressRevealEvent` resolved by the runner.

## Files

### Rust

- `frostsnap_nostr/src/signing/events.rs`:
  - New `ObservationKind` enum.
  - New `ChannelEvent::TxObservation` variant carrying
    `txid: String`, `kind`, `timestamp: u64`,
    `address_reveal_event: Option<EventId>`,
    `signing_start_event: Option<EventId>`.
- `frostsnap_nostr/src/signing/mod.rs`:
  - Extend runner state with `ActivityState`.
  - New `ChannelCommand::NotifyTxObserved(WalletTx)` + handler
    that extracts mine-output indices, last_seen,
    confirmation_time from the WalletTx, runs the fold, and
    emits `TxObservation`.
  - Extend `ReceiveAddress` and `SigningEvent::Request`
    handlers to update `address_announcements` /
    `signing_starts_by_txid` for future lookups. No walk over
    `observed_txs`, no back-patching (chronology-only).
- `frostsnap_nostr/src/signing/mod.rs` (`ChannelHandle`):
  - `pub async fn notify_tx_observed(&self, tx: frostsnap_coordinator::bitcoin::wallet::Transaction) -> Result<()>`.
    Enqueues the command. WalletTx is the canonical bitcoin-tx
    type at the boundary.
- `frostsnap_coordinator/src/bitcoin/wallet.rs`:
  - **Add** `get_transaction(master_appkey, txid: bitcoin::Txid) -> Option<Transaction>`
    (verified missing — `get_tx` at `wallet.rs:107` returns
    `Arc<bitcoin::Transaction>`, not the wallet-decorated
    `wallet::Transaction`). Extract the per-tx build logic from
    `list_transactions`'s filter_map.
- `frostsnapp/rust/src/api/super_wallet.rs`:
  - `SuperWallet::get_tx(master_appkey, txid: String) -> Option<crate::api::bitcoin::Transaction>`.
    Returns the FRB-opaque app `Transaction` (with the
    balanceDelta/recipients/fee methods), built from the
    coordinator's `wallet::Transaction` via the existing
    `From<wallet::Transaction> for Transaction` impl at
    `bitcoin.rs:513`.
- `frostsnapp/rust/src/api/nostr/mod.rs`:
  - `NostrClient::notify_tx_observed(access_structure_id, tx: Transaction)`.
    Builds a `WalletTx` from the app `Transaction` fields (wrap
    `inner` in `Arc`, recompute `txid` via
    `inner.compute_txid()`, clone the chain-state fields) and
    forwards to `ChannelHandle::notify_tx_observed`.
  - Mirror updates: `ObservationKind` enum, new
    `ChannelEvent::TxObservation` variant. Existing
    `#[frb(mirror(ChannelEvent), non_opaque)]` auto-mirror picks
    them up — no bespoke wrapper enum needed.

### Dart

- `frostsnapp/lib/nostr_chat/chat_page.dart`:
  - `_handleTxState`: per-tx call to
    `_client.notifyTxObserved(...)`. Drop the diff logic.
  - Drop `_resolveReceiveQuoteFor`,
    `_receiveCardByDerivationIndex`, `_txTimelineState`,
    `TxTimelineKind`, the receive-quote resolution at
    `TimelineTransaction` construction time.
  - The old `TimelineTransaction` for wallet-observed cases
    becomes `TimelineTxObservation`. Existing `TimelineTransaction`
    for the local pre-broadcast (`TxTimelineKind.needsBroadcast`)
    case becomes `TimelineSignedTxReady` (or keep
    `TimelineTransaction` restricted to that single case, name
    TBD during impl).
  - New `ChannelEvent_TxObservation` handler: upsert by
    `(txid, kind)`.
  - Renderer for `TimelineTxObservation` fetches the opaque
    `Transaction` via `walletCtx.superWallet.getTx(...)` at build
    time and reads display fields off it.

### FRB

- `just gen` after API changes.

## Out of scope

- Persisting `ActivityState` across runner restarts. Recreated
  via event replay + wallet re-snapshots.
- Wiring wallet stream into Rust without Dart in the middle.
  Both `SuperWallet` and `ChannelClient` live in `FfiCoordinator`;
  a direct Rust hookup is possible later. Dart pipe is simpler.
- The signing-card → broadcast-button flow. Stays Dart-side as
  `TimelineSignedTxReady` (or current name). The new
  `TxObservation` event handles wallet-observed lifecycle only;
  the link from a broadcasted tx back to its original sign
  request is via `signing_start_event` on the `TxObservation`.
- Type cleanup of app `Transaction` / `UnsignedTx` / `SignedTx` /
  `UnbroadcastedTx`. The previous discussion identified real
  duplication and category errors there, but this plan doesn't
  need any of it. Tracked as a future cleanup.
- Mempool eviction. Not handled today either.

## Verification

- `cargo check --workspace` clean.
- `flutter analyze lib` clean.
- `just gen` clean.
- Manual:
  1. Share a receive address, send funds to it. Mempool tx card
     gets the quote header.
  2. **Send funds to a fresh address FIRST, then share that
     derivation index in chat.** Mempool tx card has NO quote
     (chronology: tx existed before the announcement, can't
     point to something that came later).
  3. Sign a tx in chat → after wallet observes it, the resulting
     `TimelineTxObservation` has `signing_start_event` set
     (linkable to the original sign-request bubble).
  4. Confirmed pill shows up at the right time as its own
     timeline entry (mempool card also stays where it was —
     coexistence, no replacement).
  5. Outgoing tx to non-announced address renders without a
     quote.
  6. Restart the app with a chat that has both cached receive-
     share messages AND cached wallet txs. **Observe whether
     correlations resolve on the cached data.** This is not a
     pass/fail check — the plan accepts startup-interleave
     misses as a known risk. If misses are frequent enough to be
     user-visible, follow-up plan adds `CacheReplayComplete` +
     Dart-side notify deferral.
