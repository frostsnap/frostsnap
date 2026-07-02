# join_via_link_unified
# One Join-via-Link path for every frostsnap:// URL

Right now the app has three "join a session" entry points that all
consume `frostsnap://<host>/<hex>` URLs of the same shape:

- **wallet join** — homepage card "Join wallet from link"
  (`WalletAddColumn.showJoinFromLinkDialog` → `JoinFromLinkPage`
  → `NostrClient.joinFromLink`, prefix `channel/`).
- **keygen join** — inside `OrgKeygenPage`, "Join an existing
  session" card in the `sessionRole` step
  (`_ctrl.chooseJoinSession` → `_JoinSessionInput` at
  `org_keygen_page.dart:1749`, prefix `keygen/`).
- **recovery join** — inside `RemoteRecoveryEntryPage`, "Join with
  invite link" button (`_JoinLinkDialog`, prefix `recovery/`).

A user with a `frostsnap://…` URL has to know in advance which of
those three flows to enter. That's what this plan removes: the
homepage "Join wallet from link" card becomes THE way to join any
session, and the dispatcher figures out which flow the URL belongs
to by its prefix. The scattered joiner entry points come out.

Companion improvement: the homepage join dialog gains a Scan-via-
camera button so users don't have to type or paste — `QrStringScanner`
in `lib/camera/camera.dart` is already the primitive used by the
keygen and receive-address flows.

## Invariants

1. **One prefix → one downstream flow.** The classifier is a plain
   string switch — no shared session state, no "did the user pick
   join or create" mode. Each prefix carries its own downstream
   pipeline, unchanged; only the entry point is shared.

2. **Downstream flows stay where they are.** No FFI changes to
   `NostrClient.joinFromLink` / `joinRemoteRecoveryLobby` / keygen
   join. This plan touches routing and one widget extraction; the
   ceremony code is untouched.

3. **`QrStringScanner` is the scan primitive.** We do NOT introduce
   new camera/scanner dependencies — `mobile_scanner` is already a
   dep, `QrStringScanner` already returns a raw string, and the
   existing keygen `_JoinSessionInput` widget is the reference paste
   + scan UI. Extracting that widget avoids duplicating the pattern
   in the join dialog.

4. **Homepage layout stays**. The "Join wallet from link" card
   keeps its slot; only its label and behaviour widen. No new
   top-level cards.

## Files touched

### New

- `frostsnapp/lib/invite_link_input.dart` — extracted paste + scan
  input widget (was `_JoinSessionInput` in `org_keygen_page.dart`).
  API: `InviteLinkInput({controller, focusNode, errorText, onSubmit,
  defaultPrefix?})`. `defaultPrefix` is the on-focus autofill (was
  keygen-specific — becomes optional so the homepage dispatcher can
  omit it).

- `frostsnapp/test/join_link_dispatch_test.dart` — unit tests over
  the prefix classifier: `channel/`, `keygen/`, `recovery/`, and
  invalid strings.

### Modified

- `frostsnapp/lib/wallet_add.dart`
  - Rename card "Join wallet from link" → "Join with invite link"
    (or "Join a session" — final wording per the frontend-design
    pass), subtitle "Wallet, keygen, or recovery — the link tells
    us which".
  - `showJoinFromLinkDialog` becomes the universal dispatcher:
    `JoinFromLinkPage` (rename to `JoinLinkPage`) still owns the
    input + status UI, but on submit it calls a new pure classifier
    `LinkKind classifyJoinLink(String url)` and branches:
    - `LinkKind.channel` → existing `NostrClient.joinFromLink`
      path (unchanged).
    - `LinkKind.keygen` → hand off to a lobby-join helper (call
      out into `OrgKeygenPage`'s existing join pipeline; see
      §OrgKeygenPage handoff).
    - `LinkKind.recovery` → open `RemoteRecoveryLobbyPage` after
      calling `NostrClient.joinRemoteRecoveryLobby` (currently done
      by `RemoteRecoveryEntryPage._joinLobby` — extract that
      dispatch method so the join dialog can share it).
    - `LinkKind.unknown` → surface the existing "Not a valid
      invite link" error state.
  - Use the new `InviteLinkInput` widget for the text field + paste
    + scan row.
  - Deep-link deference: `showJoinFromLinkDialog(initialLink:
    'frostsnap://<any>/<hex>')` continues to work; the dispatcher
    handles all hosts.

- `frostsnapp/lib/main.dart` — `_handleDeepLink` simplifies to a
  single case: any `frostsnap://<host>/<path>` with a supported
  host goes to `showJoinFromLinkDialog(initialLink: uri.toString())`.
  The current split between `channel/` and `recovery/` cases goes
  away; unknown hosts return early.

- `frostsnapp/lib/recovery/remote_recovery_entry_page.dart`
  - Remove the "Join with invite link" button and its
    `_joinLobby` / `_JoinLinkDialog` supporting code.
  - Remove `initialJoinLink` (no longer wired — deep links land on
    the unified dialog instead).
  - Rename the page to `RemoteRecoveryCreatePage` — it is now
    leader-create only. Update `wallet_add.dart`'s
    `showRemoteRecoveryDialog` to use the new name.
  - The `dispatchCreate` static + widget test are unaffected.

- `frostsnapp/lib/org_keygen_page.dart`
  - Delete the "Join an existing session" `_ChoiceCard` at
    `_buildSessionRoleStep` (line 674-678).
  - Since the `sessionRole` step now has only "Start a new session",
    skip the step entirely: `choseOrganisation` (line 111) jumps
    straight to `chooseCreateSession`'s destination
    (`OrgKeygenStep.nameWallet` with `_role = host`).
  - Delete: `OrgKeygenStep.sessionRole`, `OrgKeygenStep.joinSession`,
    `chooseJoinSession`, `_buildSessionRoleStep`,
    `_buildJoinSessionStep`, `_trySubmitJoinLink`,
    `_JoinSessionInput` (already extracted to
    `invite_link_input.dart`). `_ctrl.joinLinkController` +
    `joinLinkValid` + `connectError` fields that only served the
    joiner path go too — grep confirms scope before deletion.
  - The keygen JOIN pipeline (`NostrClient.connectToChannel` /
    lobby-connect after paste) is the piece that gets re-called
    from the unified dispatcher. Extract it as a public
    `OrgKeygenPage.dispatchKeygenJoin({nostrClient, coord, link})`
    static, symmetric with `RemoteRecoveryEntryPage.dispatchCreate`
    — so the dispatcher never reaches inside the keygen page's
    private controller.

## Classifier

Pure function, no I/O:

```dart
enum LinkKind { channel, keygen, recovery, unknown }

LinkKind classifyJoinLink(String url) {
  const kinds = {
    'frostsnap://channel/':  LinkKind.channel,
    'frostsnap://keygen/':   LinkKind.keygen,
    'frostsnap://recovery/': LinkKind.recovery,
  };
  for (final e in kinds.entries) {
    if (url.startsWith(e.key)) return e.value;
  }
  return LinkKind.unknown;
}
```

Lives next to `classifyJoinLink` in `wallet_add.dart` or a new tiny
`lib/join_link.dart` if the surface grows. Tested directly by
`join_link_dispatch_test.dart` — one case per prefix + unknown.

## OrgKeygenPage handoff

The keygen join pipeline is the only downstream that's currently
tangled inside a bigger controller. `_trySubmitJoinLink` at
`org_keygen_page.dart` does the connect + wait-for-lobby-then-open
sequence. Extracting it as a static entry point:

```dart
// org_keygen_page.dart
static Future<KeyId?> dispatchKeygenJoin({
  required BuildContext context,
  required NostrClient nostrClient,
  required Coordinator coord,
  required String link,
}) async { ... }
```

lets the join dialog call it without knowing about
`OrgKeygenController` internals. The extracted body is what the
old `chooseJoinSession` → `_trySubmitJoinLink` → `nameWallet`
short-lived controller was doing, minus the shared state.

If the extraction turns out to fight the existing controller shape
(state that's only relevant during a joiner session), STOP and
propose one of:
  a) Keep a small keygen-join-only controller local to the new
     dispatcher.
  b) Push the join UI into a fresh page mirroring
     `RemoteRecoveryLobbyPage`'s shape.

Do not paper over the mismatch with cross-page state sharing.

## Deliberately NOT done

- No changes to link *scheme*. `frostsnap://channel/…` stays
  `channel/`; we're not consolidating to one URL host.
- No "smart" link parsing (params, versioning). Prefix + hex.
- No new camera / scanner packages.
- No unification of the underlying JOIN Rust FFI surface — each
  session type keeps its own `NostrClient` entry.
- No changes to the CREATE side of any flow (recovery leader,
  wallet keygen host).

## Acceptance

- `flutter test test/join_link_dispatch_test.dart` green — one case
  per prefix + `unknown` + empty string.
- Existing `test/recovery_entry_page_test.dart` +
  `test/recovery_lobby_view_test.dart` unchanged and still green.
- `flutter analyze` clean.
- Manual: paste each of the three prefixes into the homepage join
  dialog → observe the correct downstream page opens. Scan a QR
  code produced by the leader side → same behaviour.
- Manual (deep link): `open "frostsnap://recovery/deadbeef…"` (and
  `channel/`, `keygen/`) opens the homepage join dialog with the
  URL prefilled and auto-submits.

## Order of work

1. Extract `_JoinSessionInput` → `lib/invite_link_input.dart` (no
   behavior change).
2. Add `classifyJoinLink` + tests.
3. Extract `RemoteRecoveryEntryPage._joinLobby` into
   `RemoteRecoveryEntryPage.dispatchJoin` static (symmetric with
   the existing `dispatchCreate`).
4. Extract `OrgKeygenPage._trySubmitJoinLink` into
   `OrgKeygenPage.dispatchKeygenJoin` static. Block per §handoff
   if it fights the controller shape.
5. Rewrite `JoinFromLinkPage` (rename to `JoinLinkPage`) to use
   `InviteLinkInput` + the classifier + branching dispatch.
6. Simplify `_handleDeepLink` in `main.dart`.
7. Remove "Join an existing session" from `OrgKeygenPage` +
   dead-code sweep of `chooseJoinSession`, `joinSession` step,
   `_JoinSessionInput`, `_buildSessionRoleStep`,
   `_buildJoinSessionStep`, and controller fields.
8. Remove "Join with invite link" from `RemoteRecoveryEntryPage`,
   rename to `RemoteRecoveryCreatePage`.
9. Copy pass (final wording of the homepage card + dialog title)
   with the frontend-design skill.
