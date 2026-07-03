# homepage_three_verbs
# Homepage = Create / Restore / Join; mechanism forks live inside the flows

The wallet-add homepage currently leaks the restore *mechanism* onto
the surface: "Restore wallet" (local) and "Start a recovery lobby"
(remote) sit side by side, and a user holding backup cards AND
knowing friends hold shares has no idea which to tap — at the moment
they have the least context, next to protocol jargon ("lobby").
Create doesn't have this problem: one card, and the local/remote
fork ("Just me" / "A group of us") happens inside `OrgKeygenPage`
where each branch gets a sentence of explanation.

Make the homepage three intent verbs and push every mechanism fork
inside:

- **Create a wallet** — unchanged; already forks internally.
- **Restore a wallet** — ONE card; a new chooser step forks to the
  existing local flow or the remote recovery create flow.
- **Join with invite link** — unchanged (universal classifier).

## Design direction (frontend-design pass)

- The homepage asks "what do you want to do?", never "which
  mechanism?". Verbs only; jargon ("lobby", "nostr") is banished to
  the depth where it can be explained.
- The fork is phrased as WHO'S INVOLVED, in parallel with create's
  existing language:
  - "With your devices here" — plug in devices holding keys or
    enter seed-word backups yourself.
  - "With others" — other people hold key shares; coordinate the
    recovery over nostr.
- The chooser step reuses the `_ChoiceCard` dialect from
  `OrgKeygenPage`'s "Who is this for?" step (icon + title +
  subtitle cards, one emphasized) so the two ceremonies read as one
  system. Extract `_ChoiceCard` to a shared widget rather than
  copying it a second time.

## Changes

1. **Extract `_ChoiceCard`** from `org_keygen_page.dart` into a
   shared file (e.g. `lib/choice_card.dart`); keygen adopts the
   import. No visual change.

2. **Restore chooser** — new small stepped page (or a step inside a
   dialog shown by `wallet_add.dart`):
   - `MultiStepDialogScaffold`, title "Restore a wallet", two
     `ChoiceCard`s per the design direction ("with your devices
     here" emphasized — it's the common path).
   - Local branch → the existing
     `MaybeFullscreenDialog.show<RestorationId>(RecoveryFlowWithDiscovery(
     RecoveryContext.newRestoration()))` path currently in
     `showWalletRecoverDialog` (unchanged semantics: cancelProtocol
     + selectRecoveringWallet after).
   - Remote branch → the existing `showRemoteRecoveryDialog` body
     (`RemoteRecoveryPage.create` + unplug prompt +
     `openNewlyCreatedWallet`).
   - Back/close from the chooser dismisses without side effects.

3. **`wallet_add.dart` homepage**:
   - `WalletAddColumn` drops to three cards: Create / Restore /
     Join. `AddType.remoteRecoverWallet` is deleted;
     `AddType.recoverWallet` routes to the new chooser.
   - The "Restore wallet" section header disappears if it reads
     redundant with only one restore card (copy pass decides;
     keep the Create/Restore visual grouping if it still earns its
     space).
   - `showWalletRecoverDialog` / `showRemoteRecoveryDialog` fold
     into the chooser flow (keep `showRemoteRecoveryDialog` as an
     internal step target; nothing else calls it —
     `main.dart`'s deep-link path goes through the join dialog).

4. **Copy pass** with the frontend-design skill on the three cards
   + the chooser step.

## Deliberately NOT done

- No changes to create (`OrgKeygenPage`) beyond the `_ChoiceCard`
  extraction import.
- No changes to the join flow or deep links.
- No changes to any ceremony flow behind the fork.

## Tests

- The restore chooser is pure navigation — cover with a widget
  test: pump the chooser, tap each card, assert the right
  callback/route fires (callbacks injected so no live coord/nostr
  needed).
- Existing recovery/join tests unchanged.

## Acceptance

- `flutter analyze` + `dart format` clean; recovery/join/new
  chooser tests green.
- Manual: homepage shows exactly three action cards; Restore →
  chooser → both branches land in their existing flows; Create and
  Join behave exactly as before.
