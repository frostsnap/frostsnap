# fix-signing-channel-spinner
# Fix "Setting up signing channel…" spinner

## Problem

`org_keygen_page.dart::_connectSigningChannel` shows a
`CircularProgressIndicator` in `AlertDialog.icon`
(`org_keygen_page.dart:987`):

```dart
AlertDialog(
  icon: const CircularProgressIndicator(),
  ...
)
```

`AlertDialog`'s `icon` slot doesn't constrain its child to a
square. The `CircularProgressIndicator` expands to whatever space
the parent offers (often quite wide on desktop) and renders as an
elongated arc instead of a tidy circle.

## Fix

Wrap the spinner in a fixed-size `SizedBox` (e.g. 32×32, matching
M3 icon-slot sizing):

```dart
AlertDialog(
  icon: const SizedBox(
    width: 32,
    height: 32,
    child: CircularProgressIndicator(strokeWidth: 3),
  ),
  ...
)
```

`strokeWidth: 3` keeps it readable at small size; default 4 looks
a touch heavy in a 32px circle.

## Verification

- Open keygen → reach "Setting up signing channel…" dialog.
- Spinner is a circle, sized as a normal dialog icon, not
  stretching across the dialog width.
- `flutter analyze lib` clean.

## Out of scope

Anything else in this dialog (status text, retry behaviour) is
unchanged.
