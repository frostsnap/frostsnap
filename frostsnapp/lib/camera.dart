import 'dart:math';
import 'dart:typed_data';

import 'package:camera/camera.dart';
import 'package:flutter/material.dart';
import 'package:frostsnap/image_converter.dart';
import 'package:frostsnap/settings.dart';
import 'package:frostsnap/snackbar.dart';
import 'package:frostsnap/src/rust/api/qr.dart';
import 'package:frostsnap/theme.dart';

class PsbtCameraReader extends StatefulWidget {
  const PsbtCameraReader({required this.cameras, super.key});
  final List<CameraDescription> cameras;

  @override
  State<PsbtCameraReader> createState() => _PsbtCameraReaderState();
}

class _PsbtCameraReaderState extends State<PsbtCameraReader> {
  late CameraController controller;
  late Uint8List decodedPsbt;
  double progress = 0.0;

  @override
  void initState() {
    super.initState();
    ImageUtils imageUtils = ImageUtils();
    controller = CameraController(widget.cameras[0], ResolutionPreset.low);
    controller
        .initialize()
        .then((_) async {
          if (!mounted) {
            return;
          }
          setState(() {});

          progress = 0;
          final qrReader = QrReader();
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
                      0.99,
                    );
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
        })
        .catchError((Object e) {
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
    final preview = controller.value.isInitialized
        ? ClipRRect(
            borderRadius: BorderRadius.circular(28.0),
            child: CameraPreview(controller),
          )
        : AspectRatio(
            aspectRatio: 1.5,
            child: Center(child: CircularProgressIndicator()),
          );

    final column = Column(
      mainAxisSize: MainAxisSize.min,
      spacing: 12,
      children: [
        preview,
        LinearProgressIndicator(value: progress),
        Text("${(progress * 100).round()}%"),
      ],
    );

    final scrollView = CustomScrollView(
      shrinkWrap: true,
      slivers: [
        TopBarSliver(
          title: Text('Scan PSBT'),
          leading: IconButton(
            icon: Icon(Icons.arrow_back),
            onPressed: () => Navigator.pop(context),
          ),
          showClose: false,
        ),

        SliverToBoxAdapter(child: column),
        SliverToBoxAdapter(child: SizedBox(height: 16)),
      ],
    );

    return SafeArea(child: scrollView);
  }
}
