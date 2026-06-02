# Receive address flow in signing channel (DESIGN-ONLY)

> **This is a design plan.** No code is implemented as part of this
> plan. Reviewers should FINISH when the design is solid enough to
> spawn one or more concrete implementation plans from it.

## Problem

When a participant receives Bitcoin to a remote-coordinated wallet,
they reveal a receive address (and copy it). Today nothing tells the
other participants. Two problems:

1. **Coordination gap** — another participant could hand out the same
   address for a different purpose, leaking the link.
2. **Verification gap** — when a participant shares an address they
   claim is theirs, others have no easy way to confirm it actually
   belongs to the wallet (would be trivially possible to be tricked
   into thinking attacker's address is the wallet's).

## Proposed flow

### Sender side

1. User taps `+` in the signing-channel chat
   (`chat_page.dart::_showActionMenu`).
2. New action: **Request receive address**. Tapping opens a sheet:
   - Address preview (next unused, derivation index shown)
   - Optional **memo** field ("what is this for?")
   - Buttons: **Copy & share** / **Cancel**
3. On **Copy & share** — **relay-acceptance gated ordering**:
   - Copy address to clipboard (with chip feedback) so user has it
     regardless of relay outcome
   - Publish a `ReceiveAddress` message FIRST via the relay-gated
     dispatch path (`channel_runner.rs::dispatch_prepared` — fails
     on zero relay OK, does not apply locally on failure)
   - On publish success: call
     `mark_address_shared(master_appkey, derivation_index)` locally
     (advances `reveal_to_target` via bdk)
   - On publish failure: leave the address NOT marked shared; render
     the message bubble in a **`failed-to-share`** state with a
     Retry button. User can retry, copy again, or cancel.

   This keeps the local wallet's reveal cursor in sync with what
   peers actually know about. The only out-of-band leak risk is the
   clipboard copy itself — accepted (user explicitly asked).

### Receiver side

When a `ReceiveAddress` message arrives in the channel:

1. **Verify locally first** — derive the address at
   `derivation_index` from the wallet descriptor; compare to claimed
   address.
2. If verification **fails** → render with red error chip
   ("Address does not match expected derivation"). Do NOT advance
   `reveal_to_target`. Do NOT call `mark_address_shared`.
3. If verification **succeeds** — apply **bounded auto-advance**:
   - Render the message as a special chat card:
     `<sender> shared address #N — <memo>`, with copy button +
     green verified badge.
   - Auto-call `mark_address_shared(...)` ONLY if the claimed index
     is within a safe window of the local wallet:
     `claimed_index <= local_next_index + LOOKAHEAD`
     (e.g., `LOOKAHEAD = 100`).
   - If the index is OUTSIDE that window → render the card as
     "verified, but not auto-applied — index #N is far ahead of your
     wallet's current cursor (#M). [Apply anyway]". The button
     explicitly confirms before calling `mark_address_shared`.

   Rationale: a malicious or buggy peer could publish a valid
   derivation at index 10^6, forcing this device to call
   `reveal_to_target(10^6)` which would expand BDK's tracked range
   massively on next monitor init. The lookahead bound caps that
   blast radius to the normal range a wallet would advance.

## Open design questions (resolve before implementation plan)

### Q1: New `Kind` vs piggyback on existing chat message

Options:
- **(A)** New custom `Kind` (e.g., `Kind::Custom(7800)`) with bincode
  payload. Clean separation, follows existing pattern for
  `SigningMessage`. Requires a new `ChannelEvent::ReceiveAddress`
  variant.
- **(B)** Regular `ChannelMessage` with a structured marker prefix
  (e.g., JSON `{"type":"address",...}` in the content). Worse:
  pollutes plain chat semantics, harder to filter.

**Recommendation: (A)**. Matches how signing already works.

### Q2: Where does verification live?

- **Rust side** — `frostsnap_nostr` could verify the address against
  the wallet descriptor (it has access to `MasterAppkey` via the
  channel) before emitting the event to Dart. Pro: centralized,
  trusted. Con: `frostsnap_nostr` doesn't currently hold a
  `SuperWallet`/descriptor; it would need one.
- **Dart side** — `frostsnap_nostr` emits the claim verbatim; Dart
  calls `super_wallet.get_address_info(master_appkey, index)` to
  derive locally and compares. Pro: no new Rust dependency. Con:
  verification scattered across UI consumers.

**Recommendation: Rust side.** Emit a `ReceiveAddress` event that
already carries a `verified: bool` / `verification_error: Option<String>`
field, computed by `frostsnap_nostr` using the wallet descriptor.
Receiver UI is dumb — it just renders. This requires plumbing the
descriptor into `frostsnap_nostr` (or passing a verifier closure
when the channel is created — TBD in impl plan).

### Q3: Reveal-to-target on receive — what about race?

Two participants could share addresses at near-simultaneous indices.
Each receives the other's message AFTER they generated their own. The
later receiver's `mark_address_shared` will advance past both — fine.
But if A and B both call `next_address` before either publishes, they
both get the SAME index, and one's address won't match the wallet's
expected derivation when the other receives it.

This is a soft-coordination gap that exists today. Address derivation
is deterministic so both addresses would actually be identical (same
master pubkey, same index, same keychain) — collision but not a bug
per se. The memo text would clarify.

**Decision: accept the collision, surface both messages.** Users see
both shares for the same index in the chat history.

### Q4: What about cancel / typo / mistake?

The sender published their address. Can they retract? Options:
- **No retract** — the address is shared, period. (Simplest.)
- **Reply-with-error** — sender can post a follow-up "ignore the
  above" message; UI doesn't link them.

**Recommendation: no retract for v1.** If the sender made a mistake
they post a regular chat reply explaining.

### Q6: Lookahead window value

The receiver auto-advance bound (`LOOKAHEAD` in the receiver flow)
caps the maximum jump in the local reveal cursor when a peer's
claimed index is ahead of ours. Trade-off:

- Too small (e.g., 20) — legitimate "I haven't generated addresses
  in a while" cases require manual confirmation, friction.
- Too large (e.g., 10000) — malicious peer can still push the cursor
  meaningfully far, wasting BDK index space and scan time.

BDK's default gap limit is 20. A lookahead of 100 covers normal user
behavior while still bounding worst-case advance. Value should be a
named constant for easy tuning.

**Recommendation: `RECEIVE_INDEX_LOOKAHEAD = 100`** (named constant
in `frostsnap_nostr` or the chat page).

### Q5: Index display

Show `#42` (derivation index)? Just the address? Both?

**Recommendation: show `#42` prominently** so receivers can
cross-reference with their own wallet's address list. The address
itself is also shown for copy.

## Rendering in chat

Special card (like the signing card) with:
- Avatar + display name of sender
- "shared receive address" subtitle
- `#42 · bc1q...` (monospace)
- Memo (if present, in italic below)
- Copy button on the address
- Verification badge: green check (verified) or red error chip

## Files this design will touch (for the impl plan)

- `frostsnap_nostr/src/signing/events.rs` — new
  `ReceiveAddress` variant on `ChannelEvent`, new wire payload struct
- `frostsnap_nostr/src/channel_runner.rs` — handle the new `Kind`,
  invoke address verification
- `frostsnap_nostr/src/...` — figure out where the descriptor lives /
  how verification is wired (Q2)
- `frostsnap_coordinator/src/bitcoin/wallet.rs` —
  `mark_address_shared` already exists and does
  `reveal_to_target`; no change needed
- `frostsnapp/rust/src/api/super_wallet.rs` — may need
  `mark_address_shared` FRB binding if Dart drives it
- `frostsnapp/lib/nostr_chat/chat_page.dart` — new `+` menu entry,
  new `_proposeReceiveAddress` flow, new message card widget
- New widget: `frostsnap/lib/nostr_chat/receive_address_card.dart`
  (or similar)

## Non-goals (out of scope for this design)

- BIP-21 payment requests with amount/label
- Address rotation policy / quotas
- Receiver-side notification / push
- Privacy considerations beyond not-leaking-the-same-address (no
  per-counterparty isolation, no stealth addresses)

## Done criteria for this design plan

Reviewers should mark FINISHED when:
- Q1–Q5 have agreed answers (or alternatives are explicitly punted
  with justification)
- The flow is clear enough that an impl plan can scope per-crate work
- File-touch list is accurate
