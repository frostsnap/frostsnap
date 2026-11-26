import 'package:flutter/material.dart';
import 'package:frostsnap/certificate_details.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/theme.dart';

/// Whether a genuine certificate key was compiled into this build. Read once: it is
/// a compile-time constant on the Rust side, so calling across FFI on every badge
/// build would be a round-trip per device per frame. Assignable only so widget tests, which have
/// no coordinator, can choose it before its first read.
bool genuineCheckEnabled = coord.genuineCheckEnabled();

/// [device]'s genuine status, or `null` when this build cannot check: with no check attempted, any
/// status — even a neutral one — would imply we looked.
GenuineStatus? checkedGenuineStatus(ConnectedDevice device) =>
    genuineCheckEnabled ? device.genuine : null;

enum GenuineLook { genuine, updateToVerify, unverified }

/// The badge's colours come only from the status, never the case colour: tinting a "Genuine" pill
/// with it gave a red device a red badge (red being the universal error colour) and made a black
/// device's badge invisible against the dark theme.
extension GenuineStatusExt on GenuineStatus {
  GenuineLook get look => switch (this) {
    GenuineStatus_Genuine() => GenuineLook.genuine,
    GenuineStatus_Unverified(firmwareSupportsCheck: false) =>
      GenuineLook.updateToVerify,
    GenuineStatus_Unverified() => GenuineLook.unverified,
  };

  IconData get icon => look.icon;
  Color color(ColorScheme scheme) => look.color(scheme);
  String get label => look.label;
  (String, String) get explanation => look.explanation;
}

extension GenuineLookExt on GenuineLook {
  IconData get icon => switch (this) {
    GenuineLook.genuine => Icons.verified_rounded,
    GenuineLook.updateToVerify => Icons.system_update_rounded,
    GenuineLook.unverified => Icons.gpp_maybe_rounded,
  };

  /// Neutral unless genuine: "unverified" is the ordinary state for dev units, third-party
  /// hardware and older firmware, and dressing it as a warning would train people to ignore the
  /// badge.
  Color color(ColorScheme scheme) => switch (this) {
    GenuineLook.genuine => scheme.primary,
    GenuineLook.updateToVerify => scheme.onSurfaceVariant,
    GenuineLook.unverified => scheme.onSurfaceVariant,
  };

  String get label => switch (this) {
    GenuineLook.genuine => 'Genuine',
    GenuineLook.updateToVerify => 'Update to verify',
    GenuineLook.unverified => 'Unverified',
  };

  (String, String) get explanation => switch (this) {
    GenuineLook.genuine => (
      'Genuine device',
      'The app checks that a device was made at the Frostsnap factory each '
          'time you connect it. This device passed that check.',
    ),
    GenuineLook.updateToVerify => (
      'Update to verify',
      'The app checks that a device was made at the Frostsnap factory each '
          'time you connect it, but this device runs firmware from before that '
          'check existed.\n\n'
          'Update its firmware and it will be checked the next time you connect '
          'it.',
    ),
    GenuineLook.unverified => (
      'Unverified device',
      'The app checks that a device was made at the Frostsnap factory each '
          'time you connect it. This device has not passed that check.\n\n'
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
          content: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                body,
                style: theme.textTheme.bodyMedium?.copyWith(
                  color: theme.colorScheme.onSurfaceVariant,
                ),
              ),
              if (status case GenuineStatus_Genuine(:final certificate)) ...[
                const SizedBox(height: 16),
                CertificateDetails(certificate: certificate),
              ],
            ],
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
