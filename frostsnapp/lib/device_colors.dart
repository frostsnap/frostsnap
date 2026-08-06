import 'package:flutter/material.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';

/// Colors and styling for a device based on its case color
class DeviceColorScheme {
  /// Caution amber (not panic red) for a device that failed verification: even a
  /// failed device can't compromise a multi-device wallet on its own.
  static const Color failedColor = Color(0xFFD98A00);

  final Color deviceColor;
  final CaseColor? caseColor;
  final GenuineStatus? genuine;

  DeviceColorScheme({required this.deviceColor, this.caseColor, this.genuine});

  /// Get device color scheme from a DeviceId (works even when disconnected)
  factory DeviceColorScheme.fromDeviceId(
    BuildContext context,
    DeviceId deviceId,
  ) {
    final deviceList = coord.deviceListState();

    ConnectedDevice? connectedDevice;
    try {
      connectedDevice = deviceList.devices.firstWhere((d) => d.id == deviceId);
    } catch (_) {
      connectedDevice = null;
    }

    final caseColor =
        connectedDevice?.caseColor ?? coord.getDeviceCaseColor(id: deviceId);

    return DeviceColorScheme(
      deviceColor: caseColor?.toColor() ?? Colors.transparent,
      caseColor: caseColor,
      genuine: connectedDevice?.genuine,
    );
  }

  /// Get device color scheme from a ConnectedDevice
  factory DeviceColorScheme.fromDevice(
    BuildContext context,
    ConnectedDevice? device,
  ) {
    final caseColor = device?.caseColor;
    return DeviceColorScheme(
      deviceColor: caseColor?.toColor() ?? Colors.transparent,
      caseColor: caseColor,
      genuine: device?.genuine,
    );
  }

  /// True when the device responded but its proof did not verify (counterfeit or
  /// a relay attempt).
  bool get genuineFailed => genuine == GenuineStatus.failed;

  /// Device color for icon tinting.
  Color? get accent => caseColor != null ? deviceColor : null;

  /// Compact status pill (Genuine / Unknown / Not genuine). Null when this build
  /// doesn't run the check, so we never imply authenticity we didn't verify.
  Widget? genuineBadge(BuildContext context) {
    if (!coord.genuineCheckEnabled()) return null;
    final scheme = Theme.of(context).colorScheme;
    final (icon, color) = _statusIconColor(scheme);
    // Unknown is neutral by design: usually benign (old firmware, no cert, a
    // third-party device) and never something the user must act on.
    final label = switch (genuine) {
      GenuineStatus.genuine => 'Genuine',
      GenuineStatus.failed => 'Not genuine',
      _ => 'Unknown',
    };
    // Opens the same explanation dialog as the device-details Authenticity row.
    return GestureDetector(
      onTap: () => showGenuineExplanation(context),
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
            Icon(icon, size: 14, color: color),
            const SizedBox(width: 4),
            Text(
              label,
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

  /// Icon + colour for the current status. Shared by badge, row, and dialog.
  (IconData, Color) _statusIconColor(ColorScheme scheme) => switch (genuine) {
    GenuineStatus.genuine => (
      Icons.verified_rounded,
      caseColor != null ? deviceColor : scheme.primary,
    ),
    GenuineStatus.failed => (Icons.gpp_bad_rounded, failedColor),
    _ => (Icons.gpp_maybe_rounded, scheme.onSurfaceVariant),
  };

  /// Show the status explanation dialog. Shared by the badge and the
  /// device-details Authenticity row. Styled to match the app's dialogs
  /// (see `showErrorDialog` in theme.dart).
  void showGenuineExplanation(BuildContext context) {
    final (title, body) = genuineExplanation();
    showDialog(
      context: context,
      builder: (context) {
        final theme = Theme.of(context);
        final cs = theme.colorScheme;
        final (icon, color) = _statusIconColor(cs);
        return AlertDialog(
          constraints: const BoxConstraints(maxWidth: 560),
          icon: Icon(icon, color: color, size: 28),
          iconPadding: const EdgeInsets.only(top: 24),
          title: Text(
            title,
            style: theme.textTheme.headlineSmall?.copyWith(color: cs.onSurface),
          ),
          content: Text(
            body,
            style: theme.textTheme.bodyMedium?.copyWith(
              color: cs.onSurfaceVariant,
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context),
              child: const Text('Got it'),
            ),
          ],
        );
      },
    );
  }

  /// User-facing (title, body) explaining the current genuine status.
  (String, String) genuineExplanation() {
    switch (genuine) {
      case GenuineStatus.genuine:
        return (
          'Genuine Device',
          'This device proved it is genuine Frostsnap hardware, signed by the '
              'factory.',
        );
      case GenuineStatus.failed:
        return (
          'Not Genuine',
          'This device answered the authenticity challenge but its proof did '
              'not verify. It may be counterfeit or tampered with.\n\n'
              'Be cautious about relying on it, and get in touch if you did '
              'not expect this.',
        );
      default:
        return (
          'Unverified Device',
          'The device has not proven that it was manufactured by Frostsnap.\n\n'
              'If you are a developer, or using an open-source or third-party '
              'device, or a device on older firmware, this is expected.\n\n'
              'Otherwise, treat it with caution.',
        );
    }
  }

  /// Content for the device-details authenticity row, where the status itself
  /// is the title. Null when the check isn't active in this build.
  ({IconData icon, Color color, String title, String subtitle})? genuineRow(
    BuildContext context,
  ) {
    if (!coord.genuineCheckEnabled()) return null;
    final scheme = Theme.of(context).colorScheme;
    final (icon, color) = _statusIconColor(scheme);
    final (title, subtitle) = switch (genuine) {
      GenuineStatus.genuine => (
        'Genuine Device',
        'Verified genuine Frostsnap hardware.',
      ),
      GenuineStatus.failed => (
        'Not Genuine',
        'This device may be counterfeit or tampered with.',
      ),
      _ => (
        'Unknown Device',
        'Could not verify this is a genuine Frostsnap device.',
      ),
    };
    return (icon: icon, color: color, title: title, subtitle: subtitle);
  }

  /// Card with a coloured border and glow in the device's case colour. The glow
  /// is identity, not trust (the badge carries that).
  Widget buildGlowCard({
    required Widget child,
    EdgeInsets margin = EdgeInsets.zero,
    Clip clipBehavior = Clip.hardEdge,
  }) {
    final glowColor = caseColor != null ? deviceColor : null;
    if (glowColor == null) {
      return Card.filled(
        margin: margin,
        clipBehavior: clipBehavior,
        child: child,
      );
    }
    final card = Card.filled(
      margin: EdgeInsets.zero,
      clipBehavior: clipBehavior,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(12),
        side: BorderSide(color: glowColor.withValues(alpha: 0.6), width: 1.5),
      ),
      child: child,
    );
    return Container(
      margin: margin,
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(12),
        boxShadow: [
          BoxShadow(color: glowColor.withValues(alpha: 0.5), blurRadius: 6),
        ],
      ),
      child: card,
    );
  }
}

extension CaseColorExt on CaseColor {
  Color toColor() => switch (this) {
    CaseColor.black => const Color(0xFF2C2C2C),
    CaseColor.orange => const Color(0xFFE8731A),
    CaseColor.silver => const Color(0xFFB0B0B8),
    CaseColor.blue => const Color(0xFF2E6FD4),
    CaseColor.red => const Color(0xFFCC2936),
  };
}
