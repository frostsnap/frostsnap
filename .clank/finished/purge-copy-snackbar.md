# Purge snackbar copy confirmations

## Goal

Replace all snackbar-based "copied to clipboard" confirmations with
the standard `copyToClipboard()` / `copyToClipboardQuietly()` from
`lib/copy_feedback.dart`. That module shows a floating chip with
haptic feedback — the app's canonical copy confirmation pattern.

## Sites to fix

All sites use `copyToClipboard(value)` (with visible chip feedback),
NOT `copyToClipboardQuietly` — these copy buttons have no other
inline confirmation.

1. `lib/nostr_chat/member_detail_sheet.dart` — `_copyToClipboard` method
2. `lib/nostr_chat/profile_settings_page.dart` — `_copyToClipboard` method
3. `lib/nostr_chat/profile_settings_page.dart` — nsec export copy button
4. `lib/nostr_chat/group_info_page.dart` — invite link copy
5. `lib/wallet.dart` — error copy button
6. `lib/org_keygen_page.dart` — invite link copy
7. `lib/nostr_chat/chat_page.dart` — error timeline copy

## Pattern

The `copyToClipboard(data)` function from `copy_feedback.dart`:
- Writes to clipboard
- Fires haptic feedback
- Shows a floating "Copied" chip near the tap point (auto-dismisses)
- Announces to screen readers

`copyToClipboardQuietly(data)` skips the chip (for cases where
surrounding UI already provides feedback).

## Verification

`flutter analyze lib` clean. Grep for `SnackBar.*copied` and
`showMessageSnackbar.*copied` returns zero matches after the change.
