import 'package:camera/camera.dart';
import 'package:flutter/material.dart';
import 'package:frostsnapp/image_converter.dart';
import 'package:frostsnapp/src/rust/api/qr.dart';

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

  // Ensure that we only pop once.
  bool popped = false;
  late int selected;
  CameraController? controller;

  @override
  void initState() {
    super.initState();
    selected = widget.initialSelected;
  }

  @override
  void dispose() async {
    controller?.dispose();
    super.dispose();
  }

  incrementSelected() {
    selected += 1;
    if (selected >= widget.cameras.length) {
      selected = 0;
    }
  }

  initCamera(BuildContext context) {
    if (widget.cameras.isEmpty) return;
    controller?.dispose();
    controller?.setFocusMode(FocusMode.auto);
    final newController = CameraController(
      widget.cameras[selected],
      ResolutionPreset.low,
      enableAudio: false,
      fps: 1,
    );
    newController.initialize().then((_) async {
      setState(() => controller = newController);
      final qrReader = QrReader();
      controller?.startImageStream((image) async {
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
    });
  }

  /// This stops the image stream before popping to stop it interfering with the pop animation.
  void onPop(BuildContext context, bool didPop, String? scanResult) async {
    if (didPop) return;
    await (controller?.stopImageStream() ?? Future.value()).then(
      (_) => (context.mounted) ? Navigator.pop(context, scanResult) : null,
    );
  }

  @override
  Widget build(BuildContext context) {
    if (controller == null) {
      initCamera(context);
    }

    final cameraPreview =
        (widget.cameras.isEmpty)
            ? AspectRatio(
              aspectRatio: 1,
              child: Center(child: Text('No cameras found.')),
            )
            : (controller == null)
            ? AspectRatio(
              aspectRatio: 1,
              child: Center(child: CircularProgressIndicator()),
            )
            : ClipRRect(
              borderRadius: BorderRadius.circular(28.0),
              child: CameraPreview(controller!),
            );

    final stack = Stack(
      children: [
        AnimatedSwitcher(
          duration: Durations.long1,
          switchInCurve: Curves.easeInOutCubicEmphasized,
          transitionBuilder: (Widget child, Animation<double> animation) {
            return FadeTransition(opacity: animation, child: child);
          },
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
              initCamera(context);
            },
            icon: Icon(Icons.flip_camera_android),
          ),
        ),
      ],
    );

    return PopScope<String>(
      canPop: false,
      onPopInvokedWithResult:
          (didPop, scanResult) => onPop(context, didPop, scanResult),
      child: stack,
    );
  }
}
