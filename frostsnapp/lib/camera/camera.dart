import 'dart:io';
import 'dart:typed_data';
import 'package:flutter/material.dart';
import 'package:frostsnap/src/rust/api/qr.dart';
import 'package:frostsnap/src/rust/api/camera.dart' as camera;
import 'camera_native.dart';
import 'camera_mobile.dart';

class FrameScanResult<T> {
  final T? result;
  final double? progress;
  final String? error;

  const FrameScanResult({this.result, this.progress, this.error});

  static FrameScanResult<T> success<T>(T result) =>
      FrameScanResult(result: result);

  static FrameScanResult<T> withProgress<T>(double progress) =>
      FrameScanResult(progress: progress);

  static FrameScanResult<T> withError<T>(String error) =>
      FrameScanResult(error: error);
}

// PSBT-specific scanner with progress overlay
class PsbtCameraReader extends StatefulWidget {
  const PsbtCameraReader({super.key});

  @override
  State<PsbtCameraReader> createState() => _PsbtCameraReaderState();
}

class _PsbtCameraReaderState extends State<PsbtCameraReader> {
  final qrReader = PsbtQrDecoder();
  bool _processing = false;
  double _currentProgress = 0.0;

  Future<FrameScanResult<Uint8List>> _scanPsbtFrame(camera.Frame frame) async {
    // Drop frame if already processing, return current progress
    if (_processing) return FrameScanResult(progress: _currentProgress);

    _processing = true;
    try {
      final status = await qrReader.decodeQrFrame(frame: frame);
      switch (status) {
        case QrDecoderStatus_Progress(:final progress):
          _currentProgress = progress.toDouble();
          return FrameScanResult(progress: _currentProgress);
        case QrDecoderStatus_Decoded(:final field0):
          return FrameScanResult(result: field0);
        case QrDecoderStatus_Failed(:final field0):
          return FrameScanResult(
            error: "Failed to decode QR: $field0",
            progress: _currentProgress,
          );
      }
    } catch (e) {
      return FrameScanResult(
        error: "Error decoding frame: $e",
        progress: _currentProgress,
      );
    } finally {
      _processing = false;
    }
  }

  @override
  void dispose() {
    qrReader.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    if (Platform.isLinux || Platform.isWindows) {
      return NativeCameraWidget<Uint8List>(
        title: "Scan PSBT",
        scanFrame: _scanPsbtFrame,
      );
    }
    return MobileCameraWidget<Uint8List>(
      title: "Scan PSBT",
      scanFrame: _scanPsbtFrame,
    );
  }
}

/// Generic QR-string scanner: reads any QR code it sees and returns
/// the raw string value. Callers parse/validate as needed (bitcoin
/// address, frostsnap invite URL, …). [title] is the scanner chrome
/// title — pass what the user is scanning so the header matches.
class QrStringScanner extends StatefulWidget {
  const QrStringScanner({super.key, required this.title});

  final String title;

  @override
  State<QrStringScanner> createState() => _QrStringScannerState();
}

class _QrStringScannerState extends State<QrStringScanner> {
  bool _processing = false;

  Future<String?> _onDetect(capture) async {
    if (capture.barcodes.isNotEmpty) {
      return capture.barcodes.first.rawValue;
    }
    return null;
  }

  Future<FrameScanResult<String>> _scanFrame(camera.Frame frame) async {
    if (_processing) return FrameScanResult();

    _processing = true;
    try {
      final qrStrings = await readQrCodeBytes(bytes: frame.data);
      if (qrStrings.isNotEmpty) {
        return FrameScanResult(result: qrStrings.first);
      }
      return FrameScanResult();
    } catch (e) {
      return FrameScanResult(error: "Error scanning QR: $e");
    } finally {
      _processing = false;
    }
  }

  @override
  Widget build(BuildContext context) {
    if (Platform.isLinux || Platform.isWindows) {
      return NativeCameraWidget<String>(
        title: widget.title,
        scanFrame: _scanFrame,
      );
    }
    return MobileQrScanner<String>(title: widget.title, onDetect: _onDetect);
  }
}
