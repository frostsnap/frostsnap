import 'package:flutter/material.dart';
import 'package:frostsnap/theme.dart';

import 'maybe_fullscreen_dialog.dart';

/// Padding applied to the subtitle block when present.
const _subtitlePadding = EdgeInsets.fromLTRB(16, 0, 16, 16);

/// Padding applied to the body sliver.
const _bodyPadding = EdgeInsets.fromLTRB(16, 16, 16, 24);

/// Scrollable body for a full-screen dialog: a [TopBarSliver] header,
/// an optional subtitle, and a caller-provided body sliver, hosted
/// inside a [CustomScrollView].
///
/// This is designed as the box-level child of an [AnimatedSwitcher]
/// for multi-step flows — unlike a `Column`-rooted scaffold, a
/// `CustomScrollView` can live under the default [Stack] layout that
/// `AnimatedSwitcher` uses without triggering Flex-inside-Stack
/// errors.
///
/// Host inside [MaybeFullscreenDialog.show] — do not wrap in a
/// standalone `Dialog`.
class FullscreenDialogBody extends StatelessWidget {
  const FullscreenDialogBody({
    super.key,
    required this.title,
    this.subtitle,
    this.leading,
    this.showClose = true,
    required this.body,
  });

  final Widget title;
  final String? subtitle;
  final Widget? leading;
  final bool showClose;

  /// A sliver (`MultiSliver`, `SliverList`, `SliverToBoxAdapter`, ...).
  final Widget body;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final isFullscreen =
        WindowSizeContext.of(context) == WindowSizeClass.compact;
    final subtitleText = subtitle;

    return CustomScrollView(
      physics: const ClampingScrollPhysics(),
      shrinkWrap: !isFullscreen,
      slivers: [
        TopBarSliver(title: title, leading: leading, showClose: showClose),
        if (subtitleText != null && subtitleText.isNotEmpty)
          SliverToBoxAdapter(
            child: Padding(
              padding: _subtitlePadding.copyWith(top: isFullscreen ? null : 8),
              child: Text(subtitleText, style: theme.textTheme.titleMedium),
            ),
          ),
        SliverPadding(padding: _bodyPadding, sliver: body),
        const SliverPadding(padding: EdgeInsets.only(bottom: 16)),
      ],
    );
  }
}

/// Pinned footer for a full-screen dialog: a standard 16-px outer
/// padding, a SafeArea (bottom-only), and a bottom inset that grows
/// with the software keyboard. The [child] is typically an
/// `Align(alignment: Alignment.centerRight, child: FilledButton(...))`
/// or a `Row` of buttons.
class FullscreenDialogFooter extends StatelessWidget {
  const FullscreenDialogFooter({super.key, required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    final mediaQuery = MediaQuery.of(context);
    return Padding(
      padding: const EdgeInsets.all(
        16,
      ).add(EdgeInsets.only(bottom: mediaQuery.viewInsets.bottom)),
      child: SafeArea(top: false, child: child),
    );
  }
}

/// One-shot composer combining [FullscreenDialogBody] with an optional
/// [FullscreenDialogFooter]. Good for single-step dialogs.
///
/// For multi-step flows, compose [FullscreenDialogBody] and
/// [FullscreenDialogFooter] by hand so the body can sit inside an
/// [AnimatedSwitcher] while the footer stays pinned (or vice versa).
class FullscreenDialogScaffold extends StatelessWidget {
  const FullscreenDialogScaffold({
    super.key,
    required this.title,
    this.subtitle,
    this.leading,
    this.showClose = true,
    required this.body,
    this.footer,
  });

  final Widget title;
  final String? subtitle;
  final Widget? leading;
  final bool showClose;
  final Widget body;
  final Widget? footer;

  @override
  Widget build(BuildContext context) {
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Flexible(
          child: FullscreenDialogBody(
            title: title,
            subtitle: subtitle,
            leading: leading,
            showClose: showClose,
            body: body,
          ),
        ),
        if (footer != null) FullscreenDialogFooter(child: footer!),
      ],
    );
  }
}
