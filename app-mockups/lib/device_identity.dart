import 'package:device_info_plus/device_info_plus.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';

class FrostsnapIcons {
  FrostsnapIcons._();
  static const IconData device = IconData(0xe801, fontFamily: 'DeviceIcon');
}

class DeviceIdentity {
  static String? _cachedName;
  static bool _initialized = false;

  static Future<void> init() async {
    if (_initialized) return;
    _initialized = true;

    final plugin = DeviceInfoPlugin();
    final platform = defaultTargetPlatform;

    switch (platform) {
      case TargetPlatform.android:
        final info = await plugin.androidInfo;
        _cachedName = info.model;
      case TargetPlatform.iOS:
        final info = await plugin.iosInfo;
        // iOS .name gives the user-set name like "Fred's iPhone"
        _cachedName = info.name;
      case TargetPlatform.macOS:
        final info = await plugin.macOsInfo;
        _cachedName = info.computerName;
      case TargetPlatform.windows:
        final info = await plugin.windowsInfo;
        _cachedName = info.computerName;
      case TargetPlatform.linux:
        final info = await plugin.linuxInfo;
        _cachedName = info.prettyName;
      case TargetPlatform.fuchsia:
        _cachedName = null;
    }
  }

  static String get name => _cachedName ?? _fallbackName;

  static String get _fallbackName {
    switch (defaultTargetPlatform) {
      case TargetPlatform.iOS:
      case TargetPlatform.android:
        return 'This phone';
      case TargetPlatform.macOS:
      case TargetPlatform.windows:
      case TargetPlatform.linux:
      case TargetPlatform.fuchsia:
        return 'This computer';
    }
  }

  /// Lowercase noun for the kind of device the user is on —
  /// "phone" or "computer". For use mid-sentence ("Use this phone...").
  static String get kind {
    switch (defaultTargetPlatform) {
      case TargetPlatform.iOS:
      case TargetPlatform.android:
        return 'phone';
      case TargetPlatform.macOS:
      case TargetPlatform.windows:
      case TargetPlatform.linux:
      case TargetPlatform.fuchsia:
        return 'computer';
    }
  }

  static IconData get icon {
    switch (defaultTargetPlatform) {
      case TargetPlatform.iOS:
      case TargetPlatform.android:
        return Icons.smartphone;
      case TargetPlatform.macOS:
      case TargetPlatform.windows:
      case TargetPlatform.linux:
      case TargetPlatform.fuchsia:
        return Icons.laptop;
    }
  }
}
