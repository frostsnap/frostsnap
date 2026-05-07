import 'package:flutter/material.dart';
import 'package:frostsnap/theme.dart';

import 'maybe_fullscreen_dialog.dart';

// =============================================================================
// Canonical modal-dialog patterns
// =============================================================================
//
// Multi-step (shared footer):
//
//   MaybeFullscreenDialog.show<T>(
//     context: context,
//     barrierDismissible: false,
//     child: SomePage(...),
//   );
//
//   // Inside SomePage.build:
//   MultiStepDialogScaffold(
//     forward: _isForward,
//     body: FullscreenDialogBody(
//       key: ValueKey(_step),
//       title: const Text(...),
//       leading: <back button or null>,
//       showClose: false,
//       body: <sliver>,
//     ),
//     footer: <Row of buttons, single button, or null>,
//   )
//
// Multi-step (per-step footer): each step is a complete
// [FullscreenDialogScaffold]; wrap the switch in a
// [MultiStepDialogSwitcher] and key the swapped child.
//
// Single-step: [FullscreenDialogScaffold].
//
// Conventions:
// - Title + leading/back/close lives on the [TopBarSliver] embedded in
//   [FullscreenDialogBody] — don't roll a custom header.
// - Body is sliver-based — no `Column` + `Flexible(ListView)`.
// - Footer is [FullscreenDialogFooter] (handles keyboard inset + safe
//   area). For multi-step, prefer [MultiStepDialogScaffold] over
//   composing by hand.

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

/// Multi-step dialog scaffold: a [MultiStepDialogSwitcher] sized by
/// [Flexible] above an optional [FullscreenDialogFooter].
///
/// Use when every step shares the same footer slot (or no footer).
/// Steps with no footer should pass `footer: null`, not
/// `SizedBox.shrink()` — the footer's 16-px outer padding + safe area
/// would otherwise paint visible blank space.
///
/// For flows where each step has its own complete chrome, build each
/// step as a [FullscreenDialogScaffold] and wrap the switch in
/// [MultiStepDialogSwitcher] directly.
class MultiStepDialogScaffold extends StatelessWidget {
  const MultiStepDialogScaffold({
    super.key,
    required this.body,
    this.footer,
    this.forward = true,
    this.duration = Durations.medium2,
    this.reverseDuration,
  });

  /// The per-step body. Should be a [FullscreenDialogBody] with a
  /// unique [Key] per step so the switcher transitions on change.
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
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Flexible(
          child: MultiStepDialogSwitcher(
            forward: forward,
            duration: duration,
            reverseDuration: reverseDuration,
            child: body,
          ),
        ),
        if (f != null) FullscreenDialogFooter(child: f),
      ],
    );
  }
}
