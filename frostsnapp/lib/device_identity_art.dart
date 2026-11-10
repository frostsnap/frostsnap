import 'package:dotted_border/dotted_border.dart';
import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';
import 'package:frostsnap/device_colors.dart';
import 'package:frostsnap/genuine_badge.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/theme.dart';

const _tileSize = 56.0;
const _badgeOverhang = 6.0;
const _tileRadius = Radius.circular(12);
const _silhouette = 'assets/devices/frostsnap-device.svg';
const _unknownSilhouette = 'assets/devices/frostsnap-device-unknown.svg';

// The silhouettes' viewBox, and the USB slot's span across it.
const _silhouetteWidth = 80.0;
const _silhouetteHeight = 96.0;
const _slotLeft = 32.0;
const _slotRight = 48.0;

/// Paints [_silhouette], which is drawn in these colours, in a case's colours.
class _CaseColorMapper extends ColorMapper {
  const _CaseColorMapper(this.swatch);

  static const _drawnBody = Color(0xFFC87932);
  static const _drawnOutline = Color(0xFF965722);
  static const _drawnLogo = Color(0xFFECEFEB);

  final CaseSwatch swatch;

  @override
  Color substitute(
    String? id,
    String elementName,
    String attributeName,
    Color color,
  ) {
    if (color == _drawnBody) return swatch.body;
    if (color == _drawnOutline) return swatch.outline;
    if (color == _drawnLogo) return swatch.logo;
    return color;
  }

  // flutter_svg caches parsed pictures by their mapper, so equal swatches must share an entry.
  @override
  bool operator ==(Object other) =>
      other is _CaseColorMapper && other.swatch == swatch;

  @override
  int get hashCode => swatch.hashCode;
}

/// A device drawn as its silhouette in its case colour, on a neutral tile, with its genuine-check
/// result as a badge on the corner. Tapping it explains that result.
///
/// [DeviceIdentityArt.forDevice] shows a colour only for a device that proved it is genuine on this
/// connection.
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
      child: _Tile(swatch: swatch),
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
  const _Tile({required this.swatch, this.dimension = _tileSize});

  /// `null` when the case colour is unknown.
  final CaseSwatch? swatch;
  final double dimension;

  @override
  Widget build(BuildContext context) {
    final cs = Theme.of(context).colorScheme;
    final swatch = this.swatch;
    final scale = dimension / _tileSize;
    final radius = _tileRadius * scale;
    final silhouette = _Silhouette(
      swatch: swatch,
      width: 36 * scale,
      height: 43 * scale,
    );

    if (swatch == null) {
      return DottedBorder(
        borderType: BorderType.RRect,
        radius: radius,
        color: cs.outline,
        strokeWidth: 1,
        dashPattern: const [4, 3],
        padding: EdgeInsets.zero,
        child: SizedBox.square(
          dimension: dimension,
          child: Center(child: silhouette),
        ),
      );
    }

    return Container(
      width: dimension,
      height: dimension,
      alignment: Alignment.center,
      decoration: BoxDecoration(
        borderRadius: BorderRadius.all(radius),
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

class _Silhouette extends StatelessWidget {
  const _Silhouette({
    required this.swatch,
    required this.width,
    required this.height,
  });

  /// `null` when the case colour is unknown.
  final CaseSwatch? swatch;
  final double width;
  final double height;

  @override
  Widget build(BuildContext context) {
    final swatch = this.swatch;
    return SvgPicture.asset(
      swatch == null ? _unknownSilhouette : _silhouette,
      colorMapper: swatch == null ? null : _CaseColorMapper(swatch),
      width: width,
      height: height,
      excludeFromSemantics: true,
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

/// A device's [name] beside a small picture of its case, for anywhere the app names a device.
///
/// Identity only: unlike [DeviceIdentityArt] it never carries the genuine-check result, so it must
/// not grow a badge or anything that reads as one.
class DeviceNameWithArt extends StatelessWidget {
  const DeviceNameWithArt({
    super.key,
    required this.caseColor,
    required this.name,
  });

  /// The colour [CaseAccent] gives device [id].
  static Widget forId(DeviceId id, {Key? key, required Widget name}) =>
      CaseAccent(
        key: key,
        id: id,
        builder: (context, caseColor) =>
            DeviceNameWithArt(caseColor: caseColor, name: name),
      );

  /// `null`, or a colour this build has no name for, draws the unknown-colour outline.
  final CaseColor? caseColor;
  final Widget name;

  /// Icon-sized, since it takes the place of the generic key icon device rows used to show.
  static const artSize = 24.0;

  @override
  Widget build(BuildContext context) => Row(
    mainAxisSize: MainAxisSize.min,
    spacing: 10,
    children: [
      ExcludeSemantics(
        child: _Tile(swatch: caseColor?.swatch, dimension: artSize),
      ),
      Flexible(child: name),
    ],
  );
}

/// One device of a set, as a card for a [DeviceTileGrid]: its key index, its picture in the colour
/// [CaseAccent] gives it, and its name beneath.
class DeviceTile extends StatelessWidget {
  const DeviceTile({
    super.key,
    required this.id,
    required this.name,
    this.shareIndex,
    this.corner,
    this.onCornerTap,
    this.cornerTooltip,
    this.footer,
    this.color,
    this.selected = false,
    this.needsAction = false,
    this.onTap,
  });

  final DeviceId id;
  final String name;
  final int? shareIndex;

  /// The tile's state, an icon-sized glyph in its top-right corner.
  final Widget? corner;

  /// Makes [corner] a button of its own, with a full touch target named by [cornerTooltip].
  final VoidCallback? onCornerTap;
  final String? cornerTooltip;
  final Widget? footer;

  /// Defaults to a surface that stands off the page; a tile on a card passes a brighter one.
  final Color? color;
  final bool selected;

  /// Outlines the tile a little brighter, for a device the user still has to deal with.
  final bool needsAction;
  final VoidCallback? onTap;

  static const _cornerTarget = 48.0;
  static const _artHeight = 56.0;
  static const _artWidth = _artHeight * _silhouetteWidth / _silhouetteHeight;
  static const _padding = 10.0;
  static const _nameStyle = TextStyle(fontSize: 12, height: 16 / 12);

  /// How wide a band down the tile's centre the index must stay out of: the picture's USB slot,
  /// with its stroke and a little air.
  static const _slotBand =
      (_slotRight - _slotLeft) / _silhouetteWidth * _artWidth + 6;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final cs = theme.colorScheme;
    final nameStyle = monospaceTextStyle.merge(_nameStyle);
    final nameLines = MediaQuery.textScalerOf(context).scale(12) * 16 / 12;
    final shareIndex = this.shareIndex;
    final corner = this.corner;
    final footer = this.footer;

    final BorderSide side;
    if (selected) {
      side = BorderSide(color: cs.primary, width: 2);
    } else if (needsAction) {
      side = BorderSide(color: cs.outline);
    } else {
      side = BorderSide(color: cs.outlineVariant);
    }

    final body = Padding(
      padding: const EdgeInsets.fromLTRB(_padding, _padding, _padding, 12),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        spacing: 6,
        children: [
          const SizedBox(height: 12),
          CaseAccent(
            id: id,
            builder: (context, caseColor) => _Silhouette(
              swatch: caseColor?.swatch,
              width: _artWidth,
              height: _artHeight,
            ),
          ),
          ConstrainedBox(
            constraints: BoxConstraints(minHeight: nameLines * 2),
            child: Text(name, textAlign: TextAlign.center, style: nameStyle),
          ),
          ?footer,
        ],
      ),
    );

    // The index's row splits evenly either side of the slot band, so however the index scales it
    // can never reach the USB slot, which is drawn at the top centre of the picture.
    final indexRow = Row(
      children: [
        Expanded(
          child: Align(
            alignment: Alignment.topLeft,
            child: shareIndex == null
                ? null
                : FittedBox(
                    fit: BoxFit.scaleDown,
                    child: Text(
                      '#$shareIndex',
                      style: theme.textTheme.titleMedium?.copyWith(
                        fontSize: 18,
                        color: cs.onSurfaceVariant,
                        fontWeight: FontWeight.w500,
                      ),
                    ),
                  ),
          ),
        ),
        const SizedBox(width: _slotBand),
        const Spacer(),
      ],
    );

    final tile = Card.outlined(
      margin: EdgeInsets.zero,
      color: color ?? cs.surfaceContainer,
      clipBehavior: Clip.antiAlias,
      shape: RoundedRectangleBorder(
        borderRadius: const BorderRadius.all(Radius.circular(18)),
        side: side,
      ),
      child: InkWell(
        onTap: onTap,
        child: Stack(
          children: [
            SizedBox(width: double.infinity, child: body),
            Positioned(left: 11, right: 11, top: 4, child: indexRow),
            if (corner != null)
              Positioned(
                top: 0,
                right: 0,
                child: _TileCorner(
                  onTap: onCornerTap,
                  tooltip: cornerTooltip,
                  child: corner,
                ),
              ),
          ],
        ),
      ),
    );
    // A corner with its own button must stay a separate node, or it would merge into the tile.
    return onCornerTap == null ? MergeSemantics(child: tile) : tile;
  }
}

class _TileCorner extends StatelessWidget {
  const _TileCorner({required this.child, this.onTap, this.tooltip});

  final Widget child;
  final VoidCallback? onTap;
  final String? tooltip;

  @override
  Widget build(BuildContext context) {
    final glyph = Padding(padding: const EdgeInsets.all(6), child: child);
    final onTap = this.onTap;
    if (onTap == null) return glyph;
    return Semantics(
      container: true,
      button: true,
      child: Tooltip(
        message: tooltip ?? '',
        child: InkResponse(
          onTap: onTap,
          radius: DeviceTile._cornerTarget / 2,
          child: SizedBox.square(
            dimension: DeviceTile._cornerTarget,
            child: Align(alignment: Alignment.topRight, child: glyph),
          ),
        ),
      ),
    );
  }
}

/// [tiles] three to a row, each row as tall as its tallest tile.
class DeviceTileGrid extends StatelessWidget {
  const DeviceTileGrid({super.key, required this.tiles, this.spacing = 10});

  final List<Widget> tiles;
  final double spacing;

  static const columns = 3;

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    spacing: spacing,
    children: [
      for (var start = 0; start < tiles.length; start += columns)
        IntrinsicHeight(
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            spacing: spacing,
            children: [
              for (var i = start; i < start + columns; i++)
                Expanded(child: i < tiles.length ? tiles[i] : const SizedBox()),
            ],
          ),
        ),
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
