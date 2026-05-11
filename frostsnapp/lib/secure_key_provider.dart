import 'dart:async';
import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/lib.dart';

/// Platform-agnostic interface for secure key management
abstract class SecureKeyProvider {
  /// Get or create the encryption key
  Future<SymmetricKey> getOrCreateKey();

  /// Check if authentication is required
  Future<bool> requiresAuthentication();

  /// Clear the authentication cache
  Future<void> clearKey();

  /// Delete the key from storage
  Future<void> deleteKey();

  /// Open the OS settings page where the user can configure a screen lock.
  Future<void> openSecuritySettings();

  /// Factory method to create the appropriate provider for the platform
  static SecureKeyProvider create() {
    if (Platform.isAndroid) {
      return AndroidSecureKeyProvider._();
    } else {
      return DesktopSecureKeyProvider._();
    }
  }

  /// Global singleton instance (lazy initialization)
  static SecureKeyProvider? _instance;
  static SecureKeyProvider get instance {
    _instance ??= create();
    return _instance!;
  }

  /// Convenience method to get encryption key from global instance.
  ///
  /// On Android, if there's no device lock screen the Keystore can't bind a
  /// user-auth-required key, so we surface a global blocking dialog asking the
  /// user to set one up. The call is retried after they signal they're ready,
  /// and rethrows the original PlatformException if they cancel.
  static Future<SymmetricKey> getEncryptionKey() async {
    while (true) {
      try {
        return await instance.getOrCreateKey();
      } on PlatformException catch (e) {
        if (e.code != _noLockScreenCode) rethrow;
        final retry = await _ensureLockScreenDialog();
        if (!retry) rethrow;
      }
    }
  }

  static const _noLockScreenCode = 'NO_LOCK_SCREEN';

  static Future<bool>? _pendingDialog;

  /// Concurrent callers share a single dialog future and all retry on the same
  /// user gesture. The nav context may briefly be null at app startup before
  /// MaterialApp mounts, so we wait for it instead of failing silently —
  /// otherwise the recovery-mode listener that fires before runApp would
  /// suppress the dialog entirely.
  static Future<bool> _ensureLockScreenDialog() {
    final existing = _pendingDialog;
    if (existing != null) return existing;
    final future = _showLockScreenDialog();
    _pendingDialog = future;
    future.whenComplete(() => _pendingDialog = null);
    return future;
  }

  static Future<bool> _showLockScreenDialog() async {
    BuildContext? ctx = rootNavKey.currentContext;
    for (int i = 0; i < 100 && ctx == null; i++) {
      await Future.delayed(const Duration(milliseconds: 50));
      ctx = rootNavKey.currentContext;
    }
    if (ctx == null) {
      throw StateError(
        'SecureKeyProvider: no navigator context available to show '
        'NO_LOCK_SCREEN dialog after 5s',
      );
    }
    final result = await showDialog<bool>(
      context: ctx,
      barrierDismissible: false,
      builder: (_) => const _NoLockScreenDialog(),
    );
    return result ?? false;
  }
}

class _NoLockScreenDialog extends StatelessWidget {
  const _NoLockScreenDialog();

  Future<void> _openSettings() async {
    try {
      await SecureKeyProvider.instance.openSecuritySettings();
    } catch (e) {
      final messenger = rootScaffoldMessengerKey.currentState;
      messenger?.showSnackBar(
        SnackBar(content: Text('Could not open security settings: $e')),
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: const Text('Screen Lock Required'),
      content: const Text(
        'Frostsnap protects sensitive data on this phone using your screen '
        'lock (PIN, password, pattern, or biometrics). Please set up a screen '
        'lock in your phone\'s security settings to continue.',
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(false),
          child: const Text('Cancel'),
        ),
        TextButton(
          onPressed: () => Navigator.of(context).pop(true),
          child: const Text('Try again'),
        ),
        FilledButton(
          onPressed: _openSettings,
          child: const Text('Open Settings'),
        ),
      ],
    );
  }
}

/// Android implementation using the native SecureKeyManager plugin
class AndroidSecureKeyProvider extends SecureKeyProvider {
  static const _channel = MethodChannel('com.frostsnap/secure_key');

  AndroidSecureKeyProvider._();

  @override
  Future<SymmetricKey> getOrCreateKey() async {
    final List<int> keyBytes = await _channel.invokeMethod('getOrCreateKey');
    // Create SymmetricKey directly without Rust call
    final array = U8Array32(Uint8List.fromList(keyBytes));
    return SymmetricKey(field0: array);
  }

  @override
  Future<bool> requiresAuthentication() async {
    return await _channel.invokeMethod('requiresAuthentication');
  }

  @override
  Future<void> clearKey() async {
    await _channel.invokeMethod('clearKey');
  }

  @override
  Future<void> deleteKey() async {
    await _channel.invokeMethod('deleteKey');
  }

  @override
  Future<void> openSecuritySettings() async {
    await _channel.invokeMethod('openSecuritySettings');
  }
}

/// Desktop implementation using a temporary hardcoded key
class DesktopSecureKeyProvider extends SecureKeyProvider {
  static final _tempKeyBytes = [
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
  ];

  DesktopSecureKeyProvider._();

  @override
  Future<SymmetricKey> getOrCreateKey() async {
    // Create SymmetricKey directly without Rust call
    final array = U8Array32(Uint8List.fromList(_tempKeyBytes));
    return SymmetricKey(field0: array);
  }

  @override
  Future<bool> requiresAuthentication() async {
    return false;
  }

  @override
  Future<void> clearKey() async {
    // No-op for desktop
  }

  @override
  Future<void> deleteKey() async {
    // No-op for desktop
  }

  @override
  Future<void> openSecuritySettings() async {
    // No-op for desktop
  }
}
