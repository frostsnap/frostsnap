import 'dart:io';
import 'package:flutter/services.dart';
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

  /// Convenience method to get encryption key from global instance
  static Future<SymmetricKey> getEncryptionKey() async {
    return await instance.getOrCreateKey();
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
}
