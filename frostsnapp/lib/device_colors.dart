import 'package:flutter/material.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';

/// Case colour from the certificate on file, so it works while the device is disconnected. `null`
/// when no certificate is on file or its colour has no name in this build.
Color? caseAccentColor(DeviceId id) =>
    coord.getDeviceCaseColor(id: id)?.swatch?.color;

typedef CaseSwatch = ({Color color, String label});

/// The physical colours a Frostsnap case comes in.
///
/// Cosmetic identity only. A certificate on file says nothing about whether the device in front of
/// the user is genuine, so nothing here may signal trust; that lives in `genuine_badge.dart`.
extension CaseColorExt on CaseColor {
  /// `null` for a colour this build has no name for: no colour beats the wrong one, since the user
  /// matches it against the device in their hand.
  ///
  /// Tuned for the app's dark theme. Black is rendered as a very dark grey rather
  /// than true black, which would be indistinguishable from "no colour known"
  /// against a dark surface — the one thing the colour exists to tell you.
  ///
  /// Exhaustive with no `default` on purpose: adding a case colour should be a
  /// compile error here, not a silently wrong render.
  CaseSwatch? get swatch => switch (this) {
    CaseColor_Black() => (color: const Color(0xFF3A3A42), label: 'Black'),
    CaseColor_Orange() => (color: const Color(0xFFE8731A), label: 'Orange'),
    CaseColor_Silver() => (color: const Color(0xFFB0B0B8), label: 'Silver'),
    CaseColor_Blue() => (color: const Color(0xFF2E6FD4), label: 'Blue'),
    CaseColor_Red() => (color: const Color(0xFFCC2936), label: 'Red'),
    CaseColor_Other() => null,
  };
}
