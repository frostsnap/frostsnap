# Transaction history tray (over chat)

## Problem

The "Transaction history" button (top-right of the chat page,
`Icons.receipt_long`) currently pushes `RemoteWalletActivityPage`
as a full Navigator route — leaving the chat surface entirely.
For a chat-first wallet, transaction history is a glance — not a
destination.

The current activity page also embeds `BackupWarningBanner` via
`walletTxSlivers`/`_PinnedBalanceCard`. The user doesn't want that
banner cluttering the history tray.

## Goal

Tapping the tx-history icon slides a **tray over the chat** —
chat stays visible/dismissible behind, no new Navigator
destination. The tray shows the balance card and the transaction
list, but **without the backup warning banner**.

## Approach (recommended: end-drawer)

Use a `Scaffold.endDrawer` on the remote-wallet shell. Pros:

- Built-in slide animation from the right
- Swipe-to-dismiss for free
- No Navigator route push — opens an overlay attached to the
  current Scaffold
- Matches the button's top-right position (slides in from same
  side)

The remote-wallet shell (`_RemoteWalletShell` in `wallet.dart`)
owns the Scaffold + AppBar; the endDrawer goes there. The
tx-history icon's `onPressed` calls
`Scaffold.of(context).openEndDrawer()`.

The drawer body = the existing tx-slivers content **minus the
backup banner**.

### Alternative: modal bottom sheet

`showModalBottomSheet` with `isScrollControlled: true`,
`useSafeArea: true`, near-full height. Pros: more iOS-feeling,
draggable. Cons: takes a bit more work to get the
balance-card pin behavior right; competes with other modal
sheets if one is already open.

The end-drawer is the better fit for "tray over chat" and is
simpler to implement.

## Hide the backup banner in the tray

`walletTxSlivers` wraps `UpdatingBalance` in a `PinnedHeaderSliver`
(`wallet.dart::746-754`). `UpdatingBalance` positions the
`BackupWarningBanner` inside its Stack
(`wallet.dart::1480-1483`).

Add a `showBackupBanner: bool = true` flag to **both**
`walletTxSlivers` and `UpdatingBalance`. `walletTxSlivers` passes
it through to `UpdatingBalance`. `UpdatingBalance` conditionally
includes the `BackupWarningBanner` Align in its Stack.

Default `true` keeps every existing call site unchanged; the tray
passes `false`.

## Files

- `frostsnapp/lib/wallet.dart`:
  - `_RemoteWalletShell` gets a `GlobalKey<ScaffoldState>` and an
    `endDrawer` widget hosting the tx tray
  - `_openWalletActivity` deleted — replaced with a one-liner
    that calls `_scaffoldKey.currentState?.openEndDrawer()`
  - `walletTxSlivers` + `UpdatingBalance` gain `showBackupBanner`
    flag (default true)
  - `RemoteWalletActivityPage` + `_RemoteWalletActivityPageState`
    **deleted** (verified: only caller is `_openWalletActivity`,
    which goes away)

## Open questions

### Q2: Drawer width on desktop

Material's default Drawer width is 304. For a tx-history tray on
a desktop with chat behind, that's too narrow to be useful.
Recommendation: override width to ~420 (or
`min(MediaQuery.sizeOf(context).width * 0.9, 480)`).

### Q3: Pinned balance card behavior in the drawer

The tx slivers pin the balance card at the top while scrolling.
Inside an endDrawer this should work the same way (the drawer
hosts its own scroll). Verify during impl.

## Verification

- Tap history icon → tray slides in from right, chat visible
  behind (semi-transparent scrim)
- Swipe right or tap scrim → tray dismisses
- Tray shows balance card + tx list, **no backup warning banner**
- Other places that show `walletTxSlivers` (local wallet) STILL
  show the banner
- `flutter analyze lib` clean
