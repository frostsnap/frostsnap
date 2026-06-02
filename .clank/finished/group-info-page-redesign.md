# Group Info page redesign (DESIGN-ONLY)

> Design plan — reviewers should FINISH when the layout, hierarchy,
> and key-management placement are agreed. No implementation as part
> of this plan; an impl plan will follow.

## Problems with current layout

1. **You sits in its own block above the members list** —
   inconsistent with member psychology, hard to compare your shares
   to others'.
2. **"Leave remote wallet" is sandwiched** between you and the
   MEMBERS section. Destructive action in the middle of the page
   reads as just another option; in multi-person wallets it visually
   splits the member list.
3. **No surface for key management** — to add a device share or
   recover one, you have to leave Group Info and dig through the
   local wallet's "More" page (`wallet_more.dart`). For a remote
   wallet whose chat IS its shell, this is a dead end.
4. **No clear "what is this group?" surface** — wallet identity
   (name, threshold) sits as a small subtitle under the avatar.

## Target page structure

Single scroll, three clear bands:

```
┌──────────────────────────────────────┐
│  Group Info  [back]      [descriptor]│  app bar
├──────────────────────────────────────┤
│                                      │
│              [group avatar]          │
│             signet Chat              │
│            3 members · 2-of-3        │
│                                      │
├──────────────────────────────────────┤  ── group-actions band
│  🛡   Backup my keys                  │
│       Save share #1 to paper/steel   │
├──────────────────────────────────────┤
│  📍   Check address                  │
│       Verify an address is ours      │
├──────────────────────────────────────┤
│  </>  Show descriptor                │
│       Miniscript for export          │
├──────────────────────────────────────┤  ── members band
│  MEMBERS                             │
├──────────────────────────────────────┤
│  ⊙   alice.btc  (you)  ⭐            │  ← highlighted
│      #1                              │
│      Tap to manage your devices      │  ← subtitle hint
├──────────────────────────────────────┤
│  ⊙   bob.npub                        │
│      #2 #3                           │
├──────────────────────────────────────┤
│  ⊕   Invite someone                  │
│      Copy invite link                │
├──────────────────────────────────────┤  ── danger-zone band
│  ⊠   Leave remote wallet             │  ← error color
│      Switch back to local            │
└──────────────────────────────────────┘
```

### Band 1 — group-actions

Above members. These are GROUP-level actions every participant
shares. Subset of the local wallet's `wallet_more.dart::manageColumn`
that makes sense in a remote-wallet context:

- **Backup my keys** — opens `BackupChecklist` for our local
  shares only
- **Check address** — `CheckAddressPage` (unchanged)
- **Show descriptor** — `showExportWalletDialog` (currently the
  small `</> ` icon in app bar; surface it as a tile too)

`Coordinate over Nostr` toggle is NOT included — that lives on the
local-wallet `wallet_more.dart` only; for a remote wallet it's
already on. (Already removed in the current branch.)

### Band 2 — members

`MEMBERS` header (small caps), then ordered list:
1. **You** (highlighted — see "Self-highlighting" below)
2. Other members in deterministic order (e.g., by share index, or
   by first-published timestamp from the channel)
3. **Invite someone** as the last tile (acts like a "+ add" affordance
   in the same band)

### Band 3 — danger zone

A single tile at the very bottom: **Leave remote wallet** in error
color. Visually separated by a larger spacer + thin divider above.

## Self-highlighting (how "you" is special-cased)

You're in the member list (no separate block) but the row is
visually distinct:

- Background tint (`colorScheme.primaryContainer` at ~20% alpha)
- `(you)` badge after the display name
- Subtitle ALWAYS shows your shares (from local access structure —
  authoritative for self even when channel state is stale)
- Subtitle adds a hint line: `Tap to manage your devices` (only on
  your row, in muted text)

Other member rows: plain tile, share chips below name, no hint.

## Key management (Q: which placement?)

This is the biggest open question.

### Option A — "My devices" section before members

A dedicated band BETWEEN group-actions and members:

```
MY DEVICES
├ 📱 phone-1 · share #1
├ 💻 laptop · share #2
└ ⊕ Add another device
```

- Pro: discoverable, action-oriented, lists each local device by
  name (the channel only shows shares not devices)
- Con: redundant with the share chips on your member row;
  duplicates info; pushes other members further down

### Option B — Tap-your-row → device manager (RECOMMENDED)

The Member Detail sheet for your own row gets two extra action tiles
at the bottom:

```
[your avatar + name + share chips]

DEVICES HOLDING YOUR SHARES
├ 📱 phone-1 · share #1
├ 💻 laptop · share #2

ACTIONS
├ ⊕ Add another device for these shares
└ 🔄 Recover a share from a backup
```

- Pro: keeps main page focused, contextual (you opened your own
  card → manage your stuff), follows the "profile sheet"
  mental model
- Con: discoverability — user has to know to tap their row

### Option C — Hybrid: small button on your row

Same as B but add a `Manage` text-button or pencil icon on the right
side of your member row (in place of / alongside the `chevron_right`).
Tapping it opens the same device-management sheet from B.

- Pro: explicit affordance + clean main page
- Con: slightly more visual noise on the row

**Recommendation: Option C.** Best of both — visible affordance
(`Manage` button or pencil icon), no extra section, contextual sheet
for actions. Other members' rows just have a chevron.

## Visual language details

- **Tile grouping**: continue the existing `tileShapeTop / mid / end`
  pattern from `wallet_more.dart` so rounded-corner bands group
  related actions visually. Each band (group-actions, members,
  danger) is its own rounded-corner card.
- **Vertical rhythm**: 24px gap between bands, 2px between tiles
  within a band (matches `wallet_more.dart::spacing: 2`).
- **Colors**:
  - Action tiles: `surfaceContainerLow`
  - Your member row: `primaryContainer` at low alpha
  - Other members: `surfaceContainerLow`
  - Leave-wallet: text + icon in `colorScheme.error`, tile in
    `errorContainer` at low alpha
- **Typography**: existing Material 3 theme — `headlineSmall` for
  wallet name, `labelSmall` uppercase for band headers, `bodyMedium`
  for tile titles. No new fonts.
- **Iconography**: prefer rounded variants (`_rounded` suffix) to
  match `wallet_more.dart` style.

## Open questions

### Q1: Member order

How are non-self members ordered?
- (a) By share index (1, 2, 3, ...) — matches channel order
- (b) By first-published-timestamp from channel — chronological
- (c) Alphabetical by display name

**Recommendation: (a)** — share index is most meaningful in a FROST
context and stable across runs.

### Q2: "Add device" vs "Recover share" distinction

In Option B/C, the device manager sheet shows two actions. Are these
two distinct flows or one?

Looking at existing code:
- `RecoveryFlowWithDiscovery` recovers a share to a device from
  backup
- `KeysSettings` opens to "View wallet access structure and add
  devices" — this is the "add new device" path

Both are useful and distinct. Keep as two actions.

### Q3: Should Backup show "your shares" vs "all shares"?

The local `BackupChecklist` shows all access-structure devices. For
a remote wallet's Group Info, "Backup my keys" should ONLY surface
THIS user's local shares — not bookmarks for backing up everyone's.

**Decision:** filter `BackupChecklist` to local devices when entered
from Group Info. Detail in impl plan.

### Q4: Where does "Coordinate over Nostr" toggle live?

For a remote wallet it's already on. Currently removed from Group
Info (see prior cleanup commit). Confirm: the only way to turn it
off is via **Leave remote wallet**, which switches back to local
mode. Yes — keep it that way. No separate toggle.

### Q5: Member count line — show member count or share count?

Currently shows `3 members · 2-of-3`. Should it be
`3 members holding 4 shares · 2-of-3 threshold`?

**Recommendation:** keep concise. `3 members · 2-of-3` is fine. If
share count differs from member count (rare), the share chips on
each row tell the full story.

## Files this design will touch

- `frostsnapp/lib/nostr_chat/group_info_page.dart` — full rewrite
  of the build method's body
- `frostsnapp/lib/nostr_chat/member_detail_sheet.dart` — add the
  "manage devices" section + action tiles for self
- `frostsnapp/lib/wallet_more.dart` — extract shareable tile
  styling (or duplicate; impl plan decides)
- Possibly new: `frostsnapp/lib/nostr_chat/manage_my_devices_sheet.dart`
  if the manage view is non-trivial

No Rust changes for layout. If "Backup my keys" needs filtering by
local devices and there's no API for it, a small Rust helper may be
needed — impl plan decides.

## Done criteria (for this design plan)

Reviewers mark FINISHED when:
- The three-band layout structure is agreed
- Option A vs B vs C for key management is decided
- Q1-Q5 have agreed answers
- File-touch list is accurate enough to scope the impl plan
