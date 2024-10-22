import 'dart:async';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnapp/theme.dart';

class LogPane extends StatefulWidget {
  final Stream<String> logStream;
  const LogPane({super.key, required this.logStream});

  @override
  State<LogPane> createState() => _LogPane();
}

class _LogPane extends State<LogPane> {
  final List<String> _logs = [];
  late StreamSubscription<String> _subscription;
  final ScrollController _scrollController = ScrollController();

  @override
  void initState() {
    super.initState();
    _subscription = widget.logStream.listen((log) {
      setState(() {
        _logs.add(log);
      });
      // scroll to the bottom of the scrollable view
      WidgetsBinding.instance.addPostFrameCallback((_) {
        _scrollController.jumpTo(_scrollController.position.maxScrollExtent);
      });
      // scroll to the bottom of the scrollable view
      WidgetsBinding.instance.addPostFrameCallback((_) {
        _scrollController.jumpTo(_scrollController.position.maxScrollExtent);
      });
    });
  }

  @override
  void dispose() {
    _subscription.cancel();
    _scrollController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final List<TextSpan> logSpans = _logs.map((log) {
      try {
        if (!log.startsWith("20")) {
          throw Exception("not this millenium or not a date");
        }
        final sections = log.split(RegExp(r' +'));
        return TextSpan(
          children: [
            TextSpan(
              text: sections.sublist(0, 2).join(" "),
              style: TextStyle(
                fontWeight: FontWeight.w600,
                color: _getLevelColor(sections[1]),
              ),
            ),
            TextSpan(
              text: ' ${sections.sublist(2).join(" ")}',
            ),
            TextSpan(text: '\n'),
          ],
        );
      } catch (e) {
        return TextSpan(text: log);
      }
    }).toList();

    return Padding(
      padding: const EdgeInsets.all(16.0),
      child: Column(
        children: [
          Expanded(
            child: Container(
              width: double.infinity,
              padding: EdgeInsets.all(8.0),
              decoration: BoxDecoration(
                color: backgroundSecondaryColor,
                borderRadius: BorderRadius.circular(4.0),
                border: Border.all(color: textSecondaryColor),
              ),
              child: SingleChildScrollView(
                controller: _scrollController,
                child: SelectableText.rich(
                  TextSpan(
                    children: logSpans,
                  ),
                  style: TextStyle(
                    fontFamily: 'Courier',
                    color: textColor,
                  ),
                  dragStartBehavior: DragStartBehavior.down,
                ),
              ),
            ),
          ),
          SizedBox(height: 16),
          IconButton(
            icon: const Icon(Icons.content_copy),
            onPressed: () {
              final String combinedLogs = _logs.map((log) => log).join('\n');
              Clipboard.setData(ClipboardData(text: combinedLogs));
              ScaffoldMessenger.of(context).showSnackBar(
                const SnackBar(
                  content: Text('Logs copied to clipboard!'),
                  duration: Duration(seconds: 1),
                ),
              );
            },
            tooltip: 'Copy All Logs',
          ),
        ],
      ),
    );
  }

  Color _getLevelColor(String level) {
    switch (level.toUpperCase()) {
      case 'ERROR':
        return errorColor;
      case 'DEBUG':
        return uninterestedColor;
      case 'INFO':
        return awaitingColor;
      case 'WARNING':
        return awaitingColor;
      default:
        return textColor;
    }
  }
}
