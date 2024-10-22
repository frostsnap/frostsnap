import 'dart:math';
import 'dart:typed_data';

import 'package:camera/camera.dart';
import 'package:flutter/material.dart';
import 'package:frostsnapp/device_action.dart';
import 'package:frostsnapp/ffi.dart';
import 'package:frostsnapp/image_converter.dart';
import 'package:frostsnapp/theme.dart';

class PsbtCameraReader extends StatefulWidget {
  const PsbtCameraReader({required this.cameras, super.key});
  final List<CameraDescription> cameras;

  @override
  State<PsbtCameraReader> createState() => _PsbtCameraReaderState();
}

class _PsbtCameraReaderState extends State<PsbtCameraReader> {
  late CameraController controller;
  late Uint8List decodedPsbt;
  late double progress;

  @override
  void initState() {
    super.initState();
    ImageUtils imageUtils = ImageUtils();
    controller = CameraController(widget.cameras[0], ResolutionPreset.low);
    controller.initialize().then((_) async {
      if (!mounted) {
        return;
      }
      setState(() {});

      progress = 0;
      final qrReader = await api.newQrReader();
      var finishedDecoding = false;
      controller.startImageStream((image) async {
        final pngImage = await imageUtils.convertImagetoPng(image);
        final status = await qrReader.decodeFromBytes(bytes: pngImage);
        switch (status) {
          case QrDecoderStatus_Progress(:final field0):
            setState(() {
              if (field0.sequenceCount > 0) {
                progress = min(
                    (field0.decodedFrames.toDouble()) /
                        (field0.sequenceCount.toDouble() * 1.75),
                    0.99);
              } else {
                progress = 0;
              }
            });
          case QrDecoderStatus_Decoded(:final field0):
            if (!finishedDecoding) {
              finishedDecoding = true;
              controller.stopImageStream();
              setState(() {
                progress = 1;
              });

              if (mounted) {
                Navigator.pop(context, field0);
              }
            }
            break;
          case QrDecoderStatus_Failed(:final field0):
            throw Exception(field0);
        }
      });
    }).catchError((Object e) {
      if (mounted) {
        showErrorSnackbar(context, "Error scanning QR: $e");
      }
    });
  }

  @override
  void dispose() {
    controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    if (!controller.value.isInitialized) {
      return Scaffold(
        body: Center(
          child: FsProgressIndicator(),
        ),
      );
    }

    return Scaffold(
        appBar: AppBar(title: const Text('Scan PSBT')),
        body: Padding(
          padding: EdgeInsets.all(8.0),
          child: Column(
            children: [
              CameraPreview(controller),
              Padding(
                  padding: EdgeInsets.all(4.0),
                  child: LinearProgressIndicator(
                    value: progress,
                    backgroundColor: backgroundSecondaryColor,
                    valueColor: AlwaysStoppedAnimation<Color>(awaitingColor),
                    minHeight: 8,
                  )),
              SizedBox(height: 8),
              Text(
                "${(progress * 100).round()}%",
                style: TextStyle(fontSize: 24),
              )
            ],
          ),
        ));
  }
}
