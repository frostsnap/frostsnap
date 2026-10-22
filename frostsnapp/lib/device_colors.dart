import 'package:flutter/material.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';

/// Case colour from the certificate on file, so it works while the device is disconnected. `null`
/// when no certificate is on file or its colour has no name in this build.
Color? caseAccentColor(DeviceId id) =>
    coord.getDeviceCaseColor(id: id)?.swatch?.color;

/// `color` is the accent for icons and swatches; `body`, `outline` and `logo` paint the case's
/// silhouette.
typedef CaseSwatch = ({
  Color color,
  String label,
  Color body,
  Color outline,
  Color logo,
});

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
    CaseColor_Black() => (
      color: const Color(0xFF3A3A42),
      label: 'Black',
      body: const Color(0xFF293740),
      outline: const Color(0xFF1E2B33),
      logo: const Color(0xFFECEFEB),
    ),
    CaseColor_Orange() => (
      color: const Color(0xFFE8731A),
      label: 'Orange',
      body: const Color(0xFFC87932),
      outline: const Color(0xFF965722),
      logo: const Color(0xFFECEFEB),
    ),
    CaseColor_Silver() => (
      color: const Color(0xFFB0B0B8),
      label: 'Silver',
      body: const Color(0xFFC9CED2),
      outline: const Color(0xFF626F79),
      logo: const Color(0xFF8D989F),
    ),
    CaseColor_Blue() => (
      color: const Color(0xFF2E6FD4),
      label: 'Blue',
      body: const Color(0xFF5197B5),
      outline: const Color(0xFF376C84),
      logo: const Color(0xFFECEFEB),
    ),
    CaseColor_Red() => (
      color: const Color(0xFFCC2936),
      label: 'Red',
      body: const Color(0xFFC7353C),
      outline: const Color(0xFF98292E),
      logo: const Color(0xFFECEFEB),
    ),
    CaseColor_Other() => null,
  };
}
