import 'package:flutter/material.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';

/// Case colour for a device we may only hold an id for, taken from the persisted
/// record so it works while the device is disconnected and costs no round-trip to
/// the device itself.
///
/// `null` when we've never verified it — a device only tells us its colour as part
/// of a certificate — or when it claims a colour this build has no name for. Prefer
/// [ConnectedDevice.caseColor] when you have the device in hand; it is live.
Color? caseAccentColor(DeviceId id) => coord.getDeviceCaseColor(id: id)?.color;

/// The physical colours a Frostsnap case comes in.
///
/// Cosmetic identity only — this is how someone tells their own devices apart on a
/// desk. It is read from a device's certificate *before* and *regardless of*
/// verification, so a device effectively chooses the colour it claims.
///
/// Nothing here may be used to signal trust. That lives in `genuine_badge.dart`,
/// which deliberately imports nothing from this file: a device must not be able to
/// influence how its own authenticity is rendered.
extension CaseColorExt on CaseColor {
  /// Tuned for the app's dark theme. Black is rendered as a very dark grey rather
  /// than true black, which would be indistinguishable from "no colour known"
  /// against a dark surface — the one thing the colour exists to tell you.
  ///
  /// Exhaustive with no `default` on purpose: adding a case colour should be a
  /// compile error here, not a silently wrong render.
  Color get color => switch (this) {
    CaseColor.black => const Color(0xFF3A3A42),
    CaseColor.orange => const Color(0xFFE8731A),
    CaseColor.silver => const Color(0xFFB0B0B8),
    CaseColor.blue => const Color(0xFF2E6FD4),
    CaseColor.red => const Color(0xFFCC2936),
  };

  String get label => switch (this) {
    CaseColor.black => 'Black',
    CaseColor.orange => 'Orange',
    CaseColor.silver => 'Silver',
    CaseColor.blue => 'Blue',
    CaseColor.red => 'Red',
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
    final glow = caseColor?.color;

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
