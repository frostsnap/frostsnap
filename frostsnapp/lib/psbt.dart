import 'dart:async';
import 'dart:io';

import 'package:camera/camera.dart';
import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnapp/camera.dart';
import 'package:frostsnapp/id_ext.dart';
import 'package:frostsnapp/global.dart';
import 'package:frostsnapp/settings.dart';
import 'package:frostsnapp/sign_message.dart';
import 'package:frostsnapp/snackbar.dart';
import 'package:frostsnapp/wallet_send.dart';
import 'package:pretty_qr_code/pretty_qr_code.dart';
import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';

class LoadPsbtPage extends StatefulWidget {
  final MasterAppkey masterAppkey;
  final Wallet wallet;
  late final FrostKey frostKey;

  LoadPsbtPage({Key? key, required this.masterAppkey, required this.wallet})
      : super(key: key) {
    frostKey = coord.getFrostKey(
        keyId: api.masterAppkeyExtToKeyId(masterAppkey: masterAppkey))!;
  }

  @override
  LoadPsbtPageState createState() => LoadPsbtPageState();
}

class LoadPsbtPageState extends State<LoadPsbtPage> {
  String? fileContents;
  Set<DeviceId> selectedDevices = deviceIdSet([]);
  SignedTx? signedTx;

  @override
  Widget build(BuildContext context) {
    final accessStructure = widget.frostKey.accessStructures()[0];
    final enoughSelected =
        selectedDevices.length == accessStructure.threshold();
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
                        final psbtBytes = await Navigator.push(context,
                            MaterialPageRoute(builder: (context) {
                          return PsbtCameraReader(
                            cameras: cameras,
                          );
                        }));
                        if (context.mounted) {
                          await runPsbtSigningWorkflow(context,
                              psbtBytes: psbtBytes,
                              selectedDevices: selectedDevices.toList(),
                              accessStructureRef:
                                  accessStructure.accessStructureRef(),
                              wallet: widget.wallet);
                        }
                        if (context.mounted) {
                          Navigator.pop(context);
                        }
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
                    if (context.mounted) {
                      await runPsbtSigningWorkflow(context,
                          wallet: widget.wallet,
                          psbtBytes: psbtBytes,
                          selectedDevices: selectedDevices.toList(),
                          accessStructureRef:
                              accessStructure.accessStructureRef());
                    }
                  } else {
                    // User canceled the file picker
                  }
                },
          child: Text("Open File 📂"),
        ));

    return Scaffold(
      appBar: FsAppBar(title: const Text('Sign PSBT')),
      body: Padding(
        padding: EdgeInsets.all(8.0),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.center,
          children: [
            Text(
              'Select ${accessStructure.threshold()} device${accessStructure.threshold() > 1 ? "s" : ""} to sign with:',
              textAlign: TextAlign.center,
              style: TextStyle(fontSize: 20.0),
            ),
            SigningDeviceSelector(
                frostKey: widget.frostKey,
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

Future<void> runPsbtSigningWorkflow(
  BuildContext context, {
  required Uint8List psbtBytes,
  required List<DeviceId> selectedDevices,
  required AccessStructureRef accessStructureRef,
  required Wallet wallet,
}) async {
  final Psbt psbt;
  final UnsignedTx unsignedTx;
  final frostkey = coord.getFrostKey(keyId: accessStructureRef.keyId)!;
  final masterAppkey = frostkey.masterAppkey();
  try {
    psbt = api.psbtBytesToPsbt(psbtBytes: psbtBytes);
    unsignedTx =
        wallet.psbtToUnsignedTx(psbt: psbt, masterAppkey: masterAppkey);
  } catch (e) {
    showErrorSnackbarTop(context, "Error loading PSBT: $e");
    return;
  }

  final signingStream = coord.startSigningTx(
      accessStructureRef: accessStructureRef,
      unsignedTx: unsignedTx,
      devices: selectedDevices);

  final effect =
      unsignedTx.effect(masterAppkey: masterAppkey, network: wallet.network);

  final signatures = await showSigningProgressDialog(
    context,
    signingStream,
    describeEffect(context, effect),
  );
  if (signatures != null) {
    final signedPsbt = await unsignedTx.attachSignaturesToPsbt(
        signatures: signatures, psbt: psbt);
    final signedTx = await unsignedTx.complete(signatures: signatures);

    if (context.mounted) {
      await saveOrBroadcastSignedPsbtDialog(
        context,
        masterAppkey: masterAppkey,
        tx: signedTx,
        psbt: signedPsbt,
        wallet: wallet,
      );
    }
  }
}

Future<void> saveOrBroadcastSignedPsbtDialog(BuildContext context,
    {required MasterAppkey masterAppkey,
    required SignedTx tx,
    required Psbt psbt,
    required Wallet wallet}) {
  return showDialog(
      context: context,
      builder: (context) {
        final broadcastButton = ElevatedButton(
            onPressed: () async {
              final broadcasted = await showBroadcastConfirmDialog(context,
                  masterAppkey: masterAppkey, tx: tx, wallet: wallet);
              if (broadcasted && context.mounted) {
                ScaffoldMessenger.of(context).showSnackBar(
                  SnackBar(
                    content: Text('Broadcasted transaction!'),
                  ),
                );
              }
            },
            child: Text("Broadcast"));

        final showQr = ElevatedButton(
          onPressed: () async {
            await showDialog(
              context: context,
              builder: (BuildContext context) {
                return AnimatedQr(input: psbt.toBytes());
              },
            );
          },
          child: Text("Show QR"),
        );

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
            content: SizedBox(
                width: Platform.isAndroid ? double.maxFinite : 400.0,
                child: Align(
                    alignment: Alignment.center,
                    child: Column(
                      children: [
                        broadcastButton,
                        SizedBox(height: 20),
                        if (!Platform.isAndroid) ...[
                          saveToFileButton,
                          SizedBox(height: 20),
                        ],
                        SizedBox(height: 20),
                        showQr,
                        SizedBox(height: 20),
                        IconButton(
                          icon: Icon(Icons.content_copy),
                          onPressed: () {
                            Clipboard.setData(ClipboardData(
                                text: psbt
                                    .toBytes()
                                    .map((byte) =>
                                        byte.toRadixString(16).padLeft(2, '0'))
                                    .join()));
                            ScaffoldMessenger.of(context).showSnackBar(
                              SnackBar(
                                content:
                                    Text('Error message copied to clipboard!'),
                              ),
                            );
                          },
                          tooltip: 'Copy to Clipboard',
                        ),
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

Future<void> savePsbt(BuildContext context, Psbt psbt) async {
  try {
    // Pick a file to save the PSBT
    String? outputFile = await FilePicker.platform.saveFile(
      dialogTitle: 'Please select where to save the PSBT file:',
      fileName: "signed.psbt",
    );

    if (outputFile == null) {
      // User canceled the file picker
      return;
    }

    final file = File(outputFile);

    // Convert your PSBT object to bytes (assuming psbt.toBytes() returns Uint8List)
    final psbtBytes = psbt.toBytes();

    // Write the bytes to the selected file
    await file.writeAsBytes(psbtBytes);
  } catch (e) {
    if (context.mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('Error saving PSBT: $e')),
      );
    }
  }
}

class AnimatedQr extends StatefulWidget {
  final Uint8List input;
  const AnimatedQr({Key? key, required this.input}) : super(key: key);

  @override
  State<AnimatedQr> createState() => _AnimatedQrState();
}

class _AnimatedQrState extends State<AnimatedQr> {
  late QrEncoder _qrEncoder;
  String currentQrData = '';

  @override
  void initState() {
    super.initState();
    _initQrEncoder();
  }

  Future<void> _initQrEncoder() async {
    _qrEncoder = await api.newQrEncoder(
      bytes: widget.input,
    );
    _updateQr();
  }

  void _updateQr() {
    if (mounted) {
      setState(() {
        currentQrData = _qrEncoder.next();
      });
      Future.delayed(Duration(milliseconds: 100), _updateQr);
    }
  }

  @override
  Widget build(BuildContext context) {
    final qrCode = QrCode.fromData(
      data: currentQrData,
      errorCorrectLevel: QrErrorCorrectLevel.L,
    );
    final qrImage = QrImage(qrCode);

    return AlertDialog(
      title: Center(child: Text('PSBT')),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          PrettyQrView(
            qrImage: qrImage,
            decoration: const PrettyQrDecoration(
              shape: PrettyQrSmoothSymbol(),
            ),
          ),
        ],
      ),
      actions: [
        IconButton(
          iconSize: 30.0,
          icon: Icon(Icons.close),
          onPressed: () {
            Navigator.of(context).pop();
          },
        )
      ],
    );
  }
}
