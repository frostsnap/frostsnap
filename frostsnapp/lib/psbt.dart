import 'dart:io';
import 'dart:typed_data';

import 'package:camera/camera.dart';
import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:frostsnapp/camera.dart';
import 'package:frostsnapp/device_action.dart';
import 'package:frostsnapp/device_id_ext.dart';
import 'package:frostsnapp/global.dart';
import 'package:frostsnapp/sign_message.dart';
import 'package:frostsnapp/wallet.dart';
import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';
import "dart:developer" as developer;

class LoadPsbtPage extends StatefulWidget {
  final KeyId keyId;

  const LoadPsbtPage({Key? key, required this.keyId}) : super(key: key);

  @override
  LoadPsbtPageState createState() => LoadPsbtPageState();
}

class LoadPsbtPageState extends State<LoadPsbtPage> {
  String? fileContents;
  Set<DeviceId> selectedDevices = deviceIdSet([]);
  SignedTx? signedTx;

  @override
  Widget build(BuildContext context) {
    final frostKey = coord.getKey(keyId: widget.keyId)!;
    final enoughSelected = selectedDevices.length == frostKey.threshold();
    Widget? scanPsbtButton;

    if (Platform.isAndroid || Platform.isIOS) {
      scanPsbtButton = Padding(
          padding: const EdgeInsets.symmetric(vertical: 5),
          child: ElevatedButton(
              onPressed: !enoughSelected
                  ? null
                  : () async {
                      WidgetsFlutterBinding.ensureInitialized();
                      final cameras = await availableCameras();

                      if (context.mounted) {
                        Navigator.push(context,
                            MaterialPageRoute(builder: (context) {
                          return PsbtCameraReader(
                              cameras: cameras,
                              onPSBTDecoded: (psbtBytes) async {
                                await startSigningPsbt(
                                    context: context,
                                    psbtBytes: psbtBytes,
                                    selectedDevices: selectedDevices.toList(),
                                    keyId: widget.keyId);
                              });
                        }));
                      }
                    },
              child: Text("Scan 📷")));
    } else {
      scanPsbtButton = null;
    }

    final loadPsbtFileButton = Padding(
        padding: const EdgeInsets.symmetric(vertical: 5),
        child: ElevatedButton(
          onPressed: !enoughSelected
              ? null
              : () async {
                  FilePickerResult? fileResult =
                      await FilePicker.platform.pickFiles();
                  if (fileResult != null) {
                    File file = File(fileResult.files.single.path!);
                    Uint8List psbtBytes = await file.readAsBytes();
                    await startSigningPsbt(
                        context: context,
                        psbtBytes: psbtBytes,
                        selectedDevices: selectedDevices.toList(),
                        keyId: widget.keyId);
                  } else {
                    // User canceled the file picker
                  }
                },
          child: Text("Open File 📂"),
        ));

    return Scaffold(
      appBar: AppBar(title: const Text('Sign PSBT')),
      body: Padding(
        padding: EdgeInsets.all(8.0),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.center,
          children: [
            Text(
              'Select ${frostKey.threshold()} device${frostKey.threshold() > 1 ? "s" : ""} to sign with:',
              textAlign: TextAlign.center,
              style: TextStyle(fontSize: 20.0),
            ),
            SigningDeviceSelector(
                frostKey: frostKey,
                onChanged: (selected) {
                  setState(() {
                    selectedDevices = selected;
                  });
                }),
            Text(
              'Load a PSBT:',
            ),
            scanPsbtButton ?? Container(),
            loadPsbtFileButton,
          ],
        ),
      ),
    );
  }
}

Future<void> startSigningPsbt({
  required BuildContext context,
  required Uint8List psbtBytes,
  required List<DeviceId> selectedDevices,
  required KeyId keyId,
}) async {
  final Psbt psbt;
  final unsignedTx;
  try {
    psbt = api.psbtBytesToPsbt(psbtBytes: psbtBytes);
    unsignedTx = wallet.psbtToUnsignedTx(psbt: psbt, keyId: keyId);
  } catch (e) {
    showErrorSnackbar(context, "Error loading PSBT: $e");
    return;
  }

  final signingStream = coord.startSigningTx(
      keyId: keyId, unsignedTx: unsignedTx, devices: selectedDevices);

  if (context.mounted) {
    final effect =
        unsignedTx.effect(keyId: keyId, network: bitcoinContext.network);

    final signatures = await showSigningProgressDialog(
      context,
      signingStream,
      describeEffect(effect),
    );
    if (signatures != null) {
      final signedPsbt = await unsignedTx.attachSignaturesToPsbt(
          signatures: signatures, psbt: psbt);
      final signedTx = await unsignedTx.complete(signatures: signatures);

      if (context.mounted) {
        await saveOrBroadcastSignedPsbtDialog(
          context,
          keyId,
          signedTx,
          signedPsbt,
        );
      }
      if (context.mounted) {
        Navigator.pop(context);
      }
    }
  }
}

Future<void> saveOrBroadcastSignedPsbtDialog(
  BuildContext context,
  KeyId keyId,
  SignedTx tx,
  Psbt psbt,
) {
  return showDialog(
      context: context,
      builder: (context) {
        final broadcastButton = ElevatedButton(
            onPressed: () async {
              final broadcasted =
                  await showBroadcastConfirmDialog(context, keyId, tx);
              if (broadcasted && context.mounted) {
                ScaffoldMessenger.of(context).showSnackBar(
                  SnackBar(
                    content: Text('Broadcasted transaction!'),
                  ),
                );
              }
            },
            child: Text("Broadcast"));
        final saveToFileButton = ElevatedButton(
            onPressed: () async {
              final outputFile = await FilePicker.platform.saveFile(
                dialogTitle: 'Please select where to save the PSBT file:',
                fileName: 'signed.psbt',
              );

              if (outputFile == null) {
                // user canceled the picker
              } else {
                final newFile = File(outputFile);
                final psbtBytes = psbt.toBytes();
                await newFile.writeAsBytes(psbtBytes);
              }
            },
            child: Text("Save PSBT"));

        return AlertDialog(
            title: Text("Signed PSBT"),
            content: Container(
                width: Platform.isAndroid ? double.maxFinite : 400.0,
                child: Align(
                    alignment: Alignment.center,
                    child: Column(
                      children: [
                        broadcastButton,
                        SizedBox(height: 20),
                        saveToFileButton,
                      ],
                    ))),
            actions: [
              ElevatedButton(
                  onPressed: () {
                    if (context.mounted) {
                      Navigator.pop(context, false);
                    }
                  },
                  child: Text("Close"))
            ]);
      });
}
