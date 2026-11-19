import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

/// The Linux warning: capture can't be blocked there (spec §0.6.1.5).
const linuxCaptureWarning =
    "Screen capture can't be blocked on this computer. Close screen-sharing "
    'and recording apps.';

/// The same warning on Android, when the phone refused to block capture.
const androidCaptureWarning =
    "Screen capture couldn't be blocked on this phone. Close screen-sharing "
    'and recording apps.';

/// Wraps a card-flow or card-restore route (design §5.2, §5.3), the pages
/// that show or take card words.
/// - Android: blocks screen capture (FLAG_SECURE) and keeps the screen on
///   while it is up, through MainActivity's `secure_window` channel. If the
///   phone refuses, it shows [androidCaptureWarning] instead of going on
///   silently.
/// - Linux: shows [linuxCaptureWarning] above its child.
///
/// Nested ones share the flags, which are cleared only when the last one
/// goes, and only the outermost shows a warning.
class SecureScreen extends StatefulWidget {
  final Widget child;

  const SecureScreen({super.key, required this.child});

  static const channel = MethodChannel('com.frostsnap/secure_window');

  @override
  State<SecureScreen> createState() => _SecureScreenState();
}

/// Marks a subtree that is already under a [SecureScreen].
class _SecureScope extends InheritedWidget {
  const _SecureScope({required super.child});

  @override
  bool updateShouldNotify(_SecureScope old) => false;
}

class _SecureScreenState extends State<SecureScreen> {
  static var _open = 0;

  /// Counts the calls to [_setFlags], so only the latest one's answer sets
  /// the warning.
  static var _generation = 0;

  /// Android refused a flag while the screens are up.
  static final _refused = ValueNotifier(false);

  @override
  void initState() {
    super.initState();
    _open += 1;
    if (_open == 1) _setFlags(true);
  }

  @override
  void dispose() {
    _open -= 1;
    if (_open == 0) _setFlags(false);
    super.dispose();
  }

  static Future<void> _setFlags(bool on) async {
    if (kIsWeb || defaultTargetPlatform != TargetPlatform.android) return;
    final generation = ++_generation;
    // The warning changes only once the phone answers, never during a
    // build.
    try {
      await SecureScreen.channel.invokeMethod<void>('setSecure', on);
      await SecureScreen.channel.invokeMethod<void>('keepScreenOn', on);
      if (generation == _generation) _refused.value = false;
    } on PlatformException {
      // Nothing is logged (nothing under lib/verifiable_card logs, design
      // §8); the user is told instead.
      if (generation == _generation) _refused.value = on;
    } on MissingPluginException {
      // No Android embedding (a test, or a desktop build run as Android).
    }
  }

  @override
  Widget build(BuildContext context) {
    if (context.getInheritedWidgetOfExactType<_SecureScope>() != null) {
      return widget.child;
    }
    final Widget content;
    if (kIsWeb) {
      content = widget.child;
    } else if (defaultTargetPlatform == TargetPlatform.linux) {
      content = _warned(context, linuxCaptureWarning);
    } else if (defaultTargetPlatform == TargetPlatform.android) {
      content = ValueListenableBuilder(
        valueListenable: _refused,
        builder: (context, refused, _) =>
            _warned(context, refused ? androidCaptureWarning : null),
      );
    } else {
      content = widget.child;
    }
    return _SecureScope(child: content);
  }

  /// [widget.child] under [warning], if any. The child keeps its place, and
  /// its state, when the warning comes.
  Widget _warned(BuildContext context, String? warning) {
    final theme = Theme.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (warning != null)
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 8, 16, 0),
            child: Row(
              spacing: 8,
              children: [
                Icon(
                  Icons.screen_share_outlined,
                  size: 20,
                  color: theme.colorScheme.error,
                ),
                Expanded(
                  child: Text(
                    warning,
                    style: theme.textTheme.bodySmall?.copyWith(
                      color: theme.colorScheme.error,
                    ),
                  ),
                ),
              ],
            ),
          ),
        Expanded(
          key: const ValueKey('secure-screen-child'),
          child: widget.child,
        ),
      ],
    );
  }
}
