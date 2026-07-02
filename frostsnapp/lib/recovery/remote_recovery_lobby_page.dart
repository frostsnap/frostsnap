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
    final canRecover =
        s != null &&
        s.currentRecovery != null &&
        s.finished == null &&
        !verificationFailed;

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
                  const SizedBox(height: 4),
                  ..._participantRows(s),
                  const SizedBox(height: 12),
                  if (isLeader && s.finished == null && !s.cancelled) ...[
                    InviteTile(
                      onTap: () => showInviteDialog(context, inviteLink),
                    ),
                    const SizedBox(height: 16),
                  ],
                  _ShareProgress(state: s),
                  const SizedBox(height: 16),
                  if (onLoadShare != null &&
                      s.finished == null &&
                      !s.cancelled) ...[
                    // NOT a glowy card — in this app's design
                    // language a glowing border means "you can plug
                    // in a device right now", which only the
                    // discovery screen behind this button offers.
                    if (s.currentRecovery == null)
                      _LoadShareTile(onTap: () => onLoadShare!())
                    else
                      TextButton.icon(
                        icon: const Icon(Icons.add),
                        label: const Text('Add another key share'),
                        onPressed: () => onLoadShare!(),
                      ),
                    const SizedBox(height: 16),
                  ],
                  if (s.finished != null) ...[
                    _FinishedBanner(
                      persisting: persisting,
                      recoveredRef: recoveredRef,
                      error: error,
                    ),
                    const SizedBox(height: 16),
                  ],
                  if (s.cancelled) ...[
                    Text(
                      'Recovery lobby cancelled by leader.',
                      style: theme.textTheme.titleMedium?.copyWith(
                        color: theme.colorScheme.error,
                      ),
                    ),
                    const SizedBox(height: 16),
                  ],
                  if (error != null)
                    Text(
                      error!,
                      style: TextStyle(color: theme.colorScheme.error),
                    ),
                ],
              ),
      ),
      footer: Row(
        children: [
          // While the lobby is live: leader can cancel it, joiner can
          // leave it (publishing Leave so peers see them go). Once
          // it's cancelled or finished there's nothing to announce —
          // just Close.
          if (s != null && !s.cancelled && s.finished == null)
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
          if (isLeader)
            FilledButton.icon(
              icon: finishing
                  ? const SizedBox(
                      width: 16,
                      height: 16,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    )
                  : const Icon(Icons.check),
              label: Text(finishing ? 'Finalizing…' : 'Recover'),
              onPressed: canRecover && !finishing ? () => onFinish() : null,
            ),
        ],
      ),
    );
  }

  List<Widget> _participantRows(RecoveryLobbyState s) {
    final entries = s.participants.values.toList()
      ..sort((a, b) => a.joinedAtSecs.compareTo(b.joinedAtSecs));
    return [
      for (final p in entries)
        _ParticipantRow(
          info: p,
          profile: snapshot?.profileOf(p.pubkey),
          isMe: p.pubkey == myPubkey,
          devices: [
            for (final ref in p.postedShares)
              for (final share in s.shares)
                if (share.eventId == ref) share.post.deviceName,
          ],
        ),
    ];
  }
}

class _LoadShareTile extends StatelessWidget {
  final VoidCallback onTap;
  const _LoadShareTile({required this.onTap});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Card.filled(
      margin: EdgeInsets.zero,
      color: theme.colorScheme.surfaceContainerHigh,
      clipBehavior: Clip.hardEdge,
      child: ListTile(
        onTap: onTap,
        contentPadding: const EdgeInsets.symmetric(horizontal: 16, vertical: 4),
        leading: Icon(Icons.usb_rounded, color: theme.colorScheme.primary),
        title: const Text('Load key share'),
        subtitle: const Text('Plug in a Frostsnap or enter a seed-word backup'),
        trailing: const Icon(Icons.chevron_right_rounded),
      ),
    );
  }
}

class _ParticipantRow extends StatelessWidget {
  final RecoveryParticipantInfo info;
  final NostrProfile? profile;
  final bool isMe;

  /// Device names of the key shares this participant has posted
  /// (resolved from the state's share list by event ref).
  final List<String> devices;

  const _ParticipantRow({
    required this.info,
    required this.profile,
    required this.isMe,
    required this.devices,
  });

  // The channel-member surface is the ONLY name source — including
  // for self. Showing the locally-known settings name would let a
  // broken in-channel profile publish go unnoticed: you'd see your
  // name while peers see your pubkey.
  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 6),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              NostrAvatar.small(pubkey: info.pubkey, profile: profile),
              const SizedBox(width: 12),
              Expanded(
                child: Text(
                  _displayName(context, profile),
                  style: theme.textTheme.bodyLarge,
                  overflow: TextOverflow.ellipsis,
                ),
              ),
              Text(
                info.left ? 'Left' : (devices.isEmpty ? 'Joined' : 'Ready'),
                style: theme.textTheme.bodySmall?.copyWith(
                  color: theme.colorScheme.onSurfaceVariant,
                ),
              ),
            ],
          ),
          if (devices.isNotEmpty)
            Padding(
              padding: const EdgeInsets.only(left: 48, top: 2),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  for (final name in devices)
                    Padding(
                      padding: const EdgeInsets.symmetric(vertical: 2),
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
                              style: theme.textTheme.bodyMedium?.copyWith(
                                color: theme.colorScheme.onSurfaceVariant,
                              ),
                              overflow: TextOverflow.ellipsis,
                            ),
                          ),
                        ],
                      ),
                    ),
                ],
              ),
            ),
        ],
      ),
    );
  }

  static String? _profileName(NostrProfile? profile) {
    final name = profile?.displayName ?? profile?.name;
    return (name != null && name.isNotEmpty) ? name : null;
  }

  String _displayName(BuildContext context, NostrProfile? profile) {
    final name = _profileName(profile);
    if (isMe) {
      return name != null ? '$name (You)' : 'You';
    }
    return name ?? _shortPubkey(info.pubkey);
  }

  static String _shortPubkey(PublicKey pk) => pk.toHex().substring(0, 8);
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
