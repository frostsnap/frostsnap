import 'package:flutter/material.dart';
import 'package:frostsnap/maybe_fullscreen_dialog.dart';

/// A simple layout widget for dialog content with action buttons at the bottom.
/// Provides consistent layout structure with internal scrolling, no visual chrome.
class DialogContentWithActions extends StatelessWidget {
  final Widget content;
  final List<Widget> actions;
  final MainAxisAlignment actionsAlignment;

  const DialogContentWithActions({
    super.key,
    required this.content,
    required this.actions,
    this.actionsAlignment = MainAxisAlignment.end,
  });

  @override
  Widget build(BuildContext context) {
    // 📱 Skip divider on mobile - fullscreen dialog doesn't need visual separation
    final windowSize = WindowSizeContext.of(context);
    final isCompact = windowSize == WindowSizeClass.compact;

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.all(16.0),
          child: ConstrainedBox(
            constraints: const BoxConstraints(minHeight: 120, maxHeight: 360),
            child: SingleChildScrollView(child: content),
          ),
        ),
        if (!isCompact) const Divider(height: 0),
        Padding(
          padding: const EdgeInsets.all(16.0),
          child: Row(
            mainAxisAlignment: actionsAlignment,
            spacing: 8,
            children: actions,
          ),
        ),
      ],
    );
  }
}
