# chat-bubble-as-shape
# Chat bubble as the canonical chat-shape

## Problem

The chat timeline mixes two visual shapes:

- **Chat bubbles** (`_MessageBubble`) — colored by author (me /
  not-me), aligned right/left, with a tail timestamp and status
  tick, hover actions, reply quote, failed-state retry.
- **Free-form cards** — `_ReceiveAddressCard`, `_TransactionCard`,
  signing cards, `_SigningCompleteCard`. Each rolls its own
  alignment, colors, spacing, and status presentation.

For events that are conceptually "a message from a person with
something attached" (e.g. a receive-address share, future media
attachments, payment requests), this means every new kind
reinvents the bubble chrome AND drifts from the chat baseline.
The receive card is currently the obvious offender — it doesn't
look or behave like a chat message even though semantically it
IS one (it's a thing a participant said, with attached structured
data).

There's also no compile-time guard that says "this timeline
variant must render as a chat bubble." A new variant could be
added that's *supposed* to be a chat message but ends up looking
like a free-form card again.

## Goal

1. Introduce a single canonical `ChatBubble` widget that owns ALL
   the bubble chrome: colors (`primaryContainer` /
   `surfaceContainerHighest` / `errorContainer`), alignment,
   max-width, padding, author label, reply quote, timestamp +
   status indicator, hover actions, failed retry, mobile
   long-press menu.
2. Refactor `_MessageBubble` to delegate to it (chat text becomes
   the simplest possible bubble: no attachment, just text +
   chrome).
3. Refactor `_ReceiveAddressCard` to render INSIDE a `ChatBubble`:
   the address preview is the **attachment slot**, the memo is
   the **text slot**, and "Apply anyway" / "Retry" sit in the
   **footer slot**.
4. Use Dart's sealed-class story to force every chat-shaped
   timeline variant to produce a `ChatBubble`. Non-chat variants
   (signing cards, tx cards) stay free-form on a sibling type.

## Proposed shape

### `ChatBubble` widget

A single content-agnostic widget that takes a small data bundle
and renders the canonical bubble:

```dart
class ChatBubble extends StatefulWidget {
  /// Sender + timing metadata used to colour, align, label the
  /// bubble — and decide which side the avatar sits on.
  final PublicKey author;
  final NostrProfile? authorProfile;
  final bool isMe;
  final DateTime timestamp;
  final MessageStatus status;
  final ReplyTarget? replyQuote;  // optional, rendered inside

  /// Optional attachment panel above the text (e.g. a receive
  /// address preview, a payment-request card). Rendered in a
  /// nested rounded panel that contrasts with the bubble surface
  /// (matches the WhatsApp/Signal photo-with-caption pattern).
  final Widget? attachment;

  /// Body text (e.g. a chat message or the memo accompanying an
  /// attachment). Empty string is valid — bubble shrinks.
  final String text;

  /// Optional action buttons rendered in a footer row below the
  /// text. Reserved for attachment-specific actions like
  /// "Apply anyway" or "Verify on device".
  final List<Widget> actions;

  /// Bubble-level tap (e.g. deep-link from a receive card into
  /// `ReceivePage`). When null, the bubble is not tappable.
  final VoidCallback? onTap;

  /// Standard hover actions exposed by every chat bubble. The
  /// widget renders them itself; the caller just provides the
  /// handlers it actually has (others are skipped).
  final VoidCallback? onReply;
  final VoidCallback? onCopy;
  final VoidCallback? onRetry;
  final VoidCallback? onTapAvatar;
  final VoidCallback? onTapQuote;

  /// Highlight when this bubble is the scroll target.
  final bool isHighlighted;
}
```

The widget renders:

- Outer `Align(centerRight / centerLeft)`
- Avatar (non-me only) on the leading edge
- Hover-action rail (copy / reply / retry) on the trailing edge
- Bubble container with `isFailed ? errorContainer : isMe ?
  primaryContainer : surfaceContainerHighest`
- Inside the bubble:
  - Author label (non-me only)
  - Reply quote (if any)
  - **Attachment panel** (if `attachment != null`) — nested
    rounded container with `surface` color, distinct from bubble
    background, with bounded width
  - Text row with `text` + timestamp + status tick (the canonical
    chat-message layout — preserved from `_MessageBubble`)
  - **Footer row** (if `actions.isNotEmpty`) — small chip-row of
    actions
  - "Tap to retry" hint when failed

Tap behaviour: `onTap` fires only when set AND status == sent.
Otherwise `onTap` falls back to `onRetry` for failed bubbles
(matching today's `_MessageBubble.build`).

### Sealed type that forces the shape

```dart
sealed class TimelineItem {
  DateTime get timestamp;
}

/// Sub-sealed class for timeline items that render as chat
/// bubbles. Subclasses MUST override `buildBubble`; the timeline
/// switch can rely on this and the compiler will catch any new
/// chat-shaped variant that forgets it.
sealed class ChatBubbleItem extends TimelineItem {
  ChatBubble buildBubble(BuildContext context, ChatBubbleHandlers h);
}

class TimelineChat extends ChatBubbleItem { ... }
class TimelineReceiveAddress extends ChatBubbleItem { ... }

// Non-chat-shape variants stay direct children of TimelineItem:
class TimelineSigning extends TimelineItem { ... }
class TimelineSigningComplete extends TimelineItem { ... }
class TimelineTransaction extends TimelineItem { ... }
class TimelineError extends TimelineItem { ... }
```

`ChatBubbleHandlers` is a small record holding the chat-level
callbacks the host provides (reply, copy, retry, scroll-to-quote,
tap-avatar, deep-link). Each variant builds the bubble from its
own data plus those handlers.

### What each chat-shape variant renders

- `TimelineChat.buildBubble` → `ChatBubble(text: message.content,
  attachment: null, actions: [])` — the simplest possible bubble,
  the chat-text case
- `TimelineReceiveAddress.buildBubble` →
  `ChatBubble(text: card.memo, attachment: ReceiveAttachment(...),
  actions: outOfWindow ? [ApplyAnywayButton] : [], onTap:
  canOpen ? () => openReceivePage : null)`

The receive variant's attachment is a small private widget that
renders index + truncated address + verification badge. It has
zero responsibility for bubble colors, alignment, status, retry,
etc. — that all lives in `ChatBubble`.

### Timeline switch

The switch in `_buildTimeline` collapses to two arms for chat
content:

```dart
final child = switch (item) {
  ChatBubbleItem b => b.buildBubble(context, handlers),
  TimelineSigning ... => _buildSigningCard(...),
  TimelineSigningComplete(:final requestState) => ...,
  TimelineTransaction(...) => ...,
  TimelineError ... => ...,
};
```

The compiler now refuses any new `ChatBubbleItem` subclass that
doesn't implement `buildBubble` (sealed class with abstract
method), and any new sibling under `TimelineItem` not listed in
the switch fails exhaustiveness.

## Migration steps

1. Add `ChatBubble` widget and `ChatBubbleHandlers` record.
2. Add `sealed class ChatBubbleItem extends TimelineItem` with
   abstract `buildBubble`.
3. Move `TimelineChat` under `ChatBubbleItem`; implement
   `buildBubble` returning a `ChatBubble` with `text`, no
   attachment, no actions. Delete `_MessageBubble` (its logic
   moves into `ChatBubble`).
4. Move `TimelineReceiveAddress` under `ChatBubbleItem`; implement
   `buildBubble` with a `ReceiveAttachment` widget for the slot,
   memo as text, retry/apply-anyway in footer slot, `onTap`
   wired to open `ReceivePage` when sent+verified. Delete
   `_ReceiveAddressCard` and `_ReceiveStatusRow`.
5. Verify hover-actions, mobile long-press, scroll highlight, and
   reply-quote behaviour still work for both chat and receive
   bubbles (since both flow through the same widget now).
6. The switch in `_buildTimeline` collapses chat + receive into a
   single `ChatBubbleItem` arm.

## Files

- `frostsnapp/lib/nostr_chat/chat_page.dart` — almost all of the
  change happens here. New `ChatBubble` widget, updated sealed
  class hierarchy, updated switch. Delete `_MessageBubble`,
  `_ReceiveAddressCard`, `_ReceiveStatusRow`.

No Rust changes. No FRB regen.

## What stays the same

- `ChatMessage`, `ReceiveAddressCardModel` data models — only
  their rendering changes.
- All event handlers for `ChannelEvent_*` cases.
- Verification helpers (`_verifyCard`,
  `_maybeMarkAddressSharedForPeer`, `_markAddressSharedFor`,
  `_retryReceiveSend`, `_openReceivePage`).

## Verification

- `flutter analyze lib` clean
- `dart format --output=none --set-exit-if-changed` clean
- Manual: send a chat text → bubble renders as before (same
  shape, same colors, same hover actions, timestamp, status tick)
- Manual: send a receive-address attachment with a memo → renders
  as a chat-shaped bubble with the address attachment panel at
  top, the memo as the bubble text, status tick at the trailing
  edge (matches normal chat-message look)
- Manual: failed receive send → red bubble + "Tap to retry"
  same as a failed text message
- Manual: tap a sent+verified receive bubble → ReceivePage opens
- Manual: out-of-window verified peer card → "Apply anyway"
  action in the footer; tapping advances the cursor and the
  footer dismisses

## Compile-time invariant we gain

After this lands, adding a new chat-shape timeline event (e.g.
payment request) requires:

1. Define `class TimelineXxx extends ChatBubbleItem`.
2. Implement `buildBubble` returning a `ChatBubble`.
3. The exhaustiveness check in the switch already covers it
   because it matches `ChatBubbleItem b`.

A new variant that *wants* to render as a chat-shape but forgets
to extend `ChatBubbleItem` falls into the free-form arm and
becomes immediately visible in review as "this should have been
a chat bubble." A new chat-shape variant that extends
`ChatBubbleItem` but forgets `buildBubble` is a compile error.

## Out of scope

- Refactoring signing / transaction cards. They're conceptually
  system events, not "messages from people"; they keep their
  current free-form layout under `TimelineItem` directly.
- New attachment kinds — this plan only re-homes receive. Future
  attachments (payment requests, signing requests rendered
  inline, etc.) can land as additional `ChatBubbleItem`
  subclasses without touching the bubble widget.

---

## Revision — receive attachment correctness-by-construction

After user testing the first cut: the receive attachment was
confusing because (a) the address was only partially visible, (b)
the bubble didn't look tappable, and (c) the "verified" language
created two unsolvable problems.

### "Verified" must go entirely

The bubble has no business telling anyone an address is "verified."
Verifying an address is **fundamentally something the user does
out-of-band** — on a hardware device screen, against an external
source of truth. The app can never legitimately make that claim,
and pretending to does two bad things at once:

1. **Sender confusion** — own bubbles couldn't show "verified"
   today (we had no way to surface the badge to the author), so the
   author saw a worse bubble than the receiver and wondered how to
   "get" verification.
2. **Receiver confusion** — even on the receiver side, an in-app
   "Verified" stamp implies a guarantee the app cannot provide.
   Then the receive page offers "Verify on device" and the user
   reasonably assumes that's *how* you earn the badge — but it
   isn't.

The fix is structural: drop the badge, drop the language, drop
the runtime address comparison that was producing it.

### Make the address correct-by-construction

Today the sender ships the full bech32 string in
`ReceiveAddressPayload`. That string could be wrong (bug, attacker,
typo). So we added a Dart-side check that compared the wire string
against `superWallet.getAddressInfo(derivationIndex)` — which is
exactly the structural truth we should have just shipped.

**Drop `address: String` from the wire format.** Receivers derive
the address themselves from their own descriptor at the given
derivation index. There is no claim to verify because the address
is computed, not asserted.

```rust
// frostsnap_nostr/src/signing/events.rs
pub struct ReceiveAddressPayload {
    pub derivation_index: u32,
    pub memo: String,
}
```

`ChannelEvent::ReceiveAddress` (mirror in the FRB API) drops its
`address` field. `ChannelHandle::send_receive_address` and
`ChannelClient::send_receive_address` drop their `address`
parameter.

Keychain is always external for shared receive addresses (see
`frostsnap_coordinator/src/bitcoin/wallet.rs::address` which
hardcodes `BitcoinAccountKeychain::external()`). Account is the
wallet's default account. No extra wire fields needed.

### Dart-side simplifications

- `ReceiveAddressCardModel.address` is **derived at render time**
  via `superWallet.getAddressInfo(masterAppkey, derivationIndex)`,
  not stored on the model. (Or stored once at receive time as the
  derived string for convenience — but never as a wire-supplied
  value.)
- Delete `_verifyCard` and the bool plumbing through
  `ChatBubbleHandlers` / `_ReceiveAttachment`.
- The `_ReceiveAttachment` widget renders:
  - `Icons.call_received_rounded` + "Receive address" label
  - Derivation index (`#N`) prominent
  - **Full bech32 address** — let it wrap to multiple lines so it's
    legible without truncation.
  - No verification status. No badges. No checkmarks.
  - Trailing `Icons.open_in_new` (or `chevron_right_rounded`) inset
    visually distinct from the bubble status tick, so the
    attachment looks tappable. Combined with the bubble's InkWell
    ripple this is enough of an affordance.
- Bubble is tappable on `status == sent` regardless of who shared
  it. No verification gate. Tap → `ReceivePage(derivationIndex)`
  where the user can do an out-of-band device verify if they want.
- Sender and receiver render **identical** attachment content. No
  asymmetric badges.

### What the lookahead bound becomes

The lookahead bound (`_receiveIndexLookahead = 100`) was there
because we didn't want a malicious peer to advance our local
cursor far. That concern still exists when we call
`mark_address_shared` on a peer's published index — the index is
peer-supplied even when the address is structural.

Keep the cap. The receiver:
- Always derives + displays the address (no error path).
- Calls `mark_address_shared` only when the index is within the
  window. Out-of-window indices are still shown, but the cursor
  is not advanced and there is no special UI for it (no "Apply
  anyway" button, no warning chip). If a user actually wants to
  use the address, tapping into `ReceivePage` and using it normally
  is enough.

### Footer / actions slot

After this revision, the receive bubble has no actions:

- No "Apply anyway" (handled silently)
- No "Verify on device" inline (the page handles it)
- Retry on a failed send is still wired via `ChatBubble.onRetry`
  (taps the bubble when failed)

So `actions: []` for receive bubbles. The footer-slot
infrastructure on `ChatBubble` stays — future attachment kinds may
need it.

### Files touched (additions to the original Files list)

- `frostsnap_nostr/src/signing/events.rs` — drop `address` from
  `ReceiveAddressPayload` and from `ChannelEvent::ReceiveAddress`.
- `frostsnap_nostr/src/signing/mod.rs` — update the receive command
  handler + inbound AppEvent decoder.
- `frostsnapp/rust/src/api/nostr/mod.rs` — drop `address` from
  the FRB mirror and from `ChannelClient::send_receive_address`.
- `frostsnapp/lib/nostr_chat/chat_page.dart`:
  - `ReceiveAddressCardModel`: derive address at render time
  - `TimelineReceiveAddress.buildBubble`: identical sender +
    receiver render, no verification fields
  - `_ReceiveAttachment`: full address, no verified row, tap
    affordance icon
  - Drop `_verifyCard` and verification fields on
    `ChatBubbleHandlers`
  - `_proposeReceiveAddress`: pending attachment stays
    `(int index, String address)` for the input-ribbon preview;
    the publish call only ships the index.

### Verification (revised manual checks)

- Receive bubble shows full address (wrapped if needed), index,
  tap affordance icon. No "verified" word anywhere.
- Sender's own bubble and other members' bubbles look identical.
- Tap → ReceivePage opens at the right index. Use ReceivePage's
  device-verify flow there if desired.
- Peer publishes a wild index → bubble renders normally with the
  derived address; cursor stays put silently.
- Failed send still shows the red bubble + "Tap to retry."
