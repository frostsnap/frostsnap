import 'dart:ui';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge.dart';
import 'package:frostsnap/maybe_fullscreen_dialog.dart';
import 'package:frostsnap/snackbar.dart';
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
  Widget? title,
  Color? backgroundColor,
}) {
  final windowSize = WindowSizeContext.of(context);
  backgroundColor = backgroundColor ?? Theme.of(context).colorScheme.surface;
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

/// Shows a Material Design 3 error dialog
///
/// Uses error color scheme to clearly indicate something is wrong.
/// Follows Material Design 3 guidelines with max width of 560dp.
/// See: https://m3.material.io/components/dialogs/guidelines
Future<void> showErrorDialog(
  BuildContext context, {
  required Widget title,
  required Widget content,
  String actionLabel = 'OK',
  VoidCallback? onAction,
}) {
  return showDialog<void>(
    context: context,
    builder: (context) {
      final theme = Theme.of(context);
      final colorScheme = theme.colorScheme;

      return AlertDialog(
        // Material Design 3 spec: dialogs have a maximal width of 560dp
        // https://m3.material.io/components/dialogs/guidelines
        // Note: AlertDialog doesn't enforce this by default
        // See: https://github.com/flutter/flutter/issues/163709
        constraints: const BoxConstraints(maxWidth: 560),
        icon: Icon(Icons.error_outline, color: colorScheme.error, size: 24),
        iconPadding: const EdgeInsets.only(top: 24),
        title: DefaultTextStyle(
          style: theme.textTheme.headlineSmall!.copyWith(
            color: colorScheme.onSurface,
          ),
          child: title,
        ),
        content: DefaultTextStyle(
          style: theme.textTheme.bodyMedium!.copyWith(
            color: colorScheme.onSurfaceVariant,
          ),
          child: content,
        ),
        actions: [
          TextButton(
            onPressed: () {
              Navigator.of(context).pop();
              onAction?.call();
            },
            child: Text(actionLabel),
          ),
        ],
      );
    },
  );
}

/// Shows a Material Design 3 error dialog for exceptions
///
/// Displays exception details with scrollable content and a copy button
/// for reporting. Handles AnyhowException specially by extracting the message.
Future<void> showExceptionDialog(
  BuildContext context, {
  required String subtitle,
  required Object exception,
}) {
  return showDialog<void>(
    context: context,
    builder: (context) {
      final theme = Theme.of(context);
      final colorScheme = theme.colorScheme;
      final scrollController = ScrollController();

      // Extract error message based on exception type
      final String errorMessage;
      if (exception is AnyhowException) {
        errorMessage = exception.message;
      } else {
        errorMessage = exception.toString();
      }

      return AlertDialog(
        constraints: const BoxConstraints(maxWidth: 560),
        title: Text(
          'Error',
          style: theme.textTheme.headlineSmall?.copyWith(
            color: colorScheme.onSurface,
          ),
        ),
        content: Container(
          decoration: BoxDecoration(
            color: colorScheme.errorContainer,
            borderRadius: BorderRadius.circular(12),
          ),
          padding: const EdgeInsets.all(16),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(
                Icons.error_outline,
                color: colorScheme.onErrorContainer,
                size: 32,
              ),
              const SizedBox(height: 12),
              Text(
                subtitle,
                style: theme.textTheme.titleMedium?.copyWith(
                  color: colorScheme.onErrorContainer,
                  fontWeight: FontWeight.w600,
                ),
              ),
              const SizedBox(height: 16),
              ConstrainedBox(
                constraints: const BoxConstraints(maxHeight: 300),
                child: Scrollbar(
                  controller: scrollController,
                  thumbVisibility: true,
                  child: SingleChildScrollView(
                    controller: scrollController,
                    child: SelectableText(
                      errorMessage,
                      style: theme.textTheme.bodyMedium?.copyWith(
                        color: colorScheme.onErrorContainer,
                      ),
                    ),
                  ),
                ),
              ),
            ],
          ),
        ),
        actions: [
          TextButton.icon(
            onPressed: () async {
              await Clipboard.setData(ClipboardData(text: errorMessage));
              if (context.mounted) {
                showMessageSnackbar(context, 'Error message copied');
              }
            },
            icon: const Icon(Icons.copy),
            label: const Text('Copy'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(),
            child: const Text('OK'),
          ),
        ],
      );
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

class TopBarSliver extends StatelessWidget {
  final Widget? title;
  final Widget? leading;
  final bool showCloseOnCompact;
  final bool showClose;
  final bool automaticallyImplyLeadingOnCompact;

  TopBarSliver({
    super.key,
    this.title,
    this.leading,
    this.showClose = true,
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
            actions: [if (showCloseOnCompact && showClose) closeButton],
          )
        : SliverPinnedHeader(
            child: TopBar(
              title: title,
              leadingButton: leading,
              showClose: showClose,
            ),
          );
  }
}

class TopBar extends StatefulWidget implements PreferredSizeWidget {
  static const headerPadding = EdgeInsets.fromLTRB(16, 12, 16, 16);
  static const animationDuration = Durations.short3;
  final Widget? title;
  final Color? backgroundColor;
  final ScrollController? scrollController;
  final Widget? leadingButton;
  final bool showClose;

  const TopBar({
    super.key,
    this.title,
    this.backgroundColor,
    this.scrollController,
    this.leadingButton,
    this.showClose = true,
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
          if (isDialog && widget.showClose)
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
