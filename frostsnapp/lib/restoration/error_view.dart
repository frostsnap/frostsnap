import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnap/restoration/choose_method_view.dart';
import 'package:frostsnap/restoration/material_dialog_card.dart';

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
    return SingleChildScrollView(
      physics: NeverScrollableScrollPhysics(),
      child: MaterialDialogCard(
        iconData: widget.isWarning
            ? Icons.warning_amber_rounded
            : Icons.error_outline_rounded,
        title: Text(widget.title),
        content: Container(
          constraints: BoxConstraints(maxHeight: 200),
          child: Scrollbar(
            controller: _scrollController,
            thumbVisibility: true,
            child: SingleChildScrollView(
              controller: _scrollController,
              child: SelectableText(
                widget.message,
                textAlign: TextAlign.center,
              ),
            ),
          ),
        ),
        backgroundColor: widget.isWarning
            ? theme.colorScheme.surfaceContainerHigh
            : theme.colorScheme.errorContainer,
        textColor: widget.isWarning
            ? theme.colorScheme.onSurface
            : theme.colorScheme.onErrorContainer,
        iconColor: widget.isWarning
            ? theme.colorScheme.onSurfaceVariant
            : theme.colorScheme.onErrorContainer,
        actions: [
          if (!widget.isWarning)
            OutlinedButton.icon(
              icon: Icon(Icons.copy),
              label: Text('Copy Error'),
              onPressed: () {
                Clipboard.setData(ClipboardData(text: widget.message));
              },
              style: OutlinedButton.styleFrom(
                foregroundColor: theme.colorScheme.onErrorContainer,
                side: BorderSide(color: theme.colorScheme.onErrorContainer),
              ),
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
      ),
    );
  }
}
