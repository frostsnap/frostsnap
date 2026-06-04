# receive-flow-attach-to-message
# Receive flow: attach address to chat message + verify-on-device

The current receive flow opens a modal dialog that asks for a memo,
copies the address, and publishes a receive-address event. This is
out of step with the rest of the chat-first UI:

- Signing proposals attach to the **next chat message** via the
  pending-attachment ribbon above the input — no dialog, no separate
  memo field.
- Once the dialog's memo field moves into the regular chat text
  input, the dialog has no content left to justify itself.
- There's no way to verify the receive address on a hardware device
  the way the local-receive page does.

## Proposed changes

### 1. No dialog — attach address as a pending chat attachment

Tapping `+` → **Receive Bitcoin** does NOT open a sheet. Instead:

- Compute the next unused address synchronously
  (`walletCtx.superWallet.nextAddress(masterAppkey: ...)`)
- Set a new pending field on `_ChatPageBodyState`:
  `_pendingReceiveAttachment: ({int index, String address})?`
- The attachment ribbon above the input bar shows
  `Address #N · bc1q…abcd` (first 6 / last 4 of the address) with a
  small `Icons.call_received_rounded` leading and a close button to
  cancel the attachment. Matches the existing
  `_pendingSignRequest` / `_pendingTxSignRequest` ribbons in
  shape and behaviour.
- The address text in the ribbon is **not** selectable or
  copyable (we don't want it leaving the device before being
  shared with peers).
- The text the user types in the chat input becomes the **memo**
  when they hit send.

### 2. Send merges the attachment with the typed text

`_sendMessage` already handles three pending kinds (chat content,
sign request, tx sign request). Extend it: when
`_pendingReceiveAttachment` is set,
call `_client.sendReceiveAddress(...)` with the typed text as
`memo`. If the input is empty, send an empty memo (the index +
address card alone is fine).

The optimistic `ChannelEvent::ReceiveAddress { pending: true }`
emitted by Rust still drives the chat card; nothing changes on the
Rust side.

### 3. Card is a deep-link to `ReceivePage`, not a self-contained UI

The chat receive card stops trying to be a full address surface
(copy button, inline verify button, etc.). Instead it's a compact
summary that **opens the existing `ReceivePage`**
(`wallet_receive.dart`) on tap — which already handles:

- Full address + QR
- Copy to clipboard
- **Verify on device** (`coord.verifyAddress`)
- Transactions associated with this specific address (via
  `txStream` filter)

`ReceivePage` already accepts an optional `derivationIndex`
parameter (`wallet_receive.dart:191`). We pass the card's index so
it jumps straight to that address instead of the next-unused one.

```dart
void _openReceivePage(ReceiveAddressCardModel card) async {
  final walletCtx = WalletContext.of(context);
  if (walletCtx == null) return;
  await MaybeFullscreenDialog.show(
    context: context,
    child: walletCtx.wrap(
      ReceivePage(
        wallet: walletCtx.wallet,
        txStream: walletCtx.txStream,
        derivationIndex: card.derivationIndex,
      ),
    ),
  );
}
```

The card itself shrinks to a compact deep-link:

- Sender row (avatar + name + "shared a receive address")
- `Address #N · bc1q…abcd` (truncated, not copyable from the card)
- Memo (if any, italic)
- Status row (pending / sent / failed + verification badge)
- Trailing chevron hinting it's tappable
- `onTap` only enabled when `status == sent` AND `verified`
  (don't deep-link to a pending or unverifiable address)

No copy button on the card. No inline verify button. Reuse the
existing receive page for the canonical surface.

### 4. Drop the "Copy & share" pre-send clipboard

The current sender flow copies the address to clipboard BEFORE
publishing. That was tied to the dialog UX. New behaviour:

- Pre-send: address only visible as the truncated ribbon preview,
  not copyable
- Post-send: tap the card → `ReceivePage` → copy there

This avoids the "I copied it but the message didn't actually send"
edge case AND removes duplicate copy affordances between the chat
card and the receive page.

## What stays the same

- All Rust changes from the previous impl: `Kind::Custom(7800)`
  wire format, `ChannelEvent::ReceiveAddress`,
  `ReceiveAddressSendFailed`, `ChannelHandle::send_receive_address`,
  `ChannelClient::send_receive_address`, optimistic
  `pending: true` lifecycle.
- Receiver-side `_verifyCard` + bounded auto-advance
  (`_receiveIndexLookahead = 100`).
- `ReceivePage` itself — no changes; it already takes
  `derivationIndex` and handles copy + verify + tx history.

## Files to change

- `frostsnapp/lib/nostr_chat/chat_page.dart`:
  - Add `_pendingReceiveAttachment` field
  - `_proposeReceiveAddress` → set the field, no dialog
  - Add attachment-ribbon block above the input (mirrors
    `_pendingSignRequest` / `_pendingTxSignRequest`)
  - Extend `_sendMessage` to consume the attachment
  - `_ReceiveAddressCard`: shrink to compact deep-link;
    `onTap` opens `ReceivePage` with the card's `derivationIndex`
  - Delete the dialog / memo `TextField` code in
    `_proposeReceiveAddress`
  - Drop the inline copy block and inline verify-button work
    that the previous impl had

No Rust changes. No FRB regen. No changes to
`frostsnapp/lib/wallet_receive.dart`.

## Open questions

### Q1: Show the address in the ribbon at all?

The user said "you should be able to see which address index it
is and a bit of the address but can't copy it until you've sent
the message." So yes — show index prominently + a truncated
preview (`bc1q…abcd`). Not selectable.

### Q2: What if the user types nothing and just hits send?

Send with empty memo. The chat card renders without the italic
memo line — same as a card whose author supplied no memo.

### Q3: Single-attachment invariant — sign / tx-sign / receive

Today `_pendingSignRequest` and `_pendingTxSignRequest` can stack
in the input ribbon (both render) and `_sendMessage` resolves
ambiguity by handling `_pendingTxSignRequest` first while clearing
both. That's a latent footgun.

Implementation requirement (binding, not "if we feel like it"):

1. The three pending attachments are **mutually exclusive**.
2. Every producer clears the other two before setting its own:
   - `_proposeTestSign` (existing) clears `_pendingTxSignRequest`
     AND `_pendingReceiveAttachment`
   - `_proposeSendBitcoin`'s `onTxReady` clears `_pendingSignRequest`
     AND `_pendingReceiveAttachment`
   - `_proposeReceiveAddress` clears `_pendingSignRequest` AND
     `_pendingTxSignRequest`
3. `hasAttachment` (already exists) is extended to include
   `_pendingReceiveAttachment != null`.
4. `_sendMessage` reads exactly ONE pending attachment kind at a
   time. Snapshot the active kind early in the function, then
   dispatch the matching send path. All three pending fields are
   cleared in the same `setState` as today's `_pendingSignRequest`
   / `_pendingTxSignRequest` reset.

This converts the current "stack but only handle one" behaviour
into the documented single-attachment invariant for all three
kinds.

### Q4: Where to derive the truncated preview?

`'${addr.substring(0, 6)}…${addr.substring(addr.length - 4)}'` is
fine for bech32. There's already `shortenPubkeyHex` in
`nostr_profile.dart`; lift the same shape into a helper if we
end up using it elsewhere.

## Imports

`chat_page.dart` needs additional imports for the new deep-link
path:

```dart
import 'package:frostsnap/maybe_fullscreen_dialog.dart';
import 'package:frostsnap/wallet_receive.dart';
```

## Verification

- `flutter analyze lib` clean
- `dart format --output=none --set-exit-if-changed` clean
- Manual: `+` → Receive Bitcoin → ribbon appears with
  `Address #N · bc1q…abcd` (truncated, not selectable)
- Manual: type a memo, hit send → optimistic card with memo +
  index; ribbon clears
- Manual: tap the X on the ribbon → attachment cancelled, no event
  published
- Manual: after the optimistic card is confirmed sent (status
  flips from pending → sent) AND verified, tapping the card opens
  `ReceivePage` for that derivation index. Copy and verify-on-
  device work from inside `ReceivePage`.
- Manual: pending or unverified card → `onTap` is a no-op
  (chevron faded or absent)
- Manual: invariant check — `_proposeTestSign` while a receive
  attachment is set clears the receive attachment; same for the
  reverse and for tx-sign. Only one ribbon is ever visible
  simultaneously.
