# sidebar-chat-contrast
# Sidebar/chat contrast in remote signing view

## Problem

The wallet sidebar and the remote signing chat body currently read as one continuous surface because they share the same background color. The result is that navigation, wallet context, and chat content appear to float around without a clear visual boundary.

## Goal

Give users an immediate, subtle visual distinction between the side bar and the signing chat body while preserving the app's existing Material styling and avoiding a heavy or decorative redesign.

## Required Design Guidance

Before implementation, consult the `frontend-design` skill and apply its guidance to the UI treatment. Keep the change quiet, functional, and consistent with the existing Frostsnap app surfaces.

## Scope

Focus on the remote signing/chat layout, especially the relationship between:

- the wallet/navigation side bar or drawer area
- the remote signing chat shell/body
- app bars, separators, and surfaces that frame the chat

Likely files to inspect:

- `frostsnapp/lib/wallet.dart`
- `frostsnapp/lib/nostr_chat/chat_page.dart`
- `frostsnapp/lib/theme.dart`
- any existing drawer/sidebar widgets that define the wallet list surface

## Approach

1. Identify the exact widgets that render the sidebar surface and the signing chat body in desktop and narrow layouts.
2. Add contrast using existing theme tokens where possible, such as `surface`, `surfaceContainer`, `surfaceContainerLow`, `outlineVariant`, dividers, or a restrained elevation/shadow treatment.
3. Prefer a clear structural boundary over decorative effects: for example a sidebar background shade, a 1px divider, a different chat body container surface, or an app-bar edge treatment.
4. Preserve local wallet behavior unless the same shared component needs a safe default that keeps current visuals.
5. Avoid one-off colors that do not adapt to light/dark themes.

## Verification

- Remote signing chat on desktop shows a clear boundary between sidebar and chat body.
- Narrow/mobile layout still looks coherent when the drawer is opened or closed.
- Chat content no longer appears to float on the same plane as navigation.
- Light and dark themes both retain adequate contrast without looking heavy.
- `flutter analyze lib` passes.
