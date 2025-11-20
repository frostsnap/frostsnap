import 'dart:async';
import 'dart:typed_data';
import 'package:flutter/material.dart';
import 'package:frostsnap/snackbar.dart';
import 'package:frostsnap/src/rust/api/camera.dart' as camera;
import 'camera.dart';

class NativeCameraWidget<T> extends StatefulWidget {
  final String title;
  final Future<FrameScanResult<T>> Function(camera.Frame) scanFrame;

  const NativeCameraWidget({
    super.key,
    required this.title,
    required this.scanFrame,
  });

  @override
  State<NativeCameraWidget<T>> createState() => _NativeCameraWidgetState<T>();
}

class _NativeCameraWidgetState<T> extends State<NativeCameraWidget<T>> {
  List<camera.CameraDevice>? _devices;
  camera.CameraDevice? _selectedDevice;
  Uint8List? _latestFrame;
  String? _error;
  double? progress;
  bool _finishedScanning = false;
  StreamSubscription? _cameraSubscription;

  @override
  void initState() {
    super.initState();
    _loadDevices();
  }

  Future<void> _loadDevices() async {
    try {
      final devices = await camera.CameraDevice.list();
      if (!mounted) return;

      if (devices.isEmpty) {
        setState(() => _error = "No camera devices found");
        return;
      }

      setState(() {
        _devices = devices;
        _selectedDevice = devices.first;
      });

      _startCamera(_selectedDevice!);
    } catch (e) {
      if (mounted) {
        setState(() => _error = "Failed to list cameras: $e");
      }
    }
  }

  Future<void> _startCamera(camera.CameraDevice device) async {
    await _cameraSubscription?.cancel();
    _cameraSubscription = null;

    // Small delay to let the device be released
    await Future.delayed(const Duration(milliseconds: 100));

    try {
      final frameStream = device.start();

      _cameraSubscription = frameStream.listen((frame) {
        if (_finishedScanning) return;
        _handleFrame(frame);
      });

      setState(() {
        _error = null;
      });
    } catch (e) {
      if (mounted) {
        setState(() => _error = "Failed to start camera: $e");
      }
    }
  }

  Future<void> _handleFrame(camera.Frame frame) async {
    if (_finishedScanning) return;

    setState(() {
      _latestFrame = Uint8List.fromList(frame.data);
    });

    try {
      final result = await widget.scanFrame(frame);
      if (_finishedScanning || !mounted) return;

      if (result.error != null) {
        showErrorSnackbar(context, result.error!);
        return;
      }

      if (result.progress != null) {
        setState(() {
          progress = result.progress!;
        });
      }

      if (result.result != null) {
        setState(() {
          _finishedScanning = true;
        });
        Navigator.pop(context, result.result);
      }
    } catch (e) {
      if (mounted && !_finishedScanning) {
        showErrorSnackbar(context, "Error scanning frame: $e");
      }
    }
  }

  void _switchCamera(camera.CameraDevice device) {
    if (device.index == _selectedDevice?.index) return;

    setState(() {
      _selectedDevice = device;
      _latestFrame = null;
      progress = null;
    });

    _startCamera(device);
  }

  @override
  void dispose() {
    _cameraSubscription?.cancel();
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
                SelectableText(
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
      // Display frame (should be JPEG for MJPG cameras)
      cameraPreview = Center(
        child: Image.memory(
          _latestFrame!,
          gaplessPlayback: true,
          fit: BoxFit.contain,
        ),
      );
    } else {
      cameraPreview = const Center(child: CircularProgressIndicator());
    }

    final devices = _devices;

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
          if (devices != null && devices.isNotEmpty)
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
                      padding: const EdgeInsets.symmetric(
                        horizontal: 16,
                        vertical: 8,
                      ),
                      child: Row(
                        children: [
                          Icon(Icons.videocam, color: colorScheme.onSurface),
                          const SizedBox(width: 12),
                          Expanded(
                            child: DropdownButton<int>(
                              value: _selectedDevice?.index.toInt(),
                              isExpanded: true,
                              underline: const SizedBox(),
                              dropdownColor: colorScheme.surface,
                              autofocus: false,
                              focusColor: Colors.transparent,
                              style: theme.textTheme.bodyMedium?.copyWith(
                                color: colorScheme.onSurface,
                              ),
                              items: devices.map((device) {
                                return DropdownMenuItem(
                                  value: device.index.toInt(),
                                  child: Text(
                                    '${device.name} (${device.width}x${device.height})',
                                  ),
                                );
                              }).toList(),
                              onChanged: (index) {
                                if (index != null) {
                                  final device = devices.firstWhere(
                                    (d) => d.index.toInt() == index,
                                  );
                                  _switchCamera(device);
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
          Positioned(
            bottom: 50,
            left: 24,
            right: 24,
            child: Card(
              elevation: 8,
              color: colorScheme.surface.withValues(alpha: 0.8),
              shape: RoundedRectangleBorder(
                borderRadius: BorderRadius.circular(16),
              ),
              child: Padding(
                padding: const EdgeInsets.all(20),
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Icon(
                          Icons.qr_code_scanner,
                          color: colorScheme.onSurface,
                          size: 20,
                        ),
                        const SizedBox(width: 8),
                        Text(
                          widget.title,
                          style: theme.textTheme.titleMedium?.copyWith(
                            color: colorScheme.onSurface,
                            fontWeight: FontWeight.w600,
                          ),
                        ),
                      ],
                    ),
                    if (progress != null) ...[
                      const SizedBox(height: 16),
                      LinearProgressIndicator(
                        value: progress,
                        backgroundColor: colorScheme.surfaceContainerHighest,
                        valueColor: AlwaysStoppedAnimation<Color>(
                          colorScheme.primary,
                        ),
                        minHeight: 6,
                        borderRadius: BorderRadius.circular(3),
                      ),
                      const SizedBox(height: 12),
                      Text(
                        "${(progress! * 100).round()}% complete",
                        style: theme.textTheme.bodyMedium?.copyWith(
                          color: colorScheme.onSurfaceVariant,
                        ),
                      ),
                    ],
                  ],
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}
