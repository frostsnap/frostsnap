import 'package:dotted_border/dotted_border.dart';
import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';
import 'package:frostsnap/device_colors.dart';
import 'package:frostsnap/genuine_badge.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/theme.dart';

const _tileSize = 56.0;
const _badgeOverhang = 6.0;
const _tileRadius = Radius.circular(12);
const _unknownSilhouette = 'assets/devices/frostsnap-device-unknown.svg';

String? _silhouetteAsset(CaseColor? caseColor) => switch (caseColor) {
  CaseColor_Black() => 'assets/devices/frostsnap-device-black.svg',
  CaseColor_Orange() => 'assets/devices/frostsnap-device-orange.svg',
  CaseColor_Silver() => 'assets/devices/frostsnap-device-silver.svg',
  CaseColor_Blue() => 'assets/devices/frostsnap-device-blue.svg',
  CaseColor_Red() => 'assets/devices/frostsnap-device-red.svg',
  CaseColor_Other() || null => null,
};

/// A device drawn as its silhouette in its case colour, on a neutral tile, with its genuine-check
/// result as a badge on the corner. Tapping it explains that result.
///
/// The colour is only what the factory certificate on file claims; the badge alone says whether the
/// device in hand proved it is genuine.
class DeviceIdentityArt extends StatelessWidget {
  const DeviceIdentityArt({
    super.key,
    required this.caseColor,
    required this.genuine,
    this.focusable = true,
  });

  DeviceIdentityArt.forDevice(
    ConnectedDevice device, {
    super.key,
    this.focusable = true,
  }) : caseColor = device.caseColor,
       genuine = checkedGenuineStatus(device);

  /// `null`, or a colour this build has no name for, draws the unknown-colour outline.
  final CaseColor? caseColor;

  /// `null` draws no badge and leaves nothing to tap.
  final GenuineStatus? genuine;

  /// Whether keyboard focus stops on the art. Tapping and screen readers can reach it either way.
  final bool focusable;

  static const size = Size.square(_tileSize + _badgeOverhang);

  @override
  Widget build(BuildContext context) {
    final swatch = caseColor?.swatch;
    final colourLabel = swatch == null
        ? 'Case colour unknown'
        : '${swatch.label} case';
    final tile = Align(
      alignment: Alignment.bottomLeft,
      child: _Tile(asset: _silhouetteAsset(caseColor)),
    );

    final status = genuine;
    if (status == null) {
      return Tooltip(
        message: colourLabel,
        excludeFromSemantics: true,
        child: Semantics(
          container: true,
          image: true,
          label: colourLabel,
          child: SizedBox.fromSize(size: size, child: tile),
        ),
      );
    }

    final look = status.look;
    return Tooltip(
      message: '$colourLabel · ${look.label}',
      excludeFromSemantics: true,
      child: Semantics(
        container: true,
        button: true,
        label: '$colourLabel, ${look.label}. Show genuine-check details',
        child: InkWell(
          onTap: () => showGenuineExplanation(context, status),
          canRequestFocus: focusable,
          customBorder: const RoundedRectangleBorder(
            borderRadius: BorderRadius.all(_tileRadius),
          ),
          child: SizedBox.fromSize(
            size: size,
            child: Stack(
              children: [
                tile,
                Positioned(top: 0, right: 0, child: _Badge(look: look)),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _Tile extends StatelessWidget {
  const _Tile({required this.asset});

  /// `null` when the case colour is unknown.
  final String? asset;

  @override
  Widget build(BuildContext context) {
    final cs = Theme.of(context).colorScheme;
    final silhouette = SvgPicture.asset(
      asset ?? _unknownSilhouette,
      width: 36,
      height: 43,
      excludeFromSemantics: true,
    );

    if (asset == null) {
      return DottedBorder(
        borderType: BorderType.RRect,
        radius: _tileRadius,
        color: cs.outline,
        strokeWidth: 1,
        dashPattern: const [4, 3],
        padding: EdgeInsets.zero,
        child: SizedBox.square(
          dimension: _tileSize,
          child: Center(child: silhouette),
        ),
      );
    }

    return Container(
      width: _tileSize,
      height: _tileSize,
      alignment: Alignment.center,
      decoration: BoxDecoration(
        borderRadius: const BorderRadius.all(_tileRadius),
        border: Border.all(color: cs.onSurface.withValues(alpha: 0.08)),
        gradient: RadialGradient(
          center: const Alignment(0, -0.3),
          radius: 0.8,
          colors: [cs.surfaceBright, cs.surfaceContainerHighest],
        ),
      ),
      child: silhouette,
    );
  }
}

class _Badge extends StatelessWidget {
  const _Badge({required this.look});

  final GenuineLook look;

  @override
  Widget build(BuildContext context) => Icon(
    look.icon,
    size: 20,
    color: look.color(Theme.of(context).colorScheme),
    // A disc behind the glyph read as a second, competing status; a faint shadow alone keeps it
    // legible on both the tile and the card.
    shadows: const [
      Shadow(color: Color(0x73000000), blurRadius: 2),
      Shadow(color: Color(0x40000000), blurRadius: 3),
    ],
  );
}

/// A neutral card for one device: its art, then [main] in the remaining width, then [trailing].
///
/// A [notice] sits beside [main] when the card is wide enough, otherwise beneath it across the
/// whole card.
class DeviceArtCard extends StatelessWidget {
  const DeviceArtCard({
    super.key,
    required this.art,
    required this.main,
    this.trailing,
    this.notice,
    this.onTap,
    this.focusable = true,
  });

  final Widget art;
  final Widget main;
  final Widget? trailing;
  final Widget? notice;
  final VoidCallback? onTap;

  /// Whether keyboard focus stops on the card itself. Tapping it works either way.
  final bool focusable;

  static const sideNoticeMinWidth = 480.0;

  @override
  Widget build(BuildContext context) {
    final cs = Theme.of(context).colorScheme;
    final trailing = this.trailing;
    final notice = this.notice;
    return Card.filled(
      margin: const EdgeInsets.symmetric(vertical: 5),
      color: cs.surfaceContainerHigh,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.all(Radius.circular(14)),
      ),
      clipBehavior: Clip.antiAlias,
      child: InkWell(
        onTap: onTap,
        canRequestFocus: focusable,
        child: Padding(
          padding: EdgeInsets.fromLTRB(14, 12, trailing == null ? 14 : 4, 12),
          child: LayoutBuilder(
            builder: (context, constraints) {
              final beside =
                  notice != null && constraints.maxWidth >= sideNoticeMinWidth;
              final row = Row(
                children: [
                  art,
                  const SizedBox(width: 14),
                  Expanded(child: main),
                  if (beside) ...[
                    const SizedBox(width: 12),
                    ConstrainedBox(
                      constraints: const BoxConstraints(maxWidth: 220),
                      child: notice,
                    ),
                  ],
                  ?trailing,
                ],
              );
              if (notice == null || beside) return row;
              return Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  row,
                  const SizedBox(height: 10),
                  Divider(height: 1, color: cs.outlineVariant),
                  const SizedBox(height: 10),
                  notice,
                ],
              );
            },
          ),
        ),
      ),
    );
  }
}

/// Something about a device the user has to act on, as an icon, a short [title] and a [detail].
class DeviceNotice extends StatelessWidget {
  const DeviceNotice({
    super.key,
    required this.icon,
    required this.title,
    required this.detail,
    required this.color,
  });

  final IconData icon;
  final String title;
  final String detail;
  final Color color;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Icon(icon, color: color, size: 20),
        const SizedBox(width: 10),
        Flexible(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(
                title,
                style: theme.textTheme.titleSmall?.copyWith(color: color),
              ),
              Text(
                detail,
                style: theme.textTheme.bodySmall?.copyWith(
                  color: theme.colorScheme.onSurfaceVariant,
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

/// A device's name as every device row shows it, so it reads the same while being edited, when
/// read-only, and in the device list.
class DeviceNameText extends StatelessWidget {
  const DeviceNameText(this.name, {super.key});

  final String name;

  static TextStyle? styleOf(BuildContext context) =>
      Theme.of(context).textTheme.bodyLarge?.merge(monospaceTextStyle);

  @override
  Widget build(BuildContext context) =>
      Text(name, overflow: TextOverflow.ellipsis, style: styleOf(context));
}

/// Stands in for a device's name when there is none to show.
class DevicePlaceholderText extends StatelessWidget {
  const DevicePlaceholderText(this.text, {super.key});

  final String text;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Text(
      text,
      overflow: TextOverflow.ellipsis,
      style: theme.textTheme.bodyMedium?.copyWith(
        color: theme.colorScheme.onSurfaceVariant,
        fontStyle: FontStyle.italic,
      ),
    );
  }
}
