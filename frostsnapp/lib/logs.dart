import 'dart:async';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnapp/ffi.dart';
import 'package:frostsnapp/stream_ext.dart';
import 'package:frostsnapp/theme.dart';
import 'package:rxdart/rxdart.dart';

class LogManager {
  static final LogManager _singleton = LogManager._internal();
  final List<LogEntry> _logs = [];

  final BehaviorSubject<List<LogEntry>> _logStreamController =
      BehaviorSubject<List<LogEntry>>();

  late StreamSubscription<LogEntry> _subscription;

  Stream<List<LogEntry>> get stream => _logStreamController.stream;

  factory LogManager() {
    return _singleton;
  }

  LogManager._internal() {
    final stream = api.subLogEvents().toBehaviorSubject();
    _subscription = stream.listen((log) {
      _logs.add(log);
      _logStreamController.add(List.unmodifiable(_logs));
    });
  }

  List<LogEntry> get logs => _logs;

  void dispose() {
    _subscription.cancel();
    _logStreamController.close();
  }
}

class LogScreen extends StatefulWidget {
  const LogScreen({super.key});

  @override
  _LogScreenState createState() => _LogScreenState();
}

class _LogScreenState extends State<LogScreen> {
  List<LogEntry> _logs = [];
  late StreamSubscription<List<LogEntry>> _subscription;
  final ScrollController _scrollController = ScrollController();

  @override
  void initState() {
    super.initState();
    _subscription = LogManager().stream.listen((logs) {
      setState(() {
        _logs = logs;
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
      return TextSpan(
        children: [
          TextSpan(
            text: '[${log.level}] ',
            style: TextStyle(
              fontWeight: FontWeight.w600,
              color: _getLevelColor(log.level),
            ),
          ),
          TextSpan(
            text: log.content,
            style: TextStyle(
              fontFamily: 'Courier',
              color: textColor,
            ),
          ),
          TextSpan(text: '\n'),
        ],
      );
    }).toList();

    return Scaffold(
      appBar: AppBar(
        title: const Text('App Logs'),
      ),
      body: _logs.isEmpty
          ? const Center(child: Text('No logs yet.'))
          : Padding(
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
                      final String combinedLogs = _logs
                          .map((log) => '[${log.level}] ${log.content}')
                          .join('\n');
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
