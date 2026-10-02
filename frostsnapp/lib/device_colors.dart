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

/// A card outlined and softly glowing in the device's case colour, so a device on
/// screen can be matched to the one in your hand.
///
/// Falls back to a plain filled card when the colour isn't known — which is the
/// normal state for a device we have never verified, and for one whose firmware
/// claims a colour this build has no name for.
class DeviceGlowCard extends StatelessWidget {
  const DeviceGlowCard({
    super.key,
    required this.child,
    this.caseColor,
    this.margin = EdgeInsets.zero,
    this.clipBehavior = Clip.hardEdge,
  });

  final Widget child;
  final CaseColor? caseColor;
  final EdgeInsets margin;
  final Clip clipBehavior;

  static final BorderRadius _radius = BorderRadius.circular(12);

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final glow = caseColor?.swatch?.color;

    if (glow == null) {
      return Card.filled(
        margin: margin,
        color: theme.colorScheme.surfaceContainerHigh,
        clipBehavior: clipBehavior,
        child: child,
      );
    }

    return Container(
      margin: margin,
      decoration: BoxDecoration(
        borderRadius: _radius,
        boxShadow: [
          BoxShadow(color: glow.withValues(alpha: 0.5), blurRadius: 6),
        ],
      ),
      child: Card.filled(
        margin: EdgeInsets.zero,
        color: theme.colorScheme.surfaceContainerHigh,
        clipBehavior: clipBehavior,
        shape: RoundedRectangleBorder(
          borderRadius: _radius,
          side: BorderSide(color: glow.withValues(alpha: 0.6), width: 1.5),
        ),
        child: child,
      ),
    );
  }
}
