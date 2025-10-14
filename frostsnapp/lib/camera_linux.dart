import 'dart:async';
import 'dart:typed_data';
import 'package:flutter/material.dart';
import 'package:flutter_lite_camera/flutter_lite_camera.dart';
import 'package:zxing2/qrcode.dart';
import 'package:image/image.dart' as img;
import 'package:frostsnap/snackbar.dart';
import 'package:frostsnap/src/rust/api/qr.dart';

class LinuxQrScanner<T> extends StatefulWidget {
  final String title;
  final Future<T?> Function(Uint8List imageBytes, String? qrText) onDetect;
  final Widget? overlayWidget;

  const LinuxQrScanner({
    super.key,
    required this.title,
    required this.onDetect,
    this.overlayWidget,
  });

  @override
  State<LinuxQrScanner<T>> createState() => _LinuxQrScannerState<T>();
}

class _LinuxQrScannerState<T> extends State<LinuxQrScanner<T>> {
  final _camera = FlutterLiteCamera();
  Timer? _scanTimer;
  bool _isInitialized = false;
  bool _finishedScanning = false;
  String? _error;
  Uint8List? _latestFrame;
  List<String> _availableDevices = [];
  int _selectedDeviceIndex = 0;

  @override
  void initState() {
    super.initState();
    _initCamera();
  }

  Future<void> _initCamera() async {
    try {
      final devices = await _camera.getDeviceList();
      if (devices.isEmpty) {
        setState(() => _error = "No camera devices found");
        return;
      }

      setState(() {
        _availableDevices = devices;
        _selectedDeviceIndex = _selectedDeviceIndex.clamp(0, devices.length - 1);
      });

      await _camera.open(_selectedDeviceIndex);
      setState(() => _isInitialized = true);

      _scanTimer = Timer.periodic(
        const Duration(milliseconds: 200),
        (_) => _captureAndScan(),
      );
    } catch (e) {
      setState(() => _error = "Failed to initialize camera: $e");
    }
  }

  Future<void> _switchCamera(int newIndex) async {
    if (newIndex == _selectedDeviceIndex || newIndex >= _availableDevices.length) {
      return;
    }

    setState(() {
      _isInitialized = false;
      _selectedDeviceIndex = newIndex;
    });

    _scanTimer?.cancel();
    await _camera.release();

    try {
      await _camera.open(_selectedDeviceIndex);
      setState(() => _isInitialized = true);

      _scanTimer = Timer.periodic(
        const Duration(milliseconds: 200),
        (_) => _captureAndScan(),
      );
    } catch (e) {
      setState(() => _error = "Failed to switch camera: $e");
    }
  }

  Future<void> _captureAndScan() async {
    if (_finishedScanning || !_isInitialized) return;

    try {
      final frameData = await _camera.captureFrame();
      final frameBytes = frameData['data'] as Uint8List?;
      if (frameBytes == null) return;

      setState(() => _latestFrame = frameBytes);

      final qrText = await _decodeQr(frameBytes);
      if (qrText != null && !_finishedScanning) {
        final result = await widget.onDetect(frameBytes, qrText);
        if (result != null && !_finishedScanning && mounted) {
          _finishedScanning = true;
          Navigator.pop(context, result);
        }
      }
    } catch (e) {
      if (mounted) {
        showErrorSnackbar(context, "Error scanning: $e");
      }
    }
  }

  Future<String?> _decodeQr(Uint8List rgbBytes) async {
    try {
      // flutter_lite_camera returns 640x480 RGB888
      final image = img.Image.fromBytes(
        width: 640,
        height: 480,
        bytes: rgbBytes.buffer,
        format: img.Format.uint8,
        numChannels: 3,
      );

      // Convert to ABGR format for zxing2 RGBLuminanceSource
      final converted = image.convert(numChannels: 4);
      final abgrBytes = converted.getBytes(order: img.ChannelOrder.abgr);
      final luminanceSource = RGBLuminanceSource(
        image.width,
        image.height,
        abgrBytes.buffer.asInt32List(),
      );
      final bitmap = BinaryBitmap(HybridBinarizer(luminanceSource));

      final reader = QRCodeReader();
      final result = reader.decode(bitmap);
      return result.text;
    } catch (e) {
      return null;
    }
  }

  @override
  void dispose() {
    _scanTimer?.cancel();
    _camera.release();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colorScheme = theme.colorScheme;

    Widget cameraPreview;
    if (_error != null) {
      cameraPreview = Center(
        child: Card(
          margin: const EdgeInsets.all(16),
          child: Padding(
            padding: const EdgeInsets.all(16),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Icon(Icons.error_outline, size: 48, color: colorScheme.error),
                const SizedBox(height: 16),
                Text(
                  _error!,
                  style: theme.textTheme.bodyLarge,
                  textAlign: TextAlign.center,
                ),
              ],
            ),
          ),
        ),
      );
    } else if (_latestFrame != null) {
      // Create a clean copy to avoid alignment/padding issues
      final cleanBytes = Uint8List.fromList(_latestFrame!);

      final image = img.Image.fromBytes(
        width: 640,
        height: 480,
        bytes: cleanBytes.buffer,
        format: img.Format.uint8,
        numChannels: 3,
        order: img.ChannelOrder.bgr,
      );
      final png = img.encodePng(image);
      cameraPreview = Center(
        child: Image.memory(
          png,
          gaplessPlayback: true,
          fit: BoxFit.contain,
        ),
      );
    } else {
      cameraPreview = const Center(child: CircularProgressIndicator());
    }

    return Scaffold(
      backgroundColor: Colors.black,
      body: Stack(
        children: [
          cameraPreview,
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
          if (_availableDevices.length > 1)
            Positioned(
              top: MediaQuery.of(context).padding.top + 16,
              left: 80,
              right: 20,
              child: Center(
                child: ConstrainedBox(
                  constraints: const BoxConstraints(maxWidth: 400),
                  child: Card(
                    color: colorScheme.surface.withValues(alpha: 0.9),
                    child: Padding(
                      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
                      child: Row(
                        children: [
                          Icon(Icons.videocam, color: colorScheme.onSurface),
                          const SizedBox(width: 12),
                          Expanded(
                            child: DropdownButton<int>(
                              value: _selectedDeviceIndex,
                              isExpanded: true,
                              underline: const SizedBox(),
                              dropdownColor: colorScheme.surface,
                              style: theme.textTheme.bodyMedium?.copyWith(
                                color: colorScheme.onSurface,
                              ),
                              items: List.generate(
                                _availableDevices.length,
                                (index) => DropdownMenuItem(
                                  value: index,
                                  child: Text(_availableDevices[index]),
                                ),
                              ),
                              onChanged: (value) {
                                if (value != null) {
                                  _switchCamera(value);
                                }
                              },
                            ),
                          ),
                        ],
                      ),
                    ),
                  ),
                ),
              ),
            ),
          if (widget.overlayWidget != null)
            Positioned(
              bottom: 50,
              left: 24,
              right: 24,
              child: widget.overlayWidget!,
            ),
        ],
      ),
    );
  }
}

class LinuxPsbtCameraReader extends StatefulWidget {
  const LinuxPsbtCameraReader({super.key});

  @override
  State<LinuxPsbtCameraReader> createState() => _LinuxPsbtCameraReaderState();
}

class _LinuxPsbtCameraReaderState extends State<LinuxPsbtCameraReader> {
  final qrReader = QrReader();
  double progress = 0.0;

  Future<Uint8List?> _handlePsbtDetection(
    Uint8List imageBytes,
    String? qrText,
  ) async {
    try {
      // Encode raw RGB bytes to PNG for the Rust decoder
      final cleanBytes = Uint8List.fromList(imageBytes);
      final image = img.Image.fromBytes(
        width: 640,
        height: 480,
        bytes: cleanBytes.buffer,
        format: img.Format.uint8,
        numChannels: 3,
        order: img.ChannelOrder.bgr,
      );
      final encodedBytes = img.encodePng(image);

      final status = await qrReader.decodeFromBytes(bytes: encodedBytes);
      switch (status) {
        case QrDecoderStatus_Progress(:final field0):
          setState(() {
            if (field0.sequenceCount > 0) {
              progress = (field0.decodedFrames.toDouble()) /
                  (field0.sequenceCount.toDouble() * 1.75);
              progress = progress.clamp(0.0, 0.99);
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

    return LinuxQrScanner<Uint8List>(
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

class LinuxAddressScanner extends StatelessWidget {
  const LinuxAddressScanner({super.key});

  Future<String?> _handleAddressDetection(
    Uint8List imageBytes,
    String? qrText,
  ) async {
    return qrText;
  }

  @override
  Widget build(BuildContext context) {
    return LinuxQrScanner<String>(
      title: 'Scan Address',
      onDetect: _handleAddressDetection,
    );
  }
}
