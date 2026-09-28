import 'package:flutter/material.dart';
import 'package:frostsnap/theme.dart';

import 'maybe_fullscreen_dialog.dart';

const _subtitlePadding = EdgeInsets.fromLTRB(16, 0, 16, 16);
const _bodyPadding = EdgeInsets.fromLTRB(16, 16, 16, 24);

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

  /// Must be a sliver.
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

class FullscreenDialogFooter extends StatelessWidget {
  const FullscreenDialogFooter({super.key, required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    // No SafeArea: MaybeFullscreenDialog owns the bottom inset.
    return Padding(
      padding: const EdgeInsets.all(
        16,
      ).add(EdgeInsets.only(bottom: MediaQuery.of(context).viewInsets.bottom)),
      child: child,
    );
  }
}

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
    final f = footer;
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
        if (f != null) FullscreenDialogFooter(child: f),
      ],
    );
  }
}

/// Outgoing steps are laid out with [Positioned.fill], so the dialog takes the
/// incoming step's size instead of snapping when a taller step leaves. That
/// forces an outgoing step into the incoming step's box, so every step must be
/// scroll-rooted (as [FullscreenDialogBody] is) or it will overflow.
class MultiStepDialogSwitcher extends StatelessWidget {
  const MultiStepDialogSwitcher({
    super.key,
    required this.child,
    this.forward = true,
    this.duration = Durations.medium2,
    this.reverseDuration,
  });

  /// Must carry a key unique to its step.
  final Widget child;
  final bool forward;
  final Duration duration;

  /// Duration of the outgoing step's transition, defaulting to [duration].
  /// [Duration.zero] disposes the outgoing step at once, so its streams stop
  /// instead of running through the slide.
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

  /// Changing it slides to a new step.
  final Object stepKey;

  final Widget title;
  final String? subtitle;
  final Widget? leading;
  final bool showClose;

  /// Must be a sliver.
  final Widget body;
  final Widget? footer;

  final bool forward;
  final Duration duration;
  final Duration? reverseDuration;

  @override
  Widget build(BuildContext context) {
    final f = footer;
    final key = stepKey is Key ? stepKey as Key : ValueKey(stepKey);
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
              key: key,
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
