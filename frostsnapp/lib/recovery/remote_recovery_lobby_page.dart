import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnap/nostr_chat/nostr_profile.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/coordinator.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/src/rust/api/nostr.dart';
import 'package:frostsnap/src/rust/api/nostr/remote_keygen.dart';
import 'package:frostsnap/src/rust/api/nostr/remote_recovery.dart';
import 'package:frostsnap/src/rust/api/recovery.dart';

/// Owns a `RemoteRecoveryLobbyHandle`, subscribes to its state
/// broadcast, and renders the recovery lobby ceremony.
///
/// Lifecycle per the `nostr_recovery_transport` plan: the handle
/// is already started by the caller (`create_remote_recovery_lobby`
/// / `join_remote_recovery_lobby` returns only after the bridge's
/// first `StateChanged` seeds the broadcast). This page just
/// subscribes via `handle.subState().watch()` — no `start()`, no
/// `close()`.
class RemoteRecoveryLobbyPage extends StatefulWidget {
  final RemoteRecoveryLobbyHandle handle;
  final bool isLeader;
  final Coordinator coord;
  final SymmetricKey encryptionKey;

  const RemoteRecoveryLobbyPage({
    super.key,
    required this.handle,
    required this.isLeader,
    required this.coord,
    required this.encryptionKey,
  });

  @override
  State<RemoteRecoveryLobbyPage> createState() =>
      _RemoteRecoveryLobbyPageState();
}

class _RemoteRecoveryLobbyPageState extends State<RemoteRecoveryLobbyPage> {
  RecoveryLobbyState? _state;
  StreamSubscription<RecoveryLobbyState>? _sub;
  bool _finishing = false;
  bool _persisting = false;
  String? _error;
  AccessStructureRef? _recoveredRef;
  bool _verificationFailed = false;

  late final PublicKey _myPubkey;

  @override
  void initState() {
    super.initState();
    _myPubkey = widget.handle.myPubkey();
    _sub = widget.handle.subState().watch().listen(_onState);
    // Side-channel: FinishVerificationFailed is not exposed on the
    // state broadcast (the transport surfaces it via awaitFinished()
    // returning Err). Fire-and-forget so we surface the error banner
    // without blocking the state pipeline.
    unawaited(_watchFinished());
  }

  /// Watches the `awaitFinished()` future. On success we don't need
  /// to do anything — the state broadcast already carries the
  /// `finished` field and the `_onState` handler kicks off persist.
  /// On error the transport is signalling FinishVerificationFailed
  /// (the leader picked a subset that doesn't reconstruct); flip
  /// the flag so the Recover button locks and the banner appears.
  Future<void> _watchFinished() async {
    try {
      await widget.handle.awaitFinished();
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _verificationFailed = true;
        _error =
            "The leader's Finish message doesn't match — protocol bug or malicious leader. Aborting.";
      });
    }
  }

  void _onState(RecoveryLobbyState state) {
    if (!mounted) return;
    setState(() => _state = state);
    if (state.finished != null &&
        _recoveredRef == null &&
        !_persisting &&
        _error == null) {
      unawaited(_persist());
    }
  }

  Future<void> _persist() async {
    setState(() => _persisting = true);
    try {
      final asref = await widget.handle.persistRecovered(
        coord: widget.coord,
        encryptionKey: widget.encryptionKey,
      );
      if (!mounted) return;
      setState(() => _recoveredRef = asref);
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (!mounted) return;
        Navigator.of(context).pop(asref);
      });
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = '$e');
    } finally {
      if (mounted) setState(() => _persisting = false);
    }
  }

  Future<void> _finish() async {
    final winning = _state?.currentRecovery?.winningShareRefs;
    if (winning == null || winning.isEmpty) return;
    setState(() {
      _finishing = true;
      _error = null;
    });
    try {
      await widget.handle.finish(shareRefs: winning);
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = '$e');
    } finally {
      if (mounted) setState(() => _finishing = false);
    }
  }

  Future<void> _cancel() async {
    try {
      await widget.handle.cancel();
    } catch (e) {
      if (mounted) setState(() => _error = '$e');
    }
  }

  /// Load-share flow: pick a connected Frostsnap device, tell it to
  /// enter its physical backup (same orchestration as
  /// `restoration/recovery_flow.dart`), wait for the resulting
  /// `PhysicalBackupPhase`, unpack it into a `SharePost`, and
  /// publish via `handle.postShare`.
  Future<void> _loadShare() async {
    final devices = widget.coord.deviceListState().devices;
    if (devices.isEmpty) {
      if (!mounted) return;
      setState(() => _error = 'No devices connected. Plug in a Frostsnap.');
      return;
    }
    final selected = await showModalBottomSheet<ConnectedDevice>(
      context: context,
      builder: (ctx) => _DeviceChooserSheet(devices: devices),
    );
    if (selected == null || !mounted) return;

    final controller = _BackupEntryController();
    final subscription = widget.coord
        .tellDeviceToEnterPhysicalBackup(deviceId: selected.id)
        .listen(controller.onEvent);
    try {
      await showDialog<void>(
        context: context,
        barrierDismissible: false,
        builder: (ctx) => _AwaitBackupDialog(controller: controller),
      );
      final phase = controller.enteredPhase;
      if (phase == null) {
        if (!mounted) return;
        setState(
          () => _error = controller.abortMessage ?? 'Backup entry cancelled',
        );
        return;
      }
      final deviceName =
          widget.coord.getDeviceName(id: selected.id) ?? 'device';
      final post = SharePost(
        deviceId: phase.deviceId(),
        deviceName: deviceName,
        deviceKind: DeviceKind.frostsnap,
        shareImage: phase.shareImage(),
        needsConsolidation: true,
      );
      await widget.handle.postShare(post: post);
      if (!mounted) return;
      setState(() => _error = null);
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = '$e');
    } finally {
      await subscription.cancel();
    }
  }

  @override
  void dispose() {
    _sub?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return RecoveryLobbyView(
      state: _state,
      isLeader: widget.isLeader,
      myPubkey: _myPubkey,
      inviteLink: widget.handle.inviteLink(),
      finishing: _finishing,
      persisting: _persisting,
      error: _error,
      recoveredRef: _recoveredRef,
      verificationFailed: _verificationFailed,
      onFinish: _finish,
      onCancel: _cancel,
      onLoadShare: _loadShare,
    );
  }
}

/// Pure state → UI mapping for the recovery lobby, extracted from
/// [RemoteRecoveryLobbyPage] so widget tests can drive it directly
/// without standing up a live `RemoteRecoveryLobbyHandle`. The
/// hosting page owns the handle subscription and passes down the
/// current state + action callbacks.
class RecoveryLobbyView extends StatelessWidget {
  final RecoveryLobbyState? state;
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

  /// Callback for the "Load share" button — null in tests that
  /// don't exercise the post-share path.
  final Future<void> Function()? onLoadShare;

  const RecoveryLobbyView({
    super.key,
    required this.state,
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
    this.onLoadShare,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final s = state;
    return Scaffold(
      appBar: AppBar(
        title: Text(isLeader ? 'Recovery lobby (leader)' : 'Recovery lobby'),
        actions: [
          if (isLeader && s != null && !s.cancelled)
            IconButton(
              tooltip: 'Cancel lobby',
              icon: const Icon(Icons.close),
              onPressed: onCancel,
            ),
        ],
      ),
      body: s == null
          ? const Center(child: CircularProgressIndicator())
          : ListView(
              padding: const EdgeInsets.all(16),
              children: [
                _MetadataHeader(metadata: s.metadata),
                const SizedBox(height: 16),
                _InviteTile(inviteLink: inviteLink),
                const SizedBox(height: 24),
                _ParticipantList(state: s, myPubkey: myPubkey),
                const SizedBox(height: 24),
                _ShareProgress(state: s),
                const SizedBox(height: 16),
                if (onLoadShare != null &&
                    s.finished == null &&
                    !s.cancelled) ...[
                  OutlinedButton.icon(
                    icon: const Icon(Icons.upload),
                    label: const Text('Load share'),
                    onPressed: onLoadShare,
                  ),
                  const SizedBox(height: 16),
                ],
                if (isLeader)
                  _LeaderRecoverButton(
                    canRecover:
                        s.currentRecovery != null &&
                        s.finished == null &&
                        !verificationFailed,
                    finishing: finishing,
                    onPressed: onFinish,
                  ),
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
    );
  }
}

class _MetadataHeader extends StatelessWidget {
  final RecoveryChannelMetadata metadata;
  const _MetadataHeader({required this.metadata});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(metadata.keyName, style: theme.textTheme.headlineSmall),
        const SizedBox(height: 4),
        Text(
          metadata.thresholdHint == null
              ? 'Recovering — threshold unknown'
              : 'Recovering — target ${metadata.thresholdHint}-of-N',
          style: theme.textTheme.bodyMedium?.copyWith(
            color: theme.colorScheme.onSurfaceVariant,
          ),
        ),
      ],
    );
  }
}

class _InviteTile extends StatelessWidget {
  final String inviteLink;
  const _InviteTile({required this.inviteLink});

  @override
  Widget build(BuildContext context) {
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Row(
          children: [
            const Icon(Icons.link),
            const SizedBox(width: 12),
            Expanded(
              child: Text(
                inviteLink,
                style: Theme.of(context).textTheme.bodyMedium,
                overflow: TextOverflow.ellipsis,
              ),
            ),
            IconButton(
              tooltip: 'Copy invite link',
              icon: const Icon(Icons.copy),
              onPressed: () async {
                await Clipboard.setData(ClipboardData(text: inviteLink));
                if (context.mounted) {
                  ScaffoldMessenger.of(context).showSnackBar(
                    const SnackBar(content: Text('Invite link copied')),
                  );
                }
              },
            ),
          ],
        ),
      ),
    );
  }
}

class _ParticipantList extends StatelessWidget {
  final RecoveryLobbyState state;
  final PublicKey myPubkey;
  const _ParticipantList({required this.state, required this.myPubkey});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final entries = state.participants.values.toList()
      ..sort((a, b) => a.joinedAtSecs.compareTo(b.joinedAtSecs));
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          'Participants (${entries.length})',
          style: theme.textTheme.titleMedium,
        ),
        const SizedBox(height: 8),
        for (final p in entries)
          _ParticipantRow(info: p, isMe: p.pubkey == myPubkey),
      ],
    );
  }
}

class _ParticipantRow extends StatelessWidget {
  final RecoveryParticipantInfo info;
  final bool isMe;
  const _ParticipantRow({required this.info, required this.isMe});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Row(
        children: [
          NostrAvatar.small(pubkey: info.pubkey, profile: info.profile),
          const SizedBox(width: 12),
          Expanded(
            child: Text(
              _displayName(info),
              style: theme.textTheme.bodyLarge,
              overflow: TextOverflow.ellipsis,
            ),
          ),
          if (info.postedShares.isNotEmpty)
            Chip(
              label: Text('${info.postedShares.length} shares'),
              visualDensity: VisualDensity.compact,
            ),
          if (info.left) const SizedBox(width: 8),
          if (info.left) Text('left', style: theme.textTheme.bodySmall),
        ],
      ),
    );
  }

  String _displayName(RecoveryParticipantInfo info) {
    final profileName = info.profile?.displayName ?? info.profile?.name;
    final base = profileName != null && profileName.isNotEmpty
        ? profileName
        : info.pubkey.toNpub();
    return isMe ? '$base (You)' : base;
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
    return Card(
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
                    recovered ? 'Recovery available' : 'Waiting for shares',
                    style: theme.textTheme.titleMedium,
                  ),
                  Text(
                    threshold == null
                        ? '$total posted'
                        : '$total posted (target $threshold)',
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

class _LeaderRecoverButton extends StatelessWidget {
  final bool canRecover;
  final bool finishing;
  final Future<void> Function() onPressed;

  const _LeaderRecoverButton({
    required this.canRecover,
    required this.finishing,
    required this.onPressed,
  });

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: double.infinity,
      child: FilledButton.icon(
        icon: finishing
            ? const SizedBox(
                width: 16,
                height: 16,
                child: CircularProgressIndicator(strokeWidth: 2),
              )
            : const Icon(Icons.check),
        label: Text(finishing ? 'Finalizing…' : 'Recover'),
        onPressed: canRecover && !finishing ? () => onPressed() : null,
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
    return Card(
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

/// Bottom sheet listing connected devices — user picks the device
/// whose backup they'll enter.
class _DeviceChooserSheet extends StatelessWidget {
  final List<ConnectedDevice> devices;
  const _DeviceChooserSheet({required this.devices});

  @override
  Widget build(BuildContext context) {
    return SafeArea(
      child: ListView(
        shrinkWrap: true,
        children: [
          const Padding(
            padding: EdgeInsets.all(16),
            child: Text('Pick a device to enter your share'),
          ),
          for (final d in devices)
            ListTile(
              leading: const Icon(Icons.usb),
              title: Text(d.name ?? 'Unnamed'),
              onTap: () => Navigator.of(context).pop(d),
            ),
        ],
      ),
    );
  }
}

/// Collects the terminal event from a `tellDeviceToEnterPhysicalBackup`
/// stream so the awaiting dialog can pop when either `entered` or
/// `abort` lands.
class _BackupEntryController extends ChangeNotifier {
  PhysicalBackupPhase? enteredPhase;
  String? abortMessage;
  bool _resolved = false;

  void onEvent(EnterPhysicalBackupState state) {
    if (_resolved) return;
    if (state.entered != null) {
      enteredPhase = state.entered;
      _resolved = true;
      notifyListeners();
    } else if (state.abort != null) {
      abortMessage = state.abort;
      _resolved = true;
      notifyListeners();
    }
  }
}

class _AwaitBackupDialog extends StatefulWidget {
  final _BackupEntryController controller;
  const _AwaitBackupDialog({required this.controller});

  @override
  State<_AwaitBackupDialog> createState() => _AwaitBackupDialogState();
}

class _AwaitBackupDialogState extends State<_AwaitBackupDialog> {
  @override
  void initState() {
    super.initState();
    widget.controller.addListener(_onResolved);
  }

  void _onResolved() {
    if (!mounted) return;
    Navigator.of(context).pop();
  }

  @override
  void dispose() {
    widget.controller.removeListener(_onResolved);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: const Text('Enter your share on the device'),
      content: const Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Padding(
            padding: EdgeInsets.symmetric(vertical: 16),
            child: CircularProgressIndicator(),
          ),
          Text(
            'Use the device to type your key number and 25 seed '
            'words. This dialog closes once the device reports success.',
            textAlign: TextAlign.center,
          ),
        ],
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('Cancel'),
        ),
      ],
    );
  }
}
