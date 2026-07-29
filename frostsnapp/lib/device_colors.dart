import 'package:flutter/material.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';

/// Colors and styling for a device based on its case color
class DeviceColorScheme {
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

    // Try connected device color, then fall back to persisted color
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

  /// True when the device responded to the genuine check but did not verify —
  /// counterfeit hardware, a relay/MITM attempt, or an unknown factory key.
  bool get genuineFailed => genuine == GenuineStatus.failed;

  /// Device color for icon tinting; null if device has no color
  Color? get accent => caseColor != null ? deviceColor : null;

  /// A warning banner to show for a device that failed the genuine check, or
  /// null if the device is fine. Callers place this near the device's title.
  Widget? genuineWarning(BuildContext context) {
    if (!genuineFailed) return null;
    final scheme = Theme.of(context).colorScheme;
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Icon(Icons.gpp_bad_rounded, size: 16, color: scheme.error),
        const SizedBox(width: 4),
        Flexible(
          child: Text(
            'Could not verify authenticity',
            style: TextStyle(color: scheme.error, fontSize: 12),
          ),
        ),
      ],
    );
  }

  /// Card with colored border and glow effect. A device that failed the genuine
  /// check is bordered/glowed in the theme error color instead of its case color.
  Widget buildGlowCard({
    required Widget child,
    EdgeInsets margin = EdgeInsets.zero,
    Clip clipBehavior = Clip.hardEdge,
    Color? errorColor,
  }) {
    final glowColor = genuineFailed
        ? errorColor
        : (caseColor != null ? deviceColor : null);
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
