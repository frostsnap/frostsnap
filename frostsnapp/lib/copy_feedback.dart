import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/semantics.dart';
import 'package:flutter/services.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/theme.dart';

const _chipTotalDuration = Duration(milliseconds: 990);
const _chipRevertDelay = Duration(milliseconds: 1200);
const _chipInFraction = 0.22;
const _chipOutFraction = 0.22;

Future<bool> _writeClipboard(String data) async {
  HapticFeedback.selectionClick();
  try {
    await Clipboard.setData(ClipboardData(text: data));
    return true;
  } catch (_) {
    return false;
  }
}

/// Copies [data] without the floating chip. Use when surrounding UI already
/// provides feedback (e.g. a snackbar's Copy action button or an inline
/// label flip). Still announces "Copied" to screen readers by default, since
/// the sighted label flip isn't read unless focus happens to be on the
/// changed widget.
Future<bool> copyToClipboardQuietly(String data, {bool announce = true}) async {
  final ok = await _writeClipboard(data);
  if (ok && announce) _announceCopied();
  return ok;
}

void _announceCopied() {
  final overlay = rootNavKey.currentState?.overlay;
  if (overlay == null) return;
  SemanticsService.sendAnnouncement(
    View.of(overlay.context),
    'Copied',
    Directionality.of(overlay.context),
  );
}

/// Writes [data] to the clipboard, fires a haptic, announces to screen
/// readers, and shows a transient "COPIED" chip anchored to the tap
/// location. Returns `true` if the clipboard write succeeded.
///
/// Safe to invoke even if [context] is disposed during the async gap:
/// the chip is inserted into the root overlay directly, not via [context].
Future<bool> copyToClipboard(
  String data, {
  Offset? tapPosition,
  GlobalKey? anchorKey,
}) async {
  final anchorRect = _resolveAnchor(tapPosition, anchorKey);
  final ok = await _writeClipboard(data);
  if (!ok) return false;
  _showCopiedChip(anchorRect);
  return true;
}

Rect? _resolveAnchor(Offset? tapPosition, GlobalKey? anchorKey) {
  if (tapPosition != null) {
    return Rect.fromCenter(center: tapPosition, width: 1, height: 1);
  }
  final ctx = anchorKey?.currentContext;
  if (ctx == null) return null;
  final box = ctx.findRenderObject();
  if (box is! RenderBox || !box.attached || !box.hasSize) return null;
  return box.localToGlobal(Offset.zero) & box.size;
}

void _showCopiedChip(Rect? anchorRect) {
  final overlay = rootNavKey.currentState?.overlay;
  if (overlay == null) return;
  _announceCopied();
  late OverlayEntry entry;
  entry = OverlayEntry(builder: (_) => _CopiedChip(anchorRect: anchorRect));
  overlay.insert(entry);
  Timer(_chipTotalDuration, () {
    if (entry.mounted) entry.remove();
  });
}

/// Copy icon that swaps instantly to a check when [checked] flips to `true`.
/// The parent owns the notifier's lifetime.
class CopyIcon extends StatelessWidget {
  const CopyIcon({
    super.key,
    required this.checked,
    this.size = iconSize,
    this.color,
    this.icon = Icons.copy_rounded,
  });

  final ValueListenable<bool> checked;
  final double size;
  final Color? color;
  final IconData icon;

  @override
  Widget build(BuildContext context) {
    return ValueListenableBuilder<bool>(
      valueListenable: checked,
      builder: (context, showCheck, _) => showCheck
          ? Icon(
              Icons.check_rounded,
              size: size,
              color: Theme.of(context).colorScheme.primary,
            )
          : Icon(icon, size: size, color: color),
    );
  }
}

class CopyIconButton extends StatefulWidget {
  const CopyIconButton({
    super.key,
    required this.data,
    this.tooltip = 'Copy',
    this.size = iconSize,
    this.color,
    this.icon = Icons.copy_rounded,
    this.onCopy,
  });

  final String? data;
  final String? tooltip;
  final double size;
  final Color? color;
  final IconData icon;
  final VoidCallback? onCopy;

  @override
  State<CopyIconButton> createState() => _CopyIconButtonState();
}

class _CopyIconButtonState extends State<CopyIconButton> {
  final _checked = ValueNotifier<bool>(false);
  final _anchorKey = GlobalKey();
  Timer? _revertTimer;
  Offset? _lastTap;

  @override
  void dispose() {
    _revertTimer?.cancel();
    _checked.dispose();
    super.dispose();
  }

  Future<void> _onPressed() async {
    final data = widget.data;
    if (data == null) return;
    final tapPos = _lastTap;
    _lastTap = null;
    final ok = await copyToClipboard(
      data,
      tapPosition: tapPos,
      anchorKey: _anchorKey,
    );
    if (!ok || !mounted) return;
    _checked.value = true;
    _revertTimer?.cancel();
    _revertTimer = Timer(_chipRevertDelay, () {
      if (mounted) _checked.value = false;
    });
    widget.onCopy?.call();
  }

  @override
  Widget build(BuildContext context) {
    return Listener(
      onPointerDown: (e) => _lastTap = e.position,
      child: IconButton(
        key: _anchorKey,
        tooltip: widget.tooltip,
        onPressed: widget.data == null ? null : _onPressed,
        icon: CopyIcon(
          checked: _checked,
          size: widget.size,
          color: widget.color,
          icon: widget.icon,
        ),
      ),
    );
  }
}

/// Wraps any tappable widget and plumbs the whole copy flow to the builder.
/// The builder receives:
///   * `onCopy` — invoke to perform the copy. `null` iff [data] is `null`,
///     so `onTap: onCopy` on a ListTile gracefully disables the row.
///   * `checked` — drives a [CopyIcon] inside the child, if any.
class CopyTapTarget extends StatefulWidget {
  const CopyTapTarget({super.key, required this.data, required this.builder});

  final String? data;
  final Widget Function(
    BuildContext context,
    VoidCallback? onCopy,
    ValueListenable<bool> checked,
  )
  builder;

  @override
  State<CopyTapTarget> createState() => _CopyTapTargetState();
}

class _CopyTapTargetState extends State<CopyTapTarget> {
  final _checked = ValueNotifier<bool>(false);
  Timer? _revertTimer;
  Offset? _lastTap;

  @override
  void dispose() {
    _revertTimer?.cancel();
    _checked.dispose();
    super.dispose();
  }

  Future<void> _onCopy() async {
    final data = widget.data;
    if (data == null) return;
    final tapPos = _lastTap;
    _lastTap = null;
    final ok = await copyToClipboard(data, tapPosition: tapPos);
    if (!ok || !mounted) return;
    _checked.value = true;
    _revertTimer?.cancel();
    _revertTimer = Timer(_chipRevertDelay, () {
      if (mounted) _checked.value = false;
    });
  }

  @override
  Widget build(BuildContext context) {
    final onCopy = widget.data == null ? null : _onCopy;
    return Listener(
      onPointerDown: (e) => _lastTap = e.position,
      child: widget.builder(context, onCopy, _checked),
    );
  }
}

/// `ListTile` that copies [data] to the clipboard when tapped.
///
/// Null [data] disables the row. If [showCopyIcon] is true and [trailing] is
/// not supplied, a morphing [CopyIcon] is placed in the trailing slot — omit
/// it for rows that don't advertise a visible copy affordance.
class CopyListTile extends StatelessWidget {
  const CopyListTile({
    super.key,
    required this.data,
    this.leading,
    this.title,
    this.subtitle,
    this.trailing,
    this.contentPadding,
    this.dense,
    this.showCopyIcon = false,
  });

  final String? data;
  final Widget? leading;
  final Widget? title;
  final Widget? subtitle;
  final Widget? trailing;
  final EdgeInsetsGeometry? contentPadding;
  final bool? dense;
  final bool showCopyIcon;

  @override
  Widget build(BuildContext context) {
    return CopyTapTarget(
      data: data,
      builder: (_, onCopy, checked) {
        final effectiveTrailing =
            trailing ??
            (showCopyIcon && onCopy != null
                ? CopyIcon(checked: checked)
                : null);
        return ListTile(
          leading: leading,
          title: title,
          subtitle: subtitle,
          trailing: effectiveTrailing,
          contentPadding: contentPadding,
          dense: dense,
          onTap: onCopy,
        );
      },
    );
  }
}

class _CopiedChip extends StatefulWidget {
  const _CopiedChip({required this.anchorRect});
  final Rect? anchorRect;

  @override
  State<_CopiedChip> createState() => _CopiedChipState();
}

class _CopiedChipState extends State<_CopiedChip>
    with SingleTickerProviderStateMixin {
  late final AnimationController _ac = AnimationController(
    vsync: this,
    duration: _chipTotalDuration,
  )..forward();

  @override
  void dispose() {
    _ac.dispose();
    super.dispose();
  }

  ({double opacity, double translateY, double scale}) _stageValues(double t) {
    const holdEnd = 1.0 - _chipOutFraction;
    if (t < _chipInFraction) {
      final p = Curves.easeOutCubic.transform(t / _chipInFraction);
      return (opacity: p, translateY: 8 * (1 - p), scale: 0.85 + 0.15 * p);
    } else if (t < holdEnd) {
      final p = (t - _chipInFraction) / (holdEnd - _chipInFraction);
      return (opacity: 1.0, translateY: -10 * p, scale: 1.0);
    } else {
      final p = Curves.easeInCubic.transform((t - holdEnd) / _chipOutFraction);
      return (opacity: 1.0 - p, translateY: -10, scale: 1.0);
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final mq = MediaQuery.of(context);
    final fg = theme.colorScheme.onInverseSurface;
    final screen = mq.size;

    final chip = ExcludeSemantics(
      child: Material(
        color: theme.colorScheme.inverseSurface,
        elevation: 6,
        borderRadius: BorderRadius.circular(999),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(Icons.check_rounded, size: 18, color: fg),
              const SizedBox(width: 8),
              Text(
                'COPIED',
                style: TextStyle(
                  color: fg,
                  fontSize: 12,
                  fontWeight: FontWeight.w700,
                  letterSpacing: 1.6,
                ),
              ),
            ],
          ),
        ),
      ),
    );

    return AnimatedBuilder(
      animation: _ac,
      builder: (context, _) {
        final v = _stageValues(_ac.value);
        return CustomSingleChildLayout(
          delegate: _ChipLayoutDelegate(
            anchorRect: widget.anchorRect,
            safeTop: mq.viewPadding.top + 4,
            safeLeft: mq.viewPadding.left + 12,
            safeRight: screen.width - mq.viewPadding.right - 12,
            safeBottom: screen.height - mq.viewPadding.bottom - 80,
            gap: 10,
            fadeOffsetY: v.translateY,
          ),
          child: Opacity(
            opacity: v.opacity,
            child: Transform.scale(
              scale: v.scale,
              child: IgnorePointer(child: chip),
            ),
          ),
        );
      },
    );
  }
}

class _ChipLayoutDelegate extends SingleChildLayoutDelegate {
  _ChipLayoutDelegate({
    required this.anchorRect,
    required this.safeTop,
    required this.safeLeft,
    required this.safeRight,
    required this.safeBottom,
    required this.gap,
    required this.fadeOffsetY,
  });

  final Rect? anchorRect;
  final double safeTop;
  final double safeLeft;
  final double safeRight;
  final double safeBottom;
  final double gap;
  final double fadeOffsetY;

  @override
  BoxConstraints getConstraintsForChild(BoxConstraints constraints) {
    return BoxConstraints(maxWidth: (safeRight - safeLeft).clamp(0, 360));
  }

  @override
  Offset getPositionForChild(Size size, Size childSize) {
    final anchor = anchorRect;
    double top;
    double left;

    if (anchor != null) {
      final above = anchor.top - childSize.height - gap;
      final below = anchor.bottom + gap;
      top = above > safeTop ? above : below;
      left = anchor.center.dx - childSize.width / 2;
    } else {
      top = safeBottom - childSize.height;
      left = (size.width - childSize.width) / 2;
    }

    final maxLeft = math.max(safeLeft, safeRight - childSize.width);
    final maxTop = math.max(safeTop, safeBottom - childSize.height);
    left = left.clamp(safeLeft, maxLeft);
    top = top.clamp(safeTop, maxTop);

    return Offset(left, top + fadeOffsetY);
  }

  @override
  bool shouldRelayout(_ChipLayoutDelegate old) =>
      old.anchorRect != anchorRect || old.fadeOffsetY != fadeOffsetY;
}
