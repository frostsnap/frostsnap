# Receive address flow implementation

Implementation plan for the design in
`.clank/finished/receive-address-flow-design.md`. Follows the
design's flow and answers; decisions below where the design left
trade-offs open.

## Decisions from open questions

- **Q1 (Kind)**: new `Kind::Custom(7800)` with bincode payload —
  same pattern as `SigningMessage`.
- **Q2 (Verification location)**: **Dart side**. `frostsnap_nostr`
  doesn't currently hold a wallet descriptor; Dart already has
  `SuperWallet.getAddressInfo(masterAppkey, index)` FRB-exposed.
  Chat page verifies on render. Single UI consumer means
  centralisation isn't worth the new descriptor plumbing.
- **Q3 (Race)**: accept collision, surface both messages.
- **Q4 (Retract)**: no retract for v1.
- **Q5 (Display)**: `#42 · bc1q…` prominent, memo italic below.
- **Q6 (Lookahead)**: `RECEIVE_INDEX_LOOKAHEAD = 100`, named const
  in the chat page module.

## Files to change

1. `frostsnap_nostr/src/signing/events.rs`
2. `frostsnap_nostr/src/channel_runner.rs`
3. `frostsnap_nostr/src/lib.rs` (re-export wire payload)
4. `frostsnap_nostr/Cargo.toml` — no new deps
5. `frostsnapp/rust/src/api/nostr/mod.rs` — wire `ReceiveAddress`
   event into the existing `ChannelEvent` enum (or add new variant
   if mirrored)
6. `frostsnapp/lib/nostr_chat/chat_page.dart`
7. New: `frostsnapp/lib/nostr_chat/receive_address_card.dart`
8. `frostsnapp/lib/copy_feedback.dart` — no change (already have
   `copyToClipboard`)

No Rust core or coordinator changes — `mark_address_shared` is
already exposed via FRB (`super_wallet.rs:157`).

## Wire format

```rust
// frostsnap_nostr/src/signing/events.rs (or new module)
#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub struct ReceiveAddressPayload {
    pub derivation_index: u32,
    pub address: String,        // bech32, ready to display
    pub memo: String,           // empty if none
}
```

Custom kind: `Kind::Custom(7800)`.

Add to `ChannelEvent`:

- `ReceiveAddress { message_id, author, timestamp, pending,
    derivation_index, address, memo }` — emitted both for incoming
  remote events and for the local optimistic-send echo
- `ReceiveAddressSendFailed { message_id, reason }` — emitted when
  the relay-gated dispatch returns zero OKs; Dart flips the
  optimistic card into `failed-to-share` state by id match.

`MessageSent { message_id }` is reused for the success transition
(same as text chat) since the id is unique across event kinds.

## Sender flow

### Rust-side API

Mirror the existing `send_message` shape on `ChannelClient`:
`access_structure_id` (to resolve the handle) and `nsec` (to
parse signing keys) are required, same as every other send method.

```rust
// frostsnapp/rust/src/api/nostr/mod.rs
impl ChannelClient {
    pub async fn send_receive_address(
        &self,
        access_structure_id: AccessStructureId,
        nsec: String,
        derivation_index: u32,
        address: String,
        memo: String,
    ) -> Result<EventId> {
        let keys = Keys::parse(&nsec)?;
        let handle = self.get_handle(access_structure_id)?;
        handle.send_receive_address(
            &keys, derivation_index, address, memo,
        ).await
    }
}
```

### ChannelHandle command + signing task

`ChannelMessageDraft::app` and `dispatch_prepared` are
channel-runner internals — Dart never reaches them. The send goes
through the signing task's command channel, exactly like
`SendPreparedMessage` in `frostsnap_nostr/src/signing/mod.rs:109`:

```rust
// frostsnap_nostr/src/signing/mod.rs
enum ChannelCommand {
    SendPreparedMessage(Event),
    SendPreparedReceiveAddress(Event, ReceiveAddressPayload),  // NEW
}

// In ChannelHandle:
pub async fn send_receive_address(
    &self,
    keys: &Keys,
    derivation_index: u32,
    address: String,
    memo: String,
) -> Result<EventId> {
    let payload = ReceiveAddressPayload {
        derivation_index, address, memo,
    };
    let draft = ChannelMessageDraft::app(
        Kind::Custom(7800), &payload, vec![],
    )?;
    let prepared = draft.prepare(keys).await?;
    let id: EventId = prepared.id.into();
    self.cmd_tx
        .send(ChannelCommand::SendPreparedReceiveAddress(
            prepared, payload,
        ))
        .await?;
    Ok(id)
}
```

In the signing-task select-loop (`signing/mod.rs:106-138`), add an
arm for the new command — same shape as the existing
`SendPreparedMessage` arm, but emitting `ChannelEvent::ReceiveAddress`
optimistically and a new `ReceiveAddressSendFailed` on relay
failure:

```rust
Some(ChannelCommand::SendPreparedReceiveAddress(prepared, payload)) => {
    let message_id: EventId = prepared.id.into();
    // Optimistic local echo — pending=true
    sink.send(ChannelEvent::ReceiveAddress {
        message_id,
        author: prepared.pubkey.into(),
        timestamp: prepared.created_at.as_secs(),
        pending: true,
        derivation_index: payload.derivation_index,
        address: payload.address.clone(),
        memo: payload.memo.clone(),
    });
    match runner_handle_for_task.publish_prepared(prepared).await {
        Ok(outcome) if outcome.any_relay_success() => {
            sink.send(ChannelEvent::MessageSent { message_id });
        }
        Ok(outcome) => {
            sink.send(ChannelEvent::ReceiveAddressSendFailed {
                message_id,
                reason: format!("no relay accepted: {:?}", outcome.relay_failed),
            });
        }
        Err(e) => {
            sink.send(ChannelEvent::ReceiveAddressSendFailed {
                message_id, reason: e.to_string(),
            });
        }
    }
}
```

`MessageSent { id }` is reused for the success transition — same
event already exists; Dart flips the pending card by id match.

### Dart-side wiring

In `chat_page.dart`:

1. Add `Receive` tile to `_showActionMenu` (alongside Sign Message
   / Send Bitcoin) with `Icons.call_received_rounded`.
2. New `_proposeReceiveAddress()` opens a sheet via
   `showBottomSheetOrDialog`:
   - Calls `superWallet.nextAddress(masterAppkey)` for the
     preview address + derivation index
   - Optional memo `TextField`
   - **Cancel** / **Copy & share** buttons
3. On Copy & share:
   - `copyToClipboard(address)` (chip feedback)
   - Call
     `client.sendReceiveAddress(accessStructureId: ..., nsec: ...,
     derivationIndex: ..., address: ..., memo: ...)`. The `nsec`
     comes from `nostrSettings.getNsec()` same as other sends.
   - The future resolves with the prepared event id as soon as the
     command is queued. The optimistic `ChannelEvent::ReceiveAddress
     { pending: true }` was already emitted by the signing task at
     that point.
   - Listen for the matching `MessageSent { id }` (success) and
     `markAddressShared(masterAppkey, derivationIndex)` then.
     Best-effort — log on error.
   - If `ReceiveAddressSendFailed { id }` arrives instead, the
     pending card flips to `failed-to-share`. Retry button
     re-invokes `sendReceiveAddress` with the same payload (cursor
     never advanced because `markAddressShared` is gated on
     `MessageSent`).

## Receiver flow

In `chat_page.dart`'s event handler (the existing switch on
`ChannelEvent`):

1. New case `ChannelEvent_ReceiveAddress`. Append to
   `_messages` (or new `_receiveCards` list) keyed by `message_id`.
2. Verification (synchronous, on render):
   - `info = superWallet.getAddressInfo(masterAppkey,
     event.derivationIndex)`
   - If `info == null || info.address.toString() != event.address`
     → `verified = false`
   - Else → `verified = true`
3. If `verified`:
   - Compute `localNext = superWallet.nextAddress(...).index`
   - If `event.derivationIndex <= localNext +
     RECEIVE_INDEX_LOOKAHEAD` → call `markAddressShared(...)`
     once (track by `message_id` in a `Set<EventId>` to avoid
     repeating on rebuild)
   - Else → render the card with an "Apply anyway" button that
     does `markAddressShared` on tap

The verification is render-time cheap (a single FFI hop). To
avoid re-running on every rebuild, memoise per `message_id`:

```dart
final Map<EventId, _ReceiveCardState> _receiveCards = {};
```

Where `_ReceiveCardState` holds the resolved verification result
and whether `markAddressShared` was already called.

## Render

New `ReceiveAddressCard` widget in
`frostsnap_chat/receive_address_card.dart`:

```
┌──────────────────────────────────────────┐
│ ⊙ Alice  ·  9:42 AM                       │
│ shared a receive address                  │
│                                            │
│ #42                                        │
│ bc1qexample…  [copy]                       │
│                                            │
│ "for the dinner thing"                     │  ← italic memo
│                                            │
│ ✓ verified                                 │  ← green, or
│                                            │  ✗ does not match
└──────────────────────────────────────────┘
```

When `verified == true` but out-of-lookahead:

```
│ ⚠ verified, but index #42 is far ahead of │
│   your wallet (#3). [Apply anyway]         │
```

When `verified == false`: red error chip, no Apply Anyway button.

## State management

- `_messages` already exists; add a parallel `_receiveCards: Map<EventId, ReceiveCardModel>`
  for receive cards specifically (so chat list can interleave
  them with normal messages)
- OR add a `ReceiveAddressMessage` variant to whatever sealed
  union `_messages` uses — depends on existing chat model;
  inspect during impl
- Recommendation: extend existing chat message model with a
  variant. Renders inline in the chat scroll.

## Verification (post-impl)

- `cargo check --workspace` clean
- `flutter analyze lib` clean
- `just gen` after API change
- Manual: open `+` menu → tap Receive → sheet shows next-unused
  address + index, memo field, Copy & share
- Manual: tap Copy & share → address copied (chip), message
  appears in chat, address marked shared (verify by tapping
  Receive again — index increments)
- Manual sad path: kill relay before share → message renders
  failed-to-share with Retry, local index NOT advanced
- Manual receiver: peer publishes a receive-address message →
  card appears, verifies locally, marked shared on this device
- Manual receiver tamper: edit the wire payload to claim a
  mismatched address → red error chip, NOT marked shared
- Manual receiver bound: peer publishes index `N + 200` (where
  local next is N) → card renders "Apply anyway", NOT
  automatically applied

## Out of scope

- BIP-21 URIs (no amount/label)
- Per-counterparty stealth/isolation
- Sender-side retraction
- Push notifications
