import 'dart:ui';

import 'package:flutter/material.dart';
import 'package:google_fonts/google_fonts.dart';

final monospaceTextStyle = GoogleFonts.notoSansMono();

final blurFilter = ImageFilter.blur(sigmaX: 2.1, sigmaY: 2.1);

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
  required String titleText,
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
          titleText: titleText,
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

  final result =
      isDialog
          ? showDialog<T>(
            context: context,
            builder:
                (context) => Dialog(
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
  return result.then<T?>(
    (r) {
      scrollController.dispose();
      return r;
    },
    onError: (r) {
      scrollController.dispose();
      return r;
    },
  );
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

class TopBar extends StatefulWidget implements PreferredSizeWidget {
  static const headerPadding = EdgeInsets.fromLTRB(24, 4, 24, 16);

  final String? titleText;
  final bool isDialog;
  final Color? backgroundColor;
  final ScrollController? scrollController;

  const TopBar({
    super.key,
    this.titleText,
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
      child:
          widget.isDialog
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
            child: Text(
              widget.titleText ?? '',
              style: theme.textTheme.titleLarge,
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
          duration: Durations.medium1,
        );
      },
    );
  }
}
