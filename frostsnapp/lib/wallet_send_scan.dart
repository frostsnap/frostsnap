import 'package:camera/camera.dart';
import 'package:collection/collection.dart';
import 'package:flutter/material.dart';
import 'package:frostsnap/image_converter.dart';
import 'package:frostsnap/src/rust/api/qr.dart';

class SendScanBody extends StatefulWidget {
  final List<CameraDescription> cameras;
  final int initialSelected;

  const SendScanBody({
    super.key,
    required this.cameras,
    this.initialSelected = 0,
  });

  @override
  State<SendScanBody> createState() => _SendScanBodyState();
}

class _SendScanBodyState extends State<SendScanBody> {
  final utils = ImageUtils();
  final qrReader = QrReader();

  // Ensure that we only pop once.
  bool popped = false;
  late int selected;
  final List<CameraController?> controllers = [];

  @override
  void initState() {
    super.initState();
    selected = widget.initialSelected;
    for (final (i, _) in widget.cameras.indexed) {
      controllers.insert(i, null);
    }
    setCamera(context);
  }

  @override
  void dispose() async {
    for (final controller in controllers) {
      await controller?.dispose();
    }
    qrReader.dispose();
    super.dispose();
  }

  incrementSelected() {
    selected += 1;
    if (selected >= widget.cameras.length) {
      selected = 0;
    }
  }

  setCamera(BuildContext context) async {
    if (widget.cameras.isEmpty) return;
    final selected = this.selected;

    // stop all.
    for (var i = 0; i < controllers.length; i++) {
      final controller = controllers[i];
      if (controller != null) {
        if (context.mounted) setState(() => controllers[i] = null);
        await controller.stopImageStream();
      }
    }

    // start the one that is selected.
    final controller = CameraController(
      widget.cameras[selected],
      ResolutionPreset.low,
      enableAudio: false,
      fps: 1,
    );
    await controller.initialize();
    await controller.setFocusMode(FocusMode.auto);
    await controller.startImageStream((image) async {
      late final String? newScanData;
      try {
        final pngImage = await utils.convertImagetoPng(image);
        newScanData = await qrReader.findAddressFromBytes(bytes: pngImage);
      } catch (e) {
        newScanData = null; // TODO: Report error.
      }

      if (context.mounted && !popped && newScanData != null) {
        popped = true;
        // Manually invoke `onPop` as we aren't within `PopScope`.
        onPop(context, false, newScanData);
      }
    });
    if (mounted) setState(() => controllers[selected] = controller);
  }

  /// This stops the image stream before popping to stop it interfering with the pop animation.
  void onPop(BuildContext context, bool didPop, String? scanResult) async {
    if (didPop) return;

    for (final (_, controller) in controllers.indexed) {
      try {
        await controller?.stopImageStream();
      } catch (Exception) {
        // ignore exception
      }
      ;
    }

    if (context.mounted) Navigator.pop(context, scanResult);
  }

  @override
  Widget build(BuildContext context) {
    final controller = controllers.elementAtOrNull(selected);

    final cameraPreview = (widget.cameras.isEmpty)
        ? AspectRatio(
            aspectRatio: 1.5,
            child: Center(child: Text('No cameras found.')),
          )
        : ClipRRect(
            borderRadius: BorderRadius.circular(28.0),
            child: (controller == null)
                ? AspectRatio(
                    aspectRatio: 1.5,
                    child: Center(child: CircularProgressIndicator()),
                  )
                : CameraPreview(controller),
          );

    final stack = Stack(
      children: [
        AnimatedSize(
          duration: Durations.medium4,
          curve: Curves.easeInOutCubicEmphasized,
          child: cameraPreview,
        ),
        Positioned(
          top: 12.0,
          left: 12.0,
          child: IconButton.filledTonal(
            onPressed: () => onPop(context, false, null),
            icon: Icon(Icons.close),
          ),
        ),
        Positioned(
          bottom: 12.0,
          right: 12.0,
          child: IconButton.filledTonal(
            onPressed: () {
              incrementSelected();
              setCamera(context);
            },
            icon: Icon(Icons.flip_camera_android),
          ),
        ),
      ],
    );

    return PopScope<String>(
      canPop: false,
      onPopInvokedWithResult: (didPop, scanResult) =>
          onPop(context, didPop, scanResult),
      child: stack,
    );
  }
}
