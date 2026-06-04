# tx-quotes-receive-share
# Incoming tx quotes its receive-address share message

## Problem

When a participant publishes a receive-address share into the
chat (kind 7800) and later money arrives at that address, the
chat shows two unrelated items:

- The receive-address bubble (with the memo "for the dinner thing")
- A separate `_TransactionCard` for the incoming tx

There's no visible link between them. The user has to mentally
correlate "yeah, I shared address #4 earlier, and now there's an
incoming tx" — which is exactly the context the memo was supposed
to provide.

## Goal

When a `TimelineTransaction` (kind = `mempool` or `confirmed`)
is rendered AND it pays to an address that was previously shared
via a `TimelineReceiveAddress`, the tx card displays a quote of
that receive-share message at its top — visually the same shape
as the chat reply quote (`_ChatReplyQuote`):

- Sender display name
- Memo (italic, truncated to 2 lines)
- Tap → `_scrollToAndHighlight(receiveMessageId)` (existing
  highlight mechanism used for chat replies)

This makes the connection between "we asked for money for X" and
"X just arrived" immediate.

## Data flow

### Reverse index by derivation index

Today `_receiveCardById: Map<EventId, ReceiveAddressCardModel>`
keys by event id. We add a parallel index:

```dart
final Map<int, EventId> _receiveCardByDerivationIndex = {};
```

Populated whenever a `ChannelEvent_ReceiveAddress` lands (next to
the existing `_receiveCardById[messageId] = card`). Keyed by
`card.derivationIndex`, value is the receive message's
`EventId`. Multiple share messages for the same index are rare;
last writer wins is fine (we quote the most recent share).

### Looking up the index for an incoming tx

`Transaction.isMine: Map<ScriptBuf, int>` already maps each of
**our** output scripts in this tx to the derivation index that
owns it. For an incoming tx the receive output is in `isMine`;
the value IS the derivation index. No extra wallet derivation
needed.

When a `TimelineTransaction` lands, compute the **first**
`isMine` value that's in `_receiveCardByDerivationIndex`; that
gives `EventId` for the quoted source. Wire it onto the timeline
item:

```dart
class TimelineTransaction extends TimelineItem {
  ...
  final EventId? quotedReceiveMessageId;
}
```

Set on construction at the two places that build
`TimelineTransaction` in `_applyTxState` (mempool +
confirmed arms).

### Rendering

`_TransactionCard` gets a `quotedReceiveMessageId` parameter (or
the resolved `ReceiveAddressCardModel` directly, passed by the
build site). When non-null, render a small quote header above
the tx body using the existing `_ChatReplyQuote` shape OR a new
sibling widget styled the same way.

Approach: factor `_ChatReplyQuote` so it can be reused for either
a `ChatMessage` reply OR a `ReceiveAddressCardModel` quote. Both
have author + a short body line. Cleanest: extract a parameterised
`_QuoteHeader(author, profile, body, onTap)` widget; let
`_ChatReplyQuote` and the new tx-card quote both call it. Old
behaviour preserved for chat replies.

Tap handler → `_scrollToAndHighlight(quotedReceiveMessageId)` —
existing path that already works for chat replies. The receive
bubble already gets a `GlobalKey` in `_timelineKeys` (added by
`TimelineReceiveAddress.buildBubble`), so the highlight will
land on it.

## Edge cases

- **Confirmed replaces mempool**: today the mempool entry is
  removed and a confirmed one inserted (`_applyTxState`
  cleanup). Both arms must compute the quote so it persists across
  the transition.
- **Outgoing tx with change to own address**: don't quote.
  Filter: only quote if the tx's `is_mine` outputs aren't fully
  matched by `is_mine` inputs (i.e., net positive to us). Simpler:
  check if `tx.netOwnedDelta() > 0` (an existing API), and only
  quote when true. If unclear, only quote `mempool` /
  `confirmed` tx items that have a positive net owned delta.
- **No matching share**: leave the tx card unchanged (no quote
  header). Existing render path.
- **Receive share arrives AFTER the tx**: unlikely (you share
  the address before sending it to someone), but if it happens
  the tx already in the timeline won't retroactively gain a
  quote. Acceptable.
- **Multiple shares of the same index**: last-writer-wins in the
  derivation-index map. The most recent share is quoted.

## Files

- `frostsnapp/lib/nostr_chat/chat_page.dart`:
  - `_receiveCardByDerivationIndex` field
  - Populate in the `ChannelEvent_ReceiveAddress` handler
  - `TimelineTransaction.quotedReceiveMessageId` field
  - `_applyTxState`: resolve the quote (mempool + confirmed
    arms)
  - Extract `_QuoteHeader` from `_ChatReplyQuote`
  - `_TransactionCard`: accept a quote header + author profile,
    render above the existing body when set
  - Build site passes the resolved `ReceiveAddressCardModel`
    (lookup by EventId in `_receiveCardById`) into the card

No Rust changes. No FRB regen.

## Verification

- `flutter analyze lib` clean
- `dart format --output=none --set-exit-if-changed` clean
- Manual:
  1. Share a receive address with a memo. Bubble appears.
  2. Send funds to that address from elsewhere.
  3. Mempool tx card appears, quotes the receive bubble at top,
     shows author + memo. Tap the quote → scrolls to and
     highlights the receive bubble.
  4. Same after confirmation: confirmed tx card also carries the
     quote.
  5. An outgoing tx (or a normal received tx to an
     unannounced address) renders without a quote header.

---

## Revision — addressing review feedback

### 1. API name: `tx.balanceDelta()`, not `tx.netOwnedDelta()`

The convention everywhere else in the codebase is
`tx.balanceDelta()` (`bitcoin.dart:113`, used by
`TxDetailsModel(tx).isSend` which evaluates
`(tx.balanceDelta() ?? 0) < 0`). The receive filter is
**`(tx.balanceDelta() ?? 0) > 0`** — net positive ownership
delta. Equivalent inverse of `TxDetailsModel.isSend`. Don't
invent a third spelling.

### 2. Single resolver helper, called from all three sites

`TimelineTransaction` is constructed in three places (not two):

- `_processEvent`, `needsBroadcast` arm (chat_page.dart:822)
- `_applyTxState`, mempool arm
- `_applyTxState`, confirmed arm

Don't copy-paste the lookup. Add one helper:

```dart
EventId? _resolveReceiveQuoteFor(Transaction tx) {
  if ((tx.balanceDelta() ?? 0) <= 0) return null;
  // Multi-output: pick the lowest matching derivation index
  // deterministically (Map iteration order is not user-meaningful).
  final mineByIndex = SplayTreeMap<int, EventId>();
  for (final entry in tx.isMine.entries) {
    final idx = entry.value;
    final eid = _receiveCardByDerivationIndex[idx];
    if (eid != null) mineByIndex[idx] = eid;
  }
  return mineByIndex.isEmpty ? null : mineByIndex.values.first;
}
```

All three call sites pass `quotedReceiveMessageId:
_resolveReceiveQuoteFor(tx)`. The `needsBroadcast` arm gets
`null` for free (the balanceDelta filter excludes outgoing-by-
construction), so no special-case needed.

### 3. Confirmed: quote only on mempool. Document the cost.

`TxTimelineKind.confirmed` renders `_TxConfirmedLine` — a
centered pill ("confirmed +0.001 BTC · 14:32"), not a card.
Stacking a memo+author quote header above a one-line pill is
incoherent: the quote would be wider and louder than the thing
it annotates.

**Decision: quote renders ONLY on `mempool`.** Once a tx
confirms, the line is intentionally minimal; the link back to
the share is lost from the UI. Rationale: once confirmed, the
relationship is mostly historical — the recipient saw the
mempool quote when it mattered.

If we later want a permanent link, the right move is a separate
plan to promote confirmed back to a card form (or a different
pill shape that can hold an attribution line). Out of scope here.

The `TimelineTransaction.quotedReceiveMessageId` field still
gets set for confirmed (so the data is preserved across the
mempool→confirmed transition), but only the `_TransactionCard`
render path consumes it. `_TxConfirmedLine` ignores it.

### 4. Multi-output: lowest matching derivation index

`isMine` is a `Map<ScriptBuf, int>` populated from Rust; Map
iteration order is not user-meaningful. For the rare case where
one tx pays two announced addresses (e.g., batch send to an org
with multiple shared receive memos), pick the **lowest matching
derivation index**:

- Stable across reruns
- Tends to correspond to the earliest-shared address, which is
  usually the longest-standing context
- Last-writer-wins on the index map handles "two memos for the
  same index" (already in the plan)

The single-resolver in (2) above uses a `SplayTreeMap` to enforce
this deterministically.

### 5. `_TransactionCard` layout

The card is currently `Row(Icon + Flexible(Column))` and sizes
to its content. Adding a quote header means wrapping that Row in
a `Column([_QuoteHeader, existingRow])`.

Two concerns:

- **Width**: a 2-line memo can be wider than the existing
  Row's intrinsic content. The card outer is `Align(centerLeft,
  child: Padding(...))`. The card already grows with content; the
  quote will just push that wider.
- **Quote width vs card body**: in the chat-bubble case, the
  reply quote sits flush-left inside the bubble's already-bounded
  width. The tx card has no explicit max width. To avoid the
  quote ballooning the card, give the new `_QuoteHeader` a
  `ConstrainedBox(maxWidth: 320)` when used inside the tx card,
  or wrap the whole `_TransactionCard` in
  `ConstrainedBox(maxWidth: <something reasonable>)`. Prefer the
  latter — bounding the card itself prevents future weirdness.

Pick `maxWidth: 360` for the tx card (matches the bubble's
narrow-end behaviour). The plan implementer should ship the
card-wrap, not a per-quote-header cap.

### Revised file-list / verification

Same as the original plan PLUS:

- New helper `_resolveReceiveQuoteFor(Transaction tx)` on
  `_ChatPageBodyState`.
- `_TransactionCard` wrapped in `ConstrainedBox(maxWidth: 360)`.
- `_TxConfirmedLine` left alone; receives no quote.
- Verification step 4 (confirmed carries the quote) is **removed**.
  Replaced with: "After confirmation, the tx becomes the standard
  minimal confirmed line; no quote header (intentional)."
