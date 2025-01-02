import 'dart:io';
import 'package:frostsnapp/contexts.dart';
import 'package:frostsnapp/global.dart';
import 'package:flutter/material.dart';
import 'package:frostsnapp/bridge_definitions.dart';
import 'package:frostsnapp/id_ext.dart';
import 'package:frostsnapp/psbt.dart';
import 'package:frostsnapp/sign_message.dart';
import 'package:frostsnapp/snackbar.dart';
import 'package:frostsnapp/wallet.dart';
import 'package:google_fonts/google_fonts.dart';

class WalletSendPage extends StatelessWidget {
  const WalletSendPage({super.key});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Scaffold(
      appBar: AppBar(
        title: const Text('Send Bitcoin'),
        centerTitle: true,
      ),
      body: WalletSend(
          onBroadcastNewTx: () => Navigator.popUntil(context, (route) {
                return route is MaterialPageRoute &&
                    route.builder(context) is WalletHome;
              })),
      backgroundColor: theme.colorScheme.surfaceContainer,
    );
  }
}

class WalletSend extends StatefulWidget {
  final Function()? onBroadcastNewTx;

  const WalletSend({
    Key? key,
    this.onBroadcastNewTx,
  }) : super(key: key);

  @override
  State<WalletSend> createState() => _WalletSendState();
}

class _WalletSendState extends State<WalletSend> {
  final _formKey = GlobalKey<FormState>();
  String _address = '';
  int _amount = 0;
  double _feerate = 1.0; // Default fee rate
  String _eta = "1 hr";
  Set<DeviceId> selectedDevices = deviceIdSet([]);

  void _updateETA() {
    // todo: get ETA
    setState(() {
      if (_feerate < 5.0) {
        _eta = "2 hrs";
      } else if (_feerate >= 5.0 && _feerate < 10.0) {
        _eta = "1 hr";
      } else {
        _eta = "30 mins";
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    final walletCtx = WalletContext.of(context)!;
    final frostKey = coord.getFrostKey(keyId: walletCtx.keyId)!;
    final accessStructure = frostKey.accessStructures()[0];
    final enoughSelected =
        selectedDevices.length == accessStructure.threshold();

    final Widget signPsbtButton = ElevatedButton(
        onPressed: () {
          Navigator.push(context, MaterialPageRoute(builder: (context) {
            return LoadPsbtPage(
              wallet: walletCtx.superWallet,
            );
          }));
        },
        child: Text(
          "Load PSBT",
        ));

    return CustomScrollView(
      slivers: <Widget>[
        SliverToBoxAdapter(
          child: Container(
            color: Theme.of(context).colorScheme.surface,
            child: UpdatingBalance(txStream: walletCtx.txStream),
          ),
        ),
        SliverPadding(
          padding:
              const EdgeInsets.symmetric(horizontal: 24.0).copyWith(top: 24.0),
          sliver: SliverToBoxAdapter(
            child: Row(
              mainAxisSize: MainAxisSize.max,
              children: const [
                Expanded(child: Text('Create Transaction')),
              ],
            ),
          ),
        ),
        SliverPadding(
          padding: const EdgeInsets.symmetric(horizontal: 24.0),
          sliver: SliverToBoxAdapter(
              child: Column(children: [
            Form(
              key: _formKey,
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: <Widget>[
                  TextFormField(
                    decoration: InputDecoration(labelText: 'Address'),
                    validator: (value) {
                      // Use the provided predicate for address validation
                      return walletCtx.superWallet.network
                          .validateDestinationAddress(address: value ?? '');
                    },
                    onSaved: (value) => _address = value ?? '',
                  ),
                  TextFormField(
                    decoration: InputDecoration(labelText: 'Amount (sats)'),
                    keyboardType:
                        TextInputType.numberWithOptions(decimal: false),
                    validator: (value) {
                      // Convert value to int and use the provided predicate for amount validation
                      final amount = int.tryParse(value ?? '') ?? 0;
                      return walletCtx.superWallet.network
                          .validateAmount(address: _address, value: amount);
                    },
                    onSaved: (value) =>
                        _amount = int.tryParse(value ?? '') ?? 0,
                  ),
                  Row(
                    children: [
                      Expanded(
                        child: TextFormField(
                          decoration: InputDecoration(
                              labelText: 'Fee Rate (sats/vByte)'),
                          keyboardType:
                              TextInputType.numberWithOptions(decimal: true),
                          initialValue: _feerate.toString(),
                          onChanged: (value) {
                            setState(() {
                              _feerate = double.tryParse(value) ?? _feerate;
                              _updateETA();
                            });
                          },
                        ),
                      ),
                      Padding(
                        padding: const EdgeInsets.only(left: 10),
                        child: Text(
                          "ETA $_eta",
                          //style: TextStyle(color: textSecondaryColor),
                        ),
                      ),
                    ],
                  ),
                  Divider(
                    height: 20.0,
                    thickness: 2.0,
                    //color: backgroundSecondaryColor,
                  ),
                  Text(
                    'Select ${accessStructure.threshold()} device${accessStructure.threshold() > 1 ? "s" : ""} to sign with:',
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
                  Padding(
                      padding: const EdgeInsets.symmetric(vertical: 16.0),
                      child: ElevatedButton(
                        onPressed: !enoughSelected
                            ? null
                            : () async {
                                if (_formKey.currentState!.validate()) {
                                  _formKey.currentState!.save();
                                  final unsignedTx = await walletCtx.superWallet
                                      .sendTo(
                                          masterAppkey: walletCtx.masterAppkey,
                                          toAddress: _address,
                                          value: _amount,
                                          feerate: _feerate);
                                  final signingStream = coord.startSigningTx(
                                      accessStructureRef:
                                          accessStructure.accessStructureRef(),
                                      unsignedTx: unsignedTx,
                                      devices: selectedDevices.toList());
                                  if (context.mounted) {
                                    await signAndBroadcastWorkflowDialog(
                                        superWallet: walletCtx.superWallet,
                                        context: context,
                                        signingStream: signingStream,
                                        unsignedTx: unsignedTx,
                                        masterAppkey: walletCtx.masterAppkey,
                                        onBroadcastNewTx:
                                            widget.onBroadcastNewTx);
                                  }
                                }
                              },
                        child: Text('Submit Transaction'),
                      )),
                ],
              ),
            ),
            // const SizedBox(height: 15),
            signPsbtButton,
          ])),
        ),
      ],
    );
  }
}

Future<void> signAndBroadcastWorkflowDialog(
    {required BuildContext context,
    required Stream<SigningState> signingStream,
    required UnsignedTx unsignedTx,
    required SuperWallet superWallet,
    required MasterAppkey masterAppkey,
    Function()? onBroadcastNewTx}) async {
  final effect = unsignedTx.effect(
      masterAppkey: masterAppkey, network: superWallet.network);

  final signatures = await showSigningProgressDialog(
    context,
    signingStream,
    describeEffect(context, effect),
  );
  if (signatures != null) {
    final signedTx = await unsignedTx.complete(signatures: signatures);
    if (context.mounted) {
      final wasBroadcast = await showBroadcastConfirmDialog(context,
          masterAppkey: masterAppkey, tx: signedTx, superWallet: superWallet);
      if (wasBroadcast) {
        onBroadcastNewTx?.call();
      }
    }
  }
}

class EffectTable extends StatelessWidget {
  final EffectOfTx effect;
  const EffectTable({super.key, required this.effect});

  @override
  Widget build(BuildContext context) {
    List<TableRow> transactionRows =
        effect.foreignReceivingAddresses.map((entry) {
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
                  : Text("unknown")),
          Padding(
              padding: const EdgeInsets.all(8.0),
              child: SatoshiText.withSign(value: -effect.fee)),
        ],
      ),
    );

    transactionRows.add(
      TableRow(
        children: [
          Padding(
            padding: const EdgeInsets.all(8.0),
            child: Text('Net value'),
          ),
          Padding(
            padding: const EdgeInsets.all(8.0),
            child: SatoshiText.withSign(value: effect.netValue),
          ),
        ],
      ),
    );

    final effectTable = Table(
      columnWidths: const {
        0: FlexColumnWidth(4),
        1: FlexColumnWidth(2),
      },
      border: TableBorder.all(),
      children: transactionRows,
    );

    final effectWidget = Column(
      children: [
        describeEffect(context, effect),
        Divider(),
        effectTable,
      ],
    );

    return effectWidget;
  }
}

Widget describeEffect(BuildContext context, EffectOfTx effect) {
  final style =
      DefaultTextStyle.of(context).style.copyWith(fontWeight: FontWeight.w600);
  final Widget description;

  if (effect.foreignReceivingAddresses.length == 1) {
    final (dest, amount) = effect.foreignReceivingAddresses[0];
    description = Wrap(
      direction: Axis.horizontal,
      children: <Widget>[
        Text('Sending '),
        SatoshiText(value: amount, style: style),
        Text(' to '),
        Text(
          dest,
          style: GoogleFonts.sourceCodePro(textStyle: style),
        )
      ],
    );
  } else if (effect.foreignReceivingAddresses.isEmpty) {
    description = Text("Internal transfer");
  } else {
    description = Text("cannot describe this yet");
  }

  return description;
}

Future<bool> showBroadcastConfirmDialog(BuildContext context,
    {required MasterAppkey masterAppkey,
    required SignedTx tx,
    required SuperWallet superWallet}) async {
  final wasBroadcast = await showDialog<bool>(
      context: context,
      barrierDismissible: false,
      builder: (dialogContext) {
        final effect =
            tx.effect(masterAppkey: masterAppkey, network: superWallet.network);
        final effectWidget = EffectTable(effect: effect);
        return AlertDialog(
            title: Text("Broadcast?"),
            content: SizedBox(
                width: Platform.isAndroid ? double.maxFinite : 400.0,
                child: Align(
                  alignment: Alignment.center,
                  child: effectWidget,
                )),
            actions: [
              ElevatedButton(
                  onPressed: () {
                    if (dialogContext.mounted) {
                      Navigator.pop(dialogContext, false);
                    }
                  },
                  child: Text("Cancel")),
              ElevatedButton(
                  onPressed: () async {
                    try {
                      await superWallet.broadcastTx(
                          masterAppkey: masterAppkey, tx: tx);
                      if (dialogContext.mounted) {
                        Navigator.pop(context, true);
                      }
                    } catch (e) {
                      if (dialogContext.mounted) {
                        Navigator.pop(dialogContext, false);
                        showErrorSnackbarTop(
                            dialogContext, "Broadcast error: $e");
                      }
                    }
                  },
                  child: Text("Broadcast"))
            ]);
      });

  return wasBroadcast ?? false;
}
