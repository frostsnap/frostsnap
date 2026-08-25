import 'package:collection/collection.dart';
import 'package:flutter/material.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/theme.dart';

/// Whether a genuine certificate key was compiled into this build. Read once: it is
/// a compile-time constant on the Rust side, so calling across FFI on every badge
/// build would be a round-trip per device per frame.
final bool genuineCheckEnabled = coord.genuineCheckEnabled();

/// How a device's genuine-check result is rendered.
///
/// Note what this file does *not* import: `device_colors.dart`. The case colour
/// arrives in an unverified certificate, so a device chooses it — and a device must
/// not get to influence how its own authenticity is drawn. Tinting a "Genuine" pill
/// with the case colour gave a red device a red badge (red being the universal
/// error colour) and made a black device's badge invisible against the dark theme.
/// The palette below is fixed and derived only from the status.
extension GenuineStatusExt on GenuineStatus {
  IconData get icon => switch (this) {
    GenuineStatus.genuine => Icons.verified_rounded,
    GenuineStatus.failed => Icons.gpp_bad_rounded,
    GenuineStatus.firmwareTooOld => Icons.system_update_rounded,
    GenuineStatus.unknown => Icons.gpp_maybe_rounded,
  };

  /// Caution amber rather than error red for a failure: a single dishonest device
  /// cannot by itself compromise a multi-device wallet, so this is a "stop and look"
  /// signal, not a catastrophe. Everything else is neutral, because "not verified"
  /// is the ordinary state for dev units, third-party hardware and older firmware,
  /// and dressing it as a warning would train people to ignore the badge.
  Color color(ColorScheme scheme) => switch (this) {
    GenuineStatus.genuine => scheme.primary,
    GenuineStatus.failed => cautionColor,
    GenuineStatus.firmwareTooOld => scheme.onSurfaceVariant,
    GenuineStatus.unknown => scheme.onSurfaceVariant,
  };

  String get label => switch (this) {
    GenuineStatus.genuine => 'Genuine',
    GenuineStatus.failed => 'Not genuine',
    GenuineStatus.firmwareTooOld => 'Update to verify',
    GenuineStatus.unknown => 'Unverified',
  };

  /// Title and body for the explainer dialog and the device-details row.
  (String, String) get explanation => switch (this) {
    GenuineStatus.genuine => (
      'Genuine device',
      'This device proved it is genuine Frostsnap hardware, using a key sealed '
          'into its chip at the factory.\n\n'
          'The proof is tied to this device and to this connection, so it cannot '
          'be copied from another device or replayed.',
    ),
    GenuineStatus.failed => (
      'Not genuine',
      'This device answered the authenticity challenge, but its proof did not '
          'verify. It may be counterfeit, tampered with, or relaying another '
          "device's answer.\n\n"
          'A single device cannot move funds on its own, so this is not an '
          'emergency — but be careful about relying on it, and get in touch if '
          'you did not expect it.',
    ),
    GenuineStatus.firmwareTooOld => (
      'Update to verify',
      'This device is running firmware from before authenticity checks existed, '
          'so it cannot prove it is genuine — however sound the hardware is.\n\n'
          'Update its firmware and it will be checked automatically the next '
          'time you connect it.',
    ),
    GenuineStatus.unknown => (
      'Not verified',
      'This device has not proved that it was made by Frostsnap.\n\n'
          'That is expected for a development unit, third-party or open-source '
          'hardware, or a device with no factory certificate.\n\n'
          'Otherwise, treat it with caution.',
    ),
  };
}

/// Explains what a genuine status means. Kept in-app rather than linking out, in
/// keeping with the other explainers here.
void showGenuineExplanation(BuildContext context, GenuineStatus status) {
  final (title, body) = status.explanation;
  showDialog<void>(
    context: context,
    builder: (context) {
      final theme = Theme.of(context);
      return BackdropFilter(
        filter: blurFilter,
        child: AlertDialog(
          constraints: dialogConstraints,
          icon: Icon(
            status.icon,
            color: status.color(theme.colorScheme),
            size: 28,
          ),
          title: Text(title),
          content: Text(
            body,
            style: theme.textTheme.bodyMedium?.copyWith(
              color: theme.colorScheme.onSurfaceVariant,
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context),
              child: const Text('Got it'),
            ),
          ],
        ),
      );
    },
  );
}

/// Compact status pill. Tapping it explains the status.
///
/// Renders nothing at all when this build has no genuine certificate key compiled
/// in: with no check attempted, any badge — even a neutral one — would imply we
/// looked.
class GenuineBadge extends StatelessWidget {
  const GenuineBadge({super.key, required this.status});

  /// Tracks a device by id, repainting as its status resolves. Use this on screens
  /// that only hold a [DeviceId]: the status arrives asynchronously, so a one-shot
  /// read in `build` would freeze on whatever was true at first paint.
  static Widget forDeviceId(DeviceId id) => StreamBuilder<DeviceListUpdate>(
    stream: GlobalStreams.deviceListSubject,
    builder: (context, snapshot) {
      // `deviceIdEquals`, never `==`: the generated `DeviceId ==` compares the
      // underlying byte list, which is a plain Dart List and so compares by
      // reference — two ids decoded from separate FFI calls never match.
      final device = snapshot.data?.state.devices.firstWhereOrNull(
        (device) => deviceIdEquals(device.id, id),
      );
      if (device == null) return const SizedBox.shrink();
      return GenuineBadge(status: device.genuine);
    },
  );

  final GenuineStatus status;

  @override
  Widget build(BuildContext context) {
    if (!genuineCheckEnabled) return const SizedBox.shrink();
    final scheme = Theme.of(context).colorScheme;
    final color = status.color(scheme);

    return InkWell(
      onTap: () => showGenuineExplanation(context, status),
      borderRadius: BorderRadius.circular(20),
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
        decoration: BoxDecoration(
          color: color.withValues(alpha: 0.12),
          borderRadius: BorderRadius.circular(20),
          border: Border.all(color: color.withValues(alpha: 0.4)),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(status.icon, size: 14, color: color),
            const SizedBox(width: 4),
            Text(
              status.label,
              style: TextStyle(
                color: color,
                fontSize: 11,
                fontWeight: FontWeight.w500,
              ),
            ),
          ],
        ),
      ),
    );
  }
}
