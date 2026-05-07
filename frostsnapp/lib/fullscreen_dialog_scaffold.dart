import 'package:flutter/material.dart';
import 'package:frostsnap/theme.dart';

import 'maybe_fullscreen_dialog.dart';

// Multi-step dialog convention:
// - step builders live on State and return MultiStepDialogScaffold
// - workflow controllers stay widget-unaware
// - bodies are slivers; footers use FullscreenDialogFooter

/// Padding applied to the subtitle block when present.
const _subtitlePadding = EdgeInsets.fromLTRB(16, 0, 16, 16);

/// Padding applied to the body sliver.
const _bodyPadding = EdgeInsets.fromLTRB(16, 16, 16, 24);

/// Scrollable dialog body with the standard top bar and body padding.
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
              child: Text(
                subtitleText,
                style: theme.textTheme.bodyMedium?.copyWith(
                  color: theme.colorScheme.onSurfaceVariant,
                ),
              ),
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

/// Slide-and-fade [AnimatedSwitcher] for multi-step modal dialogs.
///
/// `forward` flips the slide direction so back-stepping animates
/// right-to-left. Outgoing children are [Positioned.fill]'d under the
/// [Stack], so the dialog's size tracks the *incoming* step only —
/// without this the dialog visibly snap-shrinks when the incoming step
/// is shorter than the outgoing one. The trade-off: every step's body
/// must be **scroll-flexible** (i.e. [CustomScrollView]-rooted, as
/// produced by [FullscreenDialogBody]). Non-scrollable [Column]-rooted
/// bodies will overflow when forced into a smaller incoming step's
/// box; those callers should compose their own [AnimatedSwitcher].
///
/// Wrap a [KeyedSubtree] with a unique [ValueKey] per step inside, or
/// set the key directly on the swapped child.
class MultiStepDialogSwitcher extends StatelessWidget {
  const MultiStepDialogSwitcher({
    super.key,
    required this.child,
    this.forward = true,
    this.duration = Durations.medium2,
    this.reverseDuration,
  });

  final Widget child;

  /// `true` slides the new step in from the right (forward); `false`
  /// slides in from the left (back-stepping).
  final bool forward;

  final Duration duration;

  /// Duration for the *outgoing* step's transition. `null` falls back
  /// to [duration] (symmetric cross-fade). Pass [Duration.zero] for a
  /// snappy hand-off where outgoing steps dispose immediately — useful
  /// when steps own streams or other resources that shouldn't keep
  /// running during the slide.
  final Duration? reverseDuration;

  @override
  Widget build(BuildContext context) {
    return AnimatedSwitcher(
      duration: duration,
      reverseDuration: reverseDuration,
      switchInCurve: Curves.easeOutCubic,
      switchOutCurve: Curves.easeInCubic,
      transitionBuilder: (child, animation) {
        final offset = Tween<Offset>(
          begin: Offset(forward ? 1.0 : -1.0, 0),
          end: Offset.zero,
        ).animate(animation);
        return SlideTransition(
          position: offset,
          child: FadeTransition(opacity: animation, child: child),
        );
      },
      layoutBuilder: (currentChild, previousChildren) => Stack(
        alignment: Alignment.topCenter,
        children: [
          for (final c in previousChildren) Positioned.fill(child: c),
          if (currentChild != null) currentChild,
        ],
      ),
      child: child,
    );
  }
}

/// Multi-step dialog scaffold — the canonical chrome container for
/// every per-step dialog flow in the app.
///
/// Owns:
/// - A keyed [FullscreenDialogBody] (header + subtitle + body sliver)
///   inside a [MultiStepDialogSwitcher] so step changes slide.
/// - A pinned [FullscreenDialogFooter] (`null` omits it entirely; pass
///   `null`, not `SizedBox.shrink()`, to skip the 16-px gutter).
///
/// Each per-step builder method on the State should return a
/// fully-configured `MultiStepDialogScaffold(stepKey: ..., title: ...,
/// body: ..., footer: ..., forward: ...)`.
///
/// Defaults [showClose] to `false` because non-dismissible workflows
/// are the common case (lobbies, protocol stages). Each step that
/// *does* want a tap-to-close affordance opts in explicitly.
class MultiStepDialogScaffold extends StatelessWidget {
  const MultiStepDialogScaffold({
    super.key,
    required this.stepKey,
    required this.title,
    this.subtitle,
    this.leading,
    this.showClose = false,
    required this.body,
    this.footer,
    this.forward = true,
    this.duration = Durations.medium2,
    this.reverseDuration,
  });

  /// Drives the inner [MultiStepDialogSwitcher]'s key. Wrap with
  /// `ValueKey(...)` if it isn't already a [Key].
  final Object stepKey;

  final Widget title;
  final String? subtitle;

  /// Back / cancel icon shown in the header's leading slot. `null`
  /// hides it (use for stages where back isn't valid — match the
  /// flow's [PopScope] rules).
  final Widget? leading;

  final bool showClose;

  /// A sliver — the per-step body content.
  final Widget body;

  /// Pinned footer rendered below the switcher. `null` omits the
  /// footer entirely (no gutter painted).
  final Widget? footer;

  final bool forward;
  final Duration duration;
  final Duration? reverseDuration;

  @override
  Widget build(BuildContext context) {
    final f = footer;
    final keyValue = stepKey is Key ? stepKey as Key : ValueKey(stepKey);
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Flexible(
          child: MultiStepDialogSwitcher(
            forward: forward,
            duration: duration,
            reverseDuration: reverseDuration,
            child: FullscreenDialogBody(
              key: keyValue,
              title: title,
              subtitle: subtitle,
              leading: leading,
              showClose: showClose,
              body: body,
            ),
          ),
        ),
        if (f != null) FullscreenDialogFooter(child: f),
      ],
    );
  }
}
