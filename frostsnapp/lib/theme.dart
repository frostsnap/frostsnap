import 'dart:ui';

import 'package:flutter/material.dart';
import 'package:google_fonts/google_fonts.dart';

final monospaceTextStyle = GoogleFonts.notoSansMono();
final blurFilter = ImageFilter.blur(sigmaX: 21, sigmaY: 21);
const seedColor = Color(0xFF1595B2);

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
  final mediaSize = MediaQuery.sizeOf(context);
  backgroundColor =
      backgroundColor ?? Theme.of(context).colorScheme.surfaceContainerLow;
  final scrollController = ScrollController();

  final isDialog = (mediaSize.width >= 600);

  final column = ConstrainedBox(
    constraints: BoxConstraints(maxWidth: 580),
    child: Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        TopBar(
          title: title,
          backgroundColor: backgroundColor,
          isDialog: isDialog,
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

  final result = isDialog
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

WidgetSpan buildTag(BuildContext context, {required String text}) {
  final theme = Theme.of(context);
  return WidgetSpan(
    alignment: PlaceholderAlignment.middle,
    child: Card.filled(
      color: theme.colorScheme.surfaceContainerLowest.withAlpha(128),
      margin: const EdgeInsets.all(12.0),
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 4.0, horizontal: 8.0),
        child: Text(
          text,
          style: theme.textTheme.labelSmall?.copyWith(
            color: theme.colorScheme.error,
            fontWeight: FontWeight.w500,
          ),
        ),
      ),
    ),
  );
}

class TopBar extends StatefulWidget implements PreferredSizeWidget {
  static const headerPadding = EdgeInsets.fromLTRB(20, 0, 20, 16);
  static const animationDuration = Durations.short3;

  final Widget? title;
  final bool isDialog;
  final Color? backgroundColor;
  final ScrollController? scrollController;

  const TopBar({
    super.key,
    this.title,
    this.backgroundColor,
    this.scrollController,
    this.isDialog = false,
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
    final maybeDragHandle = SizedBox(
      height: 20.0,
      child: widget.isDialog
          ? null
          : Center(
              child: Container(
                width: 36,
                height: 4,
                decoration: BoxDecoration(
                  color: theme.colorScheme.outline,
                  borderRadius: BorderRadius.circular(2),
                ),
              ),
            ),
    );
    final headline = Padding(
      padding: TopBar.headerPadding,
      child: Row(
        mainAxisAlignment: MainAxisAlignment.spaceBetween,
        children: [
          Expanded(
            child: DefaultTextStyle(
              style: theme.textTheme.titleLarge!,
              child: widget.title ?? const SizedBox.shrink(),
            ),
          ),
          if (widget.isDialog)
            IconButton(
              onPressed: () => Navigator.pop(context),
              icon: Icon(Icons.close),
              iconSize: 24,
              padding: EdgeInsets.zero,
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
          maybeDragHandle,
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
