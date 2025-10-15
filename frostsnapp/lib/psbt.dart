import 'dart:async';
import 'dart:io';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnap/camera.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/maybe_fullscreen_dialog.dart';
import 'package:frostsnap/sign_message.dart';
import 'package:frostsnap/snackbar.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/qr.dart';
import 'package:frostsnap/src/rust/api/signing.dart';
import 'package:frostsnap/src/rust/api/super_wallet.dart';
import 'package:frostsnap/src/rust/api/bitcoin.dart';
import 'package:frostsnap/theme.dart';
import 'package:frostsnap/wallet.dart';
import 'package:frostsnap/wallet_tx_details.dart';
import 'package:pretty_qr_code/pretty_qr_code.dart';

class LoadPsbtPage extends StatefulWidget {
  final Wallet wallet;

  const LoadPsbtPage({super.key, required this.wallet});

  @override
  LoadPsbtPageState createState() => LoadPsbtPageState();
}

class LoadPsbtPageState extends State<LoadPsbtPage> {
  String? fileContents;
  Set<DeviceId> selectedDevices = deviceIdSet([]);
  SignedTx? signedTx;
  final selectorKey = Key('key-selector');

  Future<bool> tryStartSigningPsbt(
    BuildContext context,
    Uint8List psbtBytes,
  ) async {
    final fsCtx = FrostsnapContext.of(context)!;
    final psbtMan = fsCtx.psbtManager;

    final walletCtx = WalletContext.of(context)!;
    final wallet = widget.wallet;

    final Psbt psbt;
    final UnsignedTx unsignedTx;

    try {
      psbt = Psbt.deserialize(bytes: psbtBytes);
    } catch (e) {
      if (context.mounted)
        showErrorSnackbar(context, "Cannot deserialize PSBT: $e");
      return false;
    }

    try {
      unsignedTx = await wallet.superWallet.psbtToUnsignedTx(
        psbt: psbt,
        masterAppkey: wallet.masterAppkey,
      );
    } catch (e) {
      if (context.mounted)
        showErrorSnackbar(context, "Cannot extract tx from PSBT: $e");
      return false;
    }

    final txDetails = TxDetailsModel(
      tx: unsignedTx.details(
        superWallet: wallet.superWallet,
        masterAppkey: wallet.masterAppkey,
      ),
      chainTipHeight: wallet.superWallet.height(),
      now: DateTime.now(),
    );

    if (context.mounted) {
      Navigator.popUntil(context, (r) => r.isFirst);
      await showBottomSheetOrDialog(
        context,
        title: Text('Transaction Details'),
        builder: (context, scrollController) => walletCtx.wrap(
          TxDetailsPage.startSigning(
            txStates: walletCtx.txStream,
            txDetails: txDetails,
            accessStructureRef: wallet
                .frostKey()!
                .accessStructures()[0]
                .accessStructureRef(),
            unsignedTx: unsignedTx,
            devices: selectedDevices.toList(),
            psbtMan: psbtMan,
            psbt: psbt,
          ),
        ),
      );
    }

    return true;
  }

  @override
  Widget build(BuildContext context) {
    final frostKey = widget.wallet.frostKey()!;
    final accessStructure = frostKey.accessStructures()[0];
    final enoughSelected =
        selectedDevices.length == accessStructure.threshold();
    Widget? scanPsbtButton;
    if (Platform.isAndroid || Platform.isIOS) {
      scanPsbtButton = TextButton.icon(
        onPressed: !enoughSelected
            ? null
            : () async {
                if (context.mounted) {
                  final psbtBytes = await MaybeFullscreenDialog.show<Uint8List>(
                    context: context,
                    child: PsbtCameraReader(),
                  );
                  if (psbtBytes == null) return;
                  if (!context.mounted) return;
                  final ok = await tryStartSigningPsbt(context, psbtBytes);
                  if (context.mounted && !ok) Navigator.pop(context);
                }
              },
        label: Text("Scan"),
        icon: Icon(Icons.qr_code_scanner),
      );
    } else {
      scanPsbtButton = null;
    }
    // ...

    final loadPsbtFileButton = TextButton.icon(
      onPressed: !enoughSelected
          ? null
          : () async {
              FilePickerResult? fileResult = await FilePicker.platform
                  .pickFiles();
              if (fileResult == null) return;

              File file = File(fileResult.files.single.path!);
              Uint8List psbtBytes = await file.readAsBytes();

              if (!context.mounted) return;
              final ok = await tryStartSigningPsbt(context, psbtBytes);
              if (context.mounted && !ok) Navigator.pop(context);
            },
      label: Text("Open file"),
      icon: Icon(Icons.file_open),
    );

    final column = Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      spacing: 16,
      children: [
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16),
          child: Text(
            'Select ${accessStructure.threshold()} device${accessStructure.threshold() > 1 ? "s" : ""} to sign with.',
          ),
        ),
        SigningDeviceSelector(
          key: selectorKey,
          initialSet: selectedDevices,
          frostKey: frostKey,
          onChanged: (selected) {
            setState(() {
              selectedDevices = selected;
            });
          },
        ),
      ],
    );

    final scrollView = CustomScrollView(
      physics: ClampingScrollPhysics(),
      shrinkWrap: true,
      slivers: [
        TopBarSliver(
          title: Text('Sign PSBT'),
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

    return SafeArea(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          WindowSizeContext.of(context) == WindowSizeClass.compact
              ? Expanded(child: scrollView)
              : scrollView,
          Divider(height: 1),
          Padding(
            padding: const EdgeInsets.all(16.0),
            child: Row(
              mainAxisAlignment: MainAxisAlignment.end,
              spacing: 8,
              children: [
                if (scanPsbtButton != null) scanPsbtButton,
                loadPsbtFileButton,
              ],
            ),
          ),
        ],
      ),
    );
  }
}

Future<void> saveOrBroadcastSignedPsbtDialog(
  BuildContext context, {
  required Wallet wallet,
  required SignedTx tx,
  required Psbt psbt,
}) {
  return showDialog(
    context: context,
    builder: (context) {
      final broadcastButton = ElevatedButton(
        onPressed: () async {
          final broadcasted = await showBroadcastConfirmDialog(
            context,
            masterAppkey: wallet.masterAppkey,
            tx: tx,
            superWallet: wallet.superWallet,
          );
          if (broadcasted && context.mounted) {
            ScaffoldMessenger.of(
              context,
            ).showSnackBar(SnackBar(content: Text('Broadcasted transaction!')));
          }
        },
        child: Text("Broadcast"),
      );

      final showQr = ElevatedButton(
        onPressed: () async {
          await showDialog(
            context: context,
            builder: (BuildContext context) {
              return AnimatedQr(input: psbt.serialize());
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
            final psbtBytes = psbt.serialize();
            await newFile.writeAsBytes(psbtBytes);
          }
        },
        child: Text("Save PSBT"),
      );

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
                    Clipboard.setData(
                      ClipboardData(
                        text: psbt
                            .serialize()
                            .map(
                              (byte) => byte.toRadixString(16).padLeft(2, '0'),
                            )
                            .join(),
                      ),
                    );
                    ScaffoldMessenger.of(context).showSnackBar(
                      SnackBar(
                        content: Text('Error message copied to clipboard!'),
                      ),
                    );
                  },
                  tooltip: 'Copy to Clipboard',
                ),
              ],
            ),
          ),
        ),
        actions: [
          ElevatedButton(
            onPressed: () {
              if (context.mounted) {
                Navigator.pop(context, false);
              }
            },
            child: Text("Close"),
          ),
        ],
      );
    },
  );
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

    final psbtBytes = psbt.serialize();

    // Write the bytes to the selected file
    await file.writeAsBytes(psbtBytes);
  } catch (e) {
    if (context.mounted) {
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text('Error saving PSBT: $e')));
    }
  }
}

class AnimatedQr extends StatefulWidget {
  final Uint8List input;
  const AnimatedQr({super.key, required this.input});

  @override
  State<AnimatedQr> createState() => _AnimatedQrState();
}

class _AnimatedQrState extends State<AnimatedQr> {
  late final QrEncoder qrEncoder;
  String? currentData;

  @override
  void initState() {
    super.initState();
    qrEncoder = QrEncoder(bytes: widget.input);
    run();
  }

  void run() async {
    while (true) {
      final next = await qrEncoder.nextPart();
      if (!mounted) return;
      setState(() => currentData = next);
      await Future.delayed(Duration(milliseconds: 100));
    }
  }

  @override
  Widget build(BuildContext context) {
    final data = currentData;
    if (data == null) return SizedBox.shrink();

    final qrImage = QrImage(
      QrCode.fromData(data: data, errorCorrectLevel: QrErrorCorrectLevel.L),
    );

    return PrettyQrView(qrImage: qrImage);
  }
}

class EffectTable extends StatelessWidget {
  final EffectOfTx effect;
  const EffectTable({super.key, required this.effect});

  @override
  Widget build(BuildContext context) {
    List<TableRow> transactionRows = effect.foreignReceivingAddresses.map((
      entry,
    ) {
      final (address, value) = entry;
      return TableRow(
        children: [
          Padding(
            padding: const EdgeInsets.all(8.0),
            child: Text('Send to $address'),
          ),
          Padding(
            padding: const EdgeInsets.all(8.0),
            child: SatoshiText.withSign(value: -value),
          ),
        ],
      );
    }).toList();

    transactionRows.add(
      TableRow(
        children: [
          Padding(
            padding: const EdgeInsets.all(8.0),
            child: effect.feerate != null
                ? Text("${effect.feerate!.toStringAsFixed(1)} (sats/vb))")
                : Text("unknown"),
          ),
          Padding(
            padding: const EdgeInsets.all(8.0),
            child: SatoshiText.withSign(value: -effect.fee),
          ),
        ],
      ),
    );

    transactionRows.add(
      TableRow(
        children: [
          Padding(padding: const EdgeInsets.all(8.0), child: Text('Net value')),
          Padding(
            padding: const EdgeInsets.all(8.0),
            child: SatoshiText.withSign(value: effect.netValue),
          ),
        ],
      ),
    );

    final effectTable = Table(
      columnWidths: const {0: FlexColumnWidth(4), 1: FlexColumnWidth(2)},
      border: TableBorder.all(),
      children: transactionRows,
    );

    final effectWidget = Column(
      children: [describeEffect(context, effect), Divider(), effectTable],
    );

    return effectWidget;
  }
}

Widget describeEffect(BuildContext context, EffectOfTx effect) {
  final style = DefaultTextStyle.of(
    context,
  ).style.copyWith(fontWeight: FontWeight.w600);
  final Widget description;

  if (effect.foreignReceivingAddresses.length == 1) {
    final (dest, amount) = effect.foreignReceivingAddresses[0];
    description = Wrap(
      direction: Axis.horizontal,
      children: <Widget>[
        Text('Sending '),
        SatoshiText(value: amount, style: style),
        Text(' to '),
        Text(dest, style: style),
      ],
    );
  } else if (effect.foreignReceivingAddresses.isEmpty) {
    description = Text("Internal transfer");
  } else {
    description = Text("cannot describe this yet");
  }

  return description;
}

Future<bool> showBroadcastConfirmDialog(
  BuildContext context, {
  required MasterAppkey masterAppkey,
  required SignedTx tx,
  required SuperWallet superWallet,
}) async {
  final wasBroadcast = await showDialog<bool>(
    context: context,
    barrierDismissible: false,
    builder: (dialogContext) {
      final effect = tx.effect(
        masterAppkey: masterAppkey,
        network: superWallet.network,
      );
      final effectWidget = EffectTable(effect: effect);
      return AlertDialog(
        title: Text("Broadcast?"),
        content: SizedBox(
          width: Platform.isAndroid ? double.maxFinite : 400.0,
          child: Align(alignment: Alignment.center, child: effectWidget),
        ),
        actions: [
          ElevatedButton(
            onPressed: () {
              if (dialogContext.mounted) {
                Navigator.pop(dialogContext, false);
              }
            },
            child: Text("Cancel"),
          ),
          ElevatedButton(
            onPressed: () async {
              try {
                await superWallet.broadcastTx(
                  masterAppkey: masterAppkey,
                  tx: tx.signedTx,
                );
                if (dialogContext.mounted) {
                  Navigator.pop(context, true);
                }
              } catch (e) {
                if (dialogContext.mounted) {
                  Navigator.pop(dialogContext, false);
                  showErrorSnackbar(dialogContext, "Broadcast error: $e");
                }
              }
            },
            child: Text("Broadcast"),
          ),
        ],
      );
    },
  );

  return wasBroadcast ?? false;
}
