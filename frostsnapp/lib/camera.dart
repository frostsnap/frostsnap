import 'dart:io' show Platform;
import 'dart:math';
import 'dart:typed_data';
import 'package:flutter/material.dart';
import 'package:mobile_scanner/mobile_scanner.dart';
import 'package:frostsnap/snackbar.dart';
import 'package:frostsnap/src/rust/api/qr.dart';
import 'package:frostsnap/camera_linux.dart';

class QrScanner<T> extends StatefulWidget {
  final String title;
  final Future<T?> Function(BarcodeCapture) onDetect;
  final Widget? overlayWidget;

  const QrScanner({
    super.key,
    required this.title,
    required this.onDetect,
    this.overlayWidget,
  });

  @override
  State<QrScanner<T>> createState() => _QrScannerState<T>();
}

class _QrScannerState<T> extends State<QrScanner<T>> {
  final controller = MobileScannerController(
    facing: CameraFacing.back,
    returnImage: true,
    detectionSpeed: DetectionSpeed.normal,
    // Lower resolution significantly improves animated PSBT scanning speed
    // without noticeably affecting single-frame address QR detection
    cameraResolution: Size(640, 480),
  );
  double _zoom = 0.0;
  double _baseZoom = 0.0;
  bool finishedScanning = false;

  Future<void> _onDetect(BarcodeCapture capture) async {
    if (finishedScanning) return;
    try {
      final result = await widget.onDetect(capture);
      if (result != null && !finishedScanning && mounted) {
        finishedScanning = true;
        Navigator.pop(context, result);
      }
    } catch (e) {
      if (mounted) {
        showErrorSnackbar(context, "Error scanning QR: $e");
      }
    }
  }

  Future<void> _updateZoom(double newZoom) async {
    if ((newZoom - _zoom).abs() > 0.03) {
      setState(() => _zoom = newZoom);
      try {
        await controller.setZoomScale(_zoom);
      } catch (e) {
        // ignore
      }
    }
  }

  @override
  void dispose() {
    controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colorScheme = theme.colorScheme;

    return Scaffold(
      backgroundColor: Colors.black,
      body: Stack(
        children: [
          GestureDetector(
            onScaleStart: (details) => _baseZoom = _zoom,
            onScaleUpdate: (details) {
              final newZoom = (_baseZoom + (details.scale - 1.0) * 0.3).clamp(
                0.0,
                1.0,
              );
              _updateZoom(newZoom);
            },
            child: MobileScanner(controller: controller, onDetect: _onDetect),
          ),
          Positioned(
            top: MediaQuery.of(context).padding.top + 16,
            left: 16,
            child: FilledButton.tonal(
              onPressed: () => Navigator.pop(context),
              style: FilledButton.styleFrom(
                shape: const CircleBorder(),
                padding: const EdgeInsets.all(12),
                backgroundColor: colorScheme.surface.withValues(alpha: 0.8),
                foregroundColor: colorScheme.onSurface,
              ),
              child: const Icon(Icons.close),
            ),
          ),
          Positioned(
            bottom: 50,
            left: 20,
            right: 20,
            child: Card(
              color: colorScheme.surface.withValues(alpha: 0.8),
              elevation: 4,
              shape: RoundedRectangleBorder(
                borderRadius: BorderRadius.circular(28),
              ),
              child: Padding(
                padding: const EdgeInsets.symmetric(
                  horizontal: 16,
                  vertical: 8,
                ),
                child: Row(
                  children: [
                    Icon(
                      Icons.zoom_out,
                      color: colorScheme.onSurface,
                      size: 20,
                    ),
                    Expanded(
                      child: SliderTheme(
                        data: SliderTheme.of(context).copyWith(
                          activeTrackColor: colorScheme.primary,
                          inactiveTrackColor: colorScheme.outline,
                          thumbColor: colorScheme.primary,
                          overlayColor: colorScheme.primary.withValues(
                            alpha: 0.8,
                          ),
                        ),
                        child: Slider(
                          value: _zoom,
                          onChanged: (value) => _updateZoom(value),
                        ),
                      ),
                    ),
                    Icon(Icons.zoom_in, color: colorScheme.onSurface, size: 20),
                  ],
                ),
              ),
            ),
          ),
          if (widget.overlayWidget != null)
            Positioned(
              bottom: 140,
              left: 24,
              right: 24,
              child: widget.overlayWidget!,
            ),
        ],
      ),
    );
  }
}

// Platform-aware PSBT camera reader
class PsbtCameraReader extends StatelessWidget {
  const PsbtCameraReader({super.key});

  // Use flutter_lite_camera on Linux/Windows, mobile_scanner elsewhere
  static bool get _useDesktopCamera => Platform.isLinux || Platform.isWindows;

  @override
  Widget build(BuildContext context) {
    if (_useDesktopCamera) {
      return const LinuxPsbtCameraReader();
    }
    return const _MobilePsbtCameraReader();
  }
}

// PSBT-specific scanner with progress overlay (for mobile/macOS/web)
class _MobilePsbtCameraReader extends StatefulWidget {
  const _MobilePsbtCameraReader();

  @override
  State<_MobilePsbtCameraReader> createState() => _MobilePsbtCameraReaderState();
}

class _MobilePsbtCameraReaderState extends State<_MobilePsbtCameraReader> {
  final qrReader = QrReader();
  double progress = 0.0;

  Future<Uint8List?> _handlePsbtDetection(BarcodeCapture capture) async {
    final imageBytes = capture.image;
    if (imageBytes == null) {
      return null;
    }

    try {
      final status = await qrReader.decodeFromBytes(bytes: imageBytes);
      switch (status) {
        case QrDecoderStatus_Progress(:final field0):
          setState(() {
            if (field0.sequenceCount > 0) {
              progress = min(
                (field0.decodedFrames.toDouble()) /
                    (field0.sequenceCount.toDouble() * 1.75),
                0.99,
              );
            } else {
              progress = 0;
            }
          });
          return null;
        case QrDecoderStatus_Decoded(:final field0):
          setState(() => progress = 1);
          return field0;
        case QrDecoderStatus_Failed(:final field0):
          throw Exception(field0);
      }
    } catch (e) {
      throw Exception("Failed to decode QR: $e");
    }
  }

  @override
  void dispose() {
    qrReader.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colorScheme = theme.colorScheme;

    return QrScanner<Uint8List>(
      title: 'Scan PSBT',
      onDetect: _handlePsbtDetection,
      overlayWidget: Card(
        elevation: 8,
        color: colorScheme.surface.withValues(alpha: 0.8),
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(16)),
        child: Padding(
          padding: const EdgeInsets.all(20),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(
                "Scan PSBT",
                style: theme.textTheme.titleMedium?.copyWith(
                  color: colorScheme.onSurface,
                  fontWeight: FontWeight.w600,
                ),
              ),
              const SizedBox(height: 16),
              LinearProgressIndicator(
                value: progress,
                backgroundColor: colorScheme.surfaceContainerHighest,
                valueColor: AlwaysStoppedAnimation<Color>(colorScheme.primary),
                minHeight: 6,
                borderRadius: BorderRadius.circular(3),
              ),
              const SizedBox(height: 12),
              Text(
                "${(progress * 100).round()}% complete",
                style: theme.textTheme.bodyMedium?.copyWith(
                  color: colorScheme.onSurfaceVariant,
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

// Platform-aware address scanner
class AddressScanner extends StatelessWidget {
  const AddressScanner({super.key});

  // Use flutter_lite_camera on Linux/Windows, mobile_scanner elsewhere
  static bool get _useDesktopCamera => Platform.isLinux || Platform.isWindows;

  @override
  Widget build(BuildContext context) {
    if (_useDesktopCamera) {
      return const LinuxAddressScanner();
    }
    return const _MobileAddressScanner();
  }
}

class _MobileAddressScanner extends StatelessWidget {
  const _MobileAddressScanner();

  Future<String?> _handleAddressDetection(BarcodeCapture capture) async {
    if (capture.barcodes.isNotEmpty) {
      return capture.barcodes.first.rawValue;
    }
    return null;
  }

  @override
  Widget build(BuildContext context) {
    return QrScanner<String>(
      title: 'Scan Address',
      onDetect: _handleAddressDetection,
    );
  }
}
