import 'package:flutter/material.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';

/// Colors and styling for a device based on its case color
class DeviceColorScheme {
  final Color deviceColor;
  final CaseColor? caseColor;

  DeviceColorScheme({required this.deviceColor, this.caseColor});

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
    );
  }

  /// Device color for icon tinting; null if device has no color
  Color? get accent => caseColor != null ? deviceColor : null;

  /// Card with colored border and glow effect
  Widget buildGlowCard({
    required Widget child,
    EdgeInsets margin = EdgeInsets.zero,
    Clip clipBehavior = Clip.hardEdge,
  }) {
    if (caseColor == null) {
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
        side: BorderSide(
          color: deviceColor.withValues(alpha: 0.6),
          width: 1.5,
        ),
      ),
      child: child,
    );
    return Container(
      margin: margin,
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(12),
        boxShadow: [
          BoxShadow(
            color: deviceColor.withValues(alpha: 0.5),
            blurRadius: 6,
          ),
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
