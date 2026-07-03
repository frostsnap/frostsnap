# wallet_name_network_picker
# EnterWalletNameView uses NetworkAdvancedOptions like every other name step

The recovery flows (local restore and remote recovery create, which
now reuse `EnterWalletNameView` per [[recovery_ui_keygen_parity]])
render the app's ONLY remaining instance of `BitcoinNetworkChooser`
— a bare `DropdownButton` under a "(developer) Choose the network:"
label. Every other wallet-naming surface (`wallet_create.dart`,
`org_keygen_page.dart`) uses `NetworkAdvancedOptions`: the network
tucked behind a collapsed "Developer" toggle revealing a
`SegmentedButton`, with a dismissible `InputChip` when a
non-mainnet network is selected. The deleted `CreateLobbyForm` had
the good picker, so remote create visibly regressed when it started
reusing the restoration view.

## Change

- `restoration/enter_wallet_name_view.dart`: replace the
  `BitcoinNetworkChooser` block with `NetworkAdvancedOptions
  (selected: bitcoinNetwork, onChanged: ...)`. Keep the existing
  developer-mode gate exactly as-is (NetworkAdvancedOptions is
  additionally self-collapsing, matching keygen's name step which
  gates the same way). This fixes local restore AND remote recovery
  create in one place.
- `settings.dart`: `BitcoinNetworkChooser` loses its last consumer —
  delete it (grep first; if something in settings still renders it,
  leave it and note why).

## Tests

- `recovery_create_page_test.dart` doesn't pump the name view (it
  needs SettingsContext) — unchanged.
- No test asserts on BitcoinNetworkChooser; `flutter analyze` +
  existing suites green is the bar.

## Acceptance

- Dev-mode walkthrough (user): local restore and remote recovery
  create both show the same collapsed Developer → segmented network
  picker as wallet create / keygen; no bare dropdown anywhere.
