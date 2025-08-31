import 'dart:ui';

import 'package:flutter/material.dart';
import 'package:frostsnap/maybe_fullscreen_dialog.dart';
import 'package:google_fonts/google_fonts.dart';
import 'package:sliver_tools/sliver_tools.dart';

final monospaceTextStyle = GoogleFonts.notoSansMono();
final blurFilter = ImageFilter.blur(sigmaX: 21, sigmaY: 21);
const seedColor = Color(0xFF1595B2);
const double iconSize = 20.0;

Color tintSurfaceContainer(
  BuildContext context, {
  required Color tint,
  double elevation = 3.0,
}) => ElevationOverlay.applySurfaceTint(
  Theme.of(context).colorScheme.surfaceContainer,
  tint,
  elevation,
);

Color tintOnSurface(
  BuildContext context, {
  required Color tint,
  double elevation = 3.0,
}) => ElevationOverlay.applySurfaceTint(
  Theme.of(context).colorScheme.onSurface,
  tint,
  elevation,
);

Future<T?> showBottomSheetOrDialog<T>(
  BuildContext context, {
  required Widget Function(BuildContext, ScrollController) builder,
  required Widget title,
  Color? backgroundColor,
}) {
  final windowSize = WindowSizeContext.of(context);
  backgroundColor =
      backgroundColor ?? Theme.of(context).colorScheme.surfaceContainerLow;
  final scrollController = ScrollController();

  final column = ConstrainedBox(
    constraints: BoxConstraints(maxWidth: 580),
    child: Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        TopBar(
          title: title,
          backgroundColor: backgroundColor,
          scrollController: scrollController,
        ),
        Flexible(
          child: Builder(
            builder: (context) => builder(context, scrollController),
          ),
        ),
      ],
    ),
  );

  final result = windowSize != WindowSizeClass.compact
      ? showDialog<T>(
          context: context,
          builder: (context) => Dialog(
            backgroundColor: backgroundColor,
            clipBehavior: Clip.hardEdge,
            child: column,
          ),
        )
      : showModalBottomSheet<T>(
          context: context,
          clipBehavior: Clip.hardEdge,
          backgroundColor: backgroundColor,
          isScrollControlled: true,
          useSafeArea: true,
          isDismissible: true,
          showDragHandle: false,
          builder: (context) => column,
        );

  // FIXME: Actually this is not quite right since showDialog returns before the
  // route has been disposed in the lifecycle according to ChatGPT. The solution
  // is to make a stateful widget that handles this.
  result.whenComplete(scrollController.dispose);
  return result;
}

String spacedHex(String input, {int groupSize = 4, int? groupsPerLine}) {
  StringBuffer result = StringBuffer();

  for (int i = 0; i < input.length; i++) {
    result.write(input[i]);

    // Add a space after every x characters
    if ((i + 1) % groupSize == 0) {
      if (groupsPerLine != null) {
        if ((i + 1) % (groupSize * groupsPerLine) == 0) {
          result.write('\n');
        } else {
          result.write(' ');
        }
      } else {
        result.write(' ');
      }
    }
  }

  // Ensure the last group has exactly x characters by adding spaces
  int remainder = input.length % groupSize;
  if (remainder > 0) {
    for (int i = 0; i < groupSize - remainder; i++) {
      result.write('\u00A0');
    }
  }
  return result.toString();
}

class TopBarSliver extends StatelessWidget {
  final Widget? title;
  final Widget? leading;
  final bool showCloseOnCompact;
  final bool automaticallyImplyLeadingOnCompact;

  TopBarSliver({
    super.key,
    this.title,
    this.leading,
    this.showCloseOnCompact = true,
    this.automaticallyImplyLeadingOnCompact = false,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final windowSize = WindowSizeContext.of(context);
    final isFullscreen = windowSize == WindowSizeClass.compact;

    final closeButton = IconButton(
      onPressed: () => Navigator.pop(context, null),
      icon: Icon(Icons.close_rounded),
      tooltip: 'Close',
      style: IconButton.styleFrom(
        backgroundColor: theme.colorScheme.surfaceContainerHigh,
      ),
    );

    return isFullscreen
        ? SliverAppBar.large(
            pinned: true,
            title: title,
            automaticallyImplyLeading: automaticallyImplyLeadingOnCompact,
            leading: leading,
            actionsPadding: EdgeInsets.symmetric(horizontal: 8),
            actions: [if (showCloseOnCompact) closeButton],
          )
        : SliverPinnedHeader(
            child: TopBar(title: title, leadingButton: leading),
          );
  }
}

class TopBar extends StatefulWidget implements PreferredSizeWidget {
  static const headerPadding = EdgeInsets.fromLTRB(20, 12, 20, 16);
  static const animationDuration = Durations.short3;
  final Widget? title;
  final Color? backgroundColor;
  final ScrollController? scrollController;
  final Widget? leadingButton;

  const TopBar({
    super.key,
    this.title,
    this.backgroundColor,
    this.scrollController,
    this.leadingButton,
  });

  @override
  Size get preferredSize => const Size.fromHeight(64.0);

  @override
  State<TopBar> createState() => _TopBarState();
}

class _TopBarState extends State<TopBar> {
  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final windowSize = WindowSizeContext.of(context);
    final isDialog = windowSize != WindowSizeClass.compact;

    final dragHandle = SizedBox(
      height: 16,
      child: Align(
        alignment: Alignment.bottomCenter,
        child: Container(
          width: 32,
          height: 4,
          decoration: BoxDecoration(
            color: theme.colorScheme.outline,
            borderRadius: BorderRadius.circular(2),
          ),
        ),
      ),
    );

    final leadingButton = widget.leadingButton;

    final headline = Padding(
      padding: TopBar.headerPadding,
      child: Row(
        spacing: 20,
        children: [
          if (leadingButton != null) leadingButton,
          Expanded(
            child: DefaultTextStyle(
              style: theme.textTheme.titleLarge!,
              child: widget.title ?? const SizedBox.shrink(),
            ),
          ),
          if (isDialog)
            IconButton(
              onPressed: () => Navigator.pop(context),
              icon: Icon(Icons.close),
              style: IconButton.styleFrom(
                backgroundColor: theme.colorScheme.surfaceContainerHighest,
              ),
            ),
        ],
      ),
    );

    return Material(
      color: widget.backgroundColor,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisAlignment: MainAxisAlignment.spaceBetween,
        children: [
          isDialog ? SizedBox(height: 8) : dragHandle,
          headline,
          if (widget.scrollController != null)
            buildDivider(context, widget.scrollController!),
        ],
      ),
    );
  }

  Widget buildDivider(BuildContext context, ScrollController scrollController) {
    return ListenableBuilder(
      listenable: scrollController,
      builder: (context, _) {
        return AnimatedCrossFade(
          firstChild: Divider(height: 1),
          secondChild: SizedBox(height: 1),
          crossFadeState:
              scrollController.hasClients && scrollController.offset > 0
              ? CrossFadeState.showFirst
              : CrossFadeState.showSecond,
          duration: TopBar.animationDuration,
        );
      },
    );
  }
}
