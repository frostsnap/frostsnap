/// The kinds of `frostsnap://…` invite links the app knows how to
/// join. Each variant maps to exactly one downstream flow — see
/// `wallet_add.dart`'s `JoinLinkPage` for the branch.
enum LinkKind {
  /// `frostsnap://channel/<hex>` — join a chat channel on an
  /// existing wallet. Consumed by `NostrClient.joinFromLink`.
  channel,

  /// `frostsnap://keygen/<hex>` — join an in-flight remote keygen.
  /// Consumed by the keygen-join path extracted from
  /// `OrgKeygenPage`.
  keygen,

  /// `frostsnap://recovery/<hex>` — join a remote recovery lobby.
  /// Consumed by `NostrClient.joinRemoteRecoveryLobby`.
  recovery,

  /// Any string that doesn't start with a supported prefix.
  unknown,
}

const _prefixes = <String, LinkKind>{
  'frostsnap://channel/': LinkKind.channel,
  'frostsnap://keygen/': LinkKind.keygen,
  'frostsnap://recovery/': LinkKind.recovery,
};

/// Prefix-classify a `frostsnap://…` URL. Pure — the returned
/// [LinkKind] is stable per (input, code version). Whitespace is
/// trimmed to match paste-then-submit behaviour.
LinkKind classifyJoinLink(String url) {
  final trimmed = url.trim();
  for (final e in _prefixes.entries) {
    if (trimmed.startsWith(e.key)) return e.value;
  }
  return LinkKind.unknown;
}
