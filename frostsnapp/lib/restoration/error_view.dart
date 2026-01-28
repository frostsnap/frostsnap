import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnap/restoration/recovery_flow.dart';
import 'package:frostsnap/dialog_content_with_actions.dart';

class ErrorView extends StatefulWidget with TitledWidget {
  final String title;
  final String message;
  final VoidCallback? onRetry;
  final bool isWarning;

  const ErrorView({
    super.key,
    required this.title,
    required this.message,
    this.onRetry,
    this.isWarning = false,
  });

  @override
  State<ErrorView> createState() => _ErrorViewState();

  @override
  String get titleText => isWarning ? 'Warning' : 'Error';
}

class _ErrorViewState extends State<ErrorView> {
  final ScrollController _scrollController = ScrollController();

  @override
  void dispose() {
    _scrollController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    final backgroundColor = widget.isWarning
        ? null
        : theme.colorScheme.errorContainer;
    final textColor = widget.isWarning
        ? null
        : theme.colorScheme.onErrorContainer;
    final iconColor = widget.isWarning
        ? null
        : theme.colorScheme.onErrorContainer;

    return DialogContentWithActions(
      content: Container(
        decoration: BoxDecoration(
          color: backgroundColor,
          borderRadius: BorderRadius.circular(12),
        ),
        padding: const EdgeInsets.all(24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              widget.isWarning
                  ? Icons.warning_amber_rounded
                  : Icons.error_outline_rounded,
              size: 64,
              color: iconColor,
            ),
            const SizedBox(height: 24),
            Text(
              widget.title,
              style: theme.textTheme.headlineSmall?.copyWith(color: textColor),
              textAlign: TextAlign.center,
            ),
            const SizedBox(height: 16),
            Container(
              constraints: BoxConstraints(maxHeight: 200),
              child: Scrollbar(
                controller: _scrollController,
                thumbVisibility: true,
                child: SingleChildScrollView(
                  controller: _scrollController,
                  child: SelectableText(
                    widget.message,
                    textAlign: TextAlign.center,
                    style: TextStyle(color: textColor),
                  ),
                ),
              ),
            ),
          ],
        ),
      ),
      actions: [
        if (!widget.isWarning)
          OutlinedButton.icon(
            icon: Icon(Icons.copy),
            label: Text('Copy Error'),
            onPressed: () {
              Clipboard.setData(ClipboardData(text: widget.message));
            },
          ),
        if (widget.onRetry != null)
          FilledButton.icon(
            icon: Icon(Icons.refresh),
            label: Text('Try Again'),
            onPressed: widget.onRetry,
            style: widget.isWarning
                ? null
                : FilledButton.styleFrom(
                    backgroundColor: theme.colorScheme.error,
                    foregroundColor: theme.colorScheme.onError,
                  ),
          ),
      ],
    );
  }
}
