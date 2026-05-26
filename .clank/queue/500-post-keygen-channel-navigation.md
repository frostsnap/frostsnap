# Post-keygen navigation doesn't enter remote wallet

After completing a remote keygen, the app navigates to the local
wallet shell instead of the remote (chat-first) shell.

Likely cause: the `ChannelState` await in `_dismissOverlayThenPop`
times out or errors, so `setCoordinationUiEnabled` never runs and
`WalletModeShell` sees `isRemote = false`.

Debug: add logging to trace which step fails in the
keygen→channel-creation→coord-UI-enable→pop flow.
