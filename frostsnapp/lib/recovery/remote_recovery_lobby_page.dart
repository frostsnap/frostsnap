import 'package:flutter/material.dart';
import 'package:frostsnap/async_action_button.dart';
import 'package:frostsnap/fullscreen_dialog_scaffold.dart';
import 'package:frostsnap/invite_widgets.dart';
import 'package:frostsnap/nostr_chat/nostr_profile.dart';
import 'package:frostsnap/restoration/state.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/nostr.dart';
import 'package:frostsnap/src/rust/api/nostr/remote_keygen.dart';
import 'package:frostsnap/src/rust/api/nostr/remote_recovery.dart';

/// Convert what the reused recovery flow popped into the lobby's
/// wire shape. [deviceNameOf] resolves a display name for the
/// device-share arm (the physical-backup arm carries the name the
/// user just typed).
SharePost sharePostFromRemoteResult(
  RemoteShareResult result, {
  required String? Function(DeviceId) deviceNameOf,
}) {
  return switch (result) {
    RemoteShareResultPhysicalBackup(:final phase, :final deviceName) =>
      SharePost(
        deviceId: phase.deviceId(),
        deviceName: deviceName.isNotEmpty
            ? deviceName
            : (deviceNameOf(phase.deviceId()) ?? 'device'),
        deviceKind: DeviceKind.frostsnap,
        shareImage: phase.shareImage(),
        // The device holds the entered backup in recovery mode until
        // post-finalize consolidation.
        needsConsolidation: true,
      ),
    RemoteShareResultDeviceShare(:final share) => SharePost(
      deviceId: share.heldBy,
      deviceName: deviceNameOf(share.heldBy) ?? 'device',
      deviceKind: DeviceKind.frostsnap,
      shareImage: share.heldShare.shareImage,
      needsConsolidation: share.heldShare.needsConsolidation,
    ),
  };
}

/// Pure state → UI mapping for the recovery lobby, kept separate
/// from the stateful `RemoteRecoveryPage` (the lobby-step host in
/// `remote_recovery_page.dart`) so widget tests can drive it
/// directly without standing up a live `RemoteRecoveryLobbyHandle`.
/// The hosting page owns the handle subscription and passes down
/// the current state + action callbacks.
///
/// Rendered as a [MultiStepDialogScaffold] step so the lobby shares
/// the ceremony chrome (sizing, header, pinned footer) with the
/// keygen lobby.
class RecoveryLobbyView extends StatelessWidget {
  /// The single stream payload: app fold in `.state`, runner-owned
  /// member block (names/avatars, keyed by pubkey) in `.members`.
  final RecoveryLobbySnapshot? snapshot;
  final bool isLeader;
  final PublicKey myPubkey;
  final String inviteLink;
  final bool finishing;
  final bool persisting;
  final String? error;
  final AccessStructureRef? recoveredRef;
  final bool verificationFailed;
  final Future<void> Function() onFinish;
  final Future<void> Function() onCancel;

  /// Joiner exit — publishes Leave so peers see this participant
  /// go, then closes the ceremony (keygen lobby footer semantics).
  final Future<void> Function() onLeave;

  /// Callback for the load-share affordance — null in tests that
  /// don't exercise the post-share path.
  final Future<void> Function()? onLoadShare;

  const RecoveryLobbyView({
    super.key,
    required this.snapshot,
    required this.isLeader,
    required this.myPubkey,
    required this.inviteLink,
    required this.finishing,
    required this.persisting,
    required this.error,
    required this.recoveredRef,
    required this.verificationFailed,
    required this.onFinish,
    required this.onCancel,
    required this.onLeave,
    this.onLoadShare,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final s = snapshot?.state;
    final live = s != null && !s.cancelled && s.finished == null;
    final mePosted =
        s?.participants[myPubkey]?.postedShares.isNotEmpty ?? false;

    return MultiStepDialogScaffold(
      stepKey: 'recoveryLobby',
      title: Text(s?.metadata.keyName ?? 'Recovery lobby'),
      subtitle: s == null
          ? null
          : (s.metadata.thresholdHint == null
                ? 'Collect key shares from participants to recover this wallet.'
                : 'Collect ${s.metadata.thresholdHint} key shares from participants to recover this wallet.'),
      body: SliverToBoxAdapter(
        child: s == null
            ? Padding(
                padding: const EdgeInsets.symmetric(vertical: 48),
                child: Column(
                  children: [
                    const Center(child: CircularProgressIndicator()),
                    const SizedBox(height: 16),
                    Text(
                      'Connecting to relay…',
                      style: theme.textTheme.bodyMedium?.copyWith(
                        color: theme.colorScheme.onSurfaceVariant,
                      ),
                    ),
                  ],
                ),
              )
            : Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  // Status board reads top-down: what the ceremony
                  // needs right now, then who's here and what they
                  // gave, then how to grow the room.
                  _ShareProgress(state: s),
                  const SizedBox(height: 24),
                  Row(
                    children: [
                      Expanded(
                        child: Text(
                          'Participants',
                          style: theme.textTheme.labelLarge,
                        ),
                      ),
                      Text(
                        '${s.participants.length} joined',
                        style: theme.textTheme.labelLarge,
                      ),
                    ],
                  ),
                  const SizedBox(height: 8),
                  ..._participantCards(s),
                  if (isLeader && live) ...[
                    const SizedBox(height: 12),
                    InviteTile(
                      onTap: () => showInviteDialog(context, inviteLink),
                    ),
                  ],
                  if (s.finished != null) ...[
                    const SizedBox(height: 16),
                    _FinishedBanner(
                      persisting: persisting,
                      recoveredRef: recoveredRef,
                      error: error,
                    ),
                  ],
                  if (s.cancelled) ...[
                    const SizedBox(height: 16),
                    Text(
                      'Recovery lobby cancelled by leader.',
                      style: theme.textTheme.titleMedium?.copyWith(
                        color: theme.colorScheme.error,
                      ),
                    ),
                  ],
                  if (error != null) ...[
                    const SizedBox(height: 16),
                    Text(
                      error!,
                      style: TextStyle(color: theme.colorScheme.error),
                    ),
                  ],
                ],
              ),
      ),
      footer: Row(
        children: [
          // While the lobby is live: leader can cancel it, joiner can
          // leave it (publishing Leave so peers see them go). Once
          // it's cancelled or finished there's nothing to announce —
          // just Close.
          if (live)
            AsyncActionButton(
              onPressed: isLeader ? onCancel : onLeave,
              style: FilledButton.styleFrom(
                backgroundColor: theme.colorScheme.error,
                foregroundColor: theme.colorScheme.onError,
              ),
              child: Text(isLeader ? 'Cancel lobby' : 'Leave lobby'),
            )
          else
            TextButton(
              onPressed: () => Navigator.of(context).pop(),
              child: const Text('Close'),
            ),
          const Spacer(),
          _primaryButton(s, live: live, mePosted: mePosted),
        ],
      ),
    );
  }

  /// Phase-aware primary — always the single most useful action,
  /// mirroring keygen's `_LobbyPrimaryButton`: contribute first,
  /// then recover (leader) or wait (joiner).
  Widget _primaryButton(
    RecoveryLobbyState? s, {
    required bool live,
    required bool mePosted,
  }) {
    if (s == null) {
      return const FilledButton(onPressed: null, child: Text('Connecting…'));
    }
    if (!live) return const SizedBox.shrink();
    if (!mePosted && onLoadShare != null) {
      return FilledButton.icon(
        icon: const Icon(Icons.add_rounded),
        label: const Text('Load key share'),
        onPressed: () => onLoadShare!(),
      );
    }
    if (isLeader) {
      final canRecover = s.currentRecovery != null && !verificationFailed;
      return FilledButton.icon(
        icon: finishing
            ? const SizedBox(
                width: 16,
                height: 16,
                child: CircularProgressIndicator(strokeWidth: 2),
              )
            : const Icon(Icons.check),
        label: Text(
          finishing
              ? 'Finalizing…'
              : (canRecover ? 'Recover' : 'Waiting for key shares'),
        ),
        onPressed: canRecover && !finishing ? () => onFinish() : null,
      );
    }
    return const FilledButton(
      onPressed: null,
      child: Text('Waiting for recovery'),
    );
  }

  List<Widget> _participantCards(RecoveryLobbyState s) {
    final entries = s.participants.values.toList()
      ..sort((a, b) => a.joinedAtSecs.compareTo(b.joinedAtSecs));
    final live = !s.cancelled && s.finished == null;
    return [
      for (final p in entries)
        _ParticipantCard(
          info: p,
          profile: snapshot?.profileOf(p.pubkey),
          isMe: p.pubkey == myPubkey,
          isLeader: p.pubkey == s.leader,
          devices: [
            for (final ref in p.postedShares)
              for (final share in s.shares)
                if (share.eventId == ref) share.post.deviceName,
          ],
          // Own-card affordance: post ANOTHER key share (the first
          // one goes through the footer primary, like keygen's "Add
          // your devices").
          onAddShare:
              p.pubkey == myPubkey &&
                  live &&
                  p.postedShares.isNotEmpty &&
                  onLoadShare != null
              ? () => onLoadShare!()
              : null,
        ),
    ];
  }
}

/// Keygen-dialect participant card: avatar (+ leader badge), name,
/// status on the right, chevron-expandable list of the key shares
/// this participant contributed.
class _ParticipantCard extends StatefulWidget {
  final RecoveryParticipantInfo info;
  final NostrProfile? profile;
  final bool isMe;
  final bool isLeader;

  /// Device names of the key shares this participant has posted
  /// (resolved from the state's share list by event ref).
  final List<String> devices;

  /// Non-null only on the caller's own card while the lobby is live
  /// and they've already contributed at least one share.
  final VoidCallback? onAddShare;

  const _ParticipantCard({
    required this.info,
    required this.profile,
    required this.isMe,
    required this.isLeader,
    required this.devices,
    this.onAddShare,
  });

  @override
  State<_ParticipantCard> createState() => _ParticipantCardState();
}

class _ParticipantCardState extends State<_ParticipantCard> {
  bool _expanded = false;

  // The channel-member surface is the ONLY name source — including
  // for self. Showing the locally-known settings name would let a
  // broken in-channel profile publish go unnoticed: you'd see your
  // name while peers see your pubkey.
  String _displayName() {
    final name = widget.profile?.displayName ?? widget.profile?.name;
    if (widget.isMe) {
      return (name != null && name.isNotEmpty) ? '$name (You)' : 'You';
    }
    return (name != null && name.isNotEmpty)
        ? name
        : widget.info.pubkey.toHex().substring(0, 8);
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final p = widget.info;
    final hasShares = widget.devices.isNotEmpty;

    final Widget statusLabel;
    if (p.left) {
      statusLabel = Text(
        'Left',
        style: theme.textTheme.bodySmall?.copyWith(
          color: theme.colorScheme.onSurfaceVariant,
        ),
      );
    } else if (hasShares) {
      statusLabel = Row(
        mainAxisSize: MainAxisSize.min,
        spacing: 4,
        children: [
          Text(
            'Ready',
            style: theme.textTheme.labelMedium?.copyWith(color: Colors.green),
          ),
          const Icon(Icons.verified_rounded, size: 18, color: Colors.green),
        ],
      );
    } else {
      statusLabel = Text(
        widget.isMe ? 'Waiting for you' : 'Joined',
        style: theme.textTheme.bodySmall?.copyWith(
          color: theme.colorScheme.onSurfaceVariant,
        ),
      );
    }

    return Card.filled(
      margin: const EdgeInsets.symmetric(vertical: 4),
      color: theme.colorScheme.surfaceContainerHigh,
      clipBehavior: Clip.hardEdge,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          ListTile(
            leading: Stack(
              clipBehavior: Clip.none,
              children: [
                NostrAvatar.small(pubkey: p.pubkey, profile: widget.profile),
                if (widget.isLeader)
                  Positioned(
                    right: -2,
                    bottom: -2,
                    child: Tooltip(
                      message: 'Leader',
                      child: Container(
                        padding: const EdgeInsets.all(2),
                        decoration: BoxDecoration(
                          color: theme.colorScheme.surfaceContainerHigh,
                          shape: BoxShape.circle,
                        ),
                        child: Icon(
                          Icons.star_rounded,
                          size: 12,
                          color: theme.colorScheme.primary,
                        ),
                      ),
                    ),
                  ),
              ],
            ),
            title: Text(_displayName(), overflow: TextOverflow.ellipsis),
            trailing: Row(
              mainAxisSize: MainAxisSize.min,
              spacing: 8,
              children: [
                statusLabel,
                if (widget.onAddShare != null)
                  IconButton(
                    icon: const Icon(Icons.add_rounded, size: 18),
                    tooltip: 'Load another key share',
                    visualDensity: VisualDensity.compact,
                    padding: EdgeInsets.zero,
                    color: theme.colorScheme.onSurfaceVariant,
                    onPressed: widget.onAddShare,
                  ),
                if (hasShares)
                  AnimatedRotation(
                    turns: _expanded ? 0.5 : 0.0,
                    duration: Durations.short3,
                    child: Icon(
                      Icons.keyboard_arrow_down_rounded,
                      color: theme.colorScheme.onSurfaceVariant,
                    ),
                  ),
              ],
            ),
            onTap: hasShares
                ? () => setState(() => _expanded = !_expanded)
                : null,
          ),
          AnimatedCrossFade(
            firstChild: const SizedBox(width: double.infinity),
            secondChild: _KeyShareList(devices: widget.devices),
            crossFadeState: _expanded && hasShares
                ? CrossFadeState.showSecond
                : CrossFadeState.showFirst,
            duration: Durations.short4,
          ),
        ],
      ),
    );
  }
}

/// Contributed key shares under an expanded participant card —
/// keygen's `_DeviceList` shape.
class _KeyShareList extends StatelessWidget {
  const _KeyShareList({required this.devices});
  final List<String> devices;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Container(
      width: double.infinity,
      color: theme.colorScheme.surfaceContainerHighest,
      padding: const EdgeInsets.fromLTRB(72, 4, 16, 12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          for (final name in devices)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 4),
              child: Row(
                children: [
                  Icon(
                    Icons.key,
                    size: 16,
                    color: theme.colorScheme.onSurfaceVariant,
                  ),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      name,
                      style: theme.textTheme.bodyMedium,
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
                ],
              ),
            ),
        ],
      ),
    );
  }
}

class _ShareProgress extends StatelessWidget {
  final RecoveryLobbyState state;
  const _ShareProgress({required this.state});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final total = state.shares.length;
    final threshold = state.metadata.thresholdHint;
    final recovered = state.currentRecovery != null;
    return Card.filled(
      margin: EdgeInsets.zero,
      color: recovered
          ? theme.colorScheme.primaryContainer
          : theme.colorScheme.surfaceContainerHighest,
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Row(
          children: [
            Icon(
              recovered ? Icons.check_circle : Icons.hourglass_top,
              color: recovered
                  ? theme.colorScheme.primary
                  : theme.colorScheme.onSurfaceVariant,
            ),
            const SizedBox(width: 12),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    recovered ? 'Recovery available' : 'Waiting for key shares',
                    style: theme.textTheme.titleMedium,
                  ),
                  Text(
                    threshold == null
                        ? '$total key shares posted'
                        : '$total of $threshold key shares posted',
                    style: theme.textTheme.bodySmall,
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _FinishedBanner extends StatelessWidget {
  final bool persisting;
  final AccessStructureRef? recoveredRef;
  final String? error;

  const _FinishedBanner({
    required this.persisting,
    required this.recoveredRef,
    required this.error,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final IconData icon;
    final String title;
    final Color color;
    if (error != null) {
      icon = Icons.error_outline;
      title = 'Persist failed';
      color = theme.colorScheme.error;
    } else if (recoveredRef != null) {
      icon = Icons.check_circle_outline;
      title = 'Recovered — wallet available';
      color = theme.colorScheme.primary;
    } else if (persisting) {
      icon = Icons.sync;
      title = 'Persisting on this device…';
      color = theme.colorScheme.onSurfaceVariant;
    } else {
      icon = Icons.hourglass_top;
      title = 'Finished — awaiting persist';
      color = theme.colorScheme.onSurfaceVariant;
    }
    return Card.filled(
      margin: EdgeInsets.zero,
      color: theme.colorScheme.surfaceContainerHighest,
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Row(
          children: [
            Icon(icon, color: color),
            const SizedBox(width: 12),
            Expanded(child: Text(title, style: theme.textTheme.titleMedium)),
          ],
        ),
      ),
    );
  }
}
