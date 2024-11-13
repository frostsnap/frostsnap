import 'dart:async';
import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnapp/id_ext.dart';
import 'package:frostsnapp/global.dart';
import 'package:frostsnapp/icons.dart';
import 'package:frostsnapp/psbt.dart';
import 'package:frostsnapp/settings.dart';
import 'package:frostsnapp/sign_message.dart';
import 'package:frostsnapp/snackbar.dart';
import 'package:frostsnapp/stream_ext.dart';
import 'package:frostsnapp/theme.dart';
import 'package:frostsnapp/address.dart';

import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';

class WalletContext extends InheritedWidget {
  final Wallet wallet;
  final MasterAppkey masterAppkey;
  late final KeyId keyId;
  late final Stream<TxState> txStream;
  // We have a contextual Stream of syncing events (each syncing event is
  // represented as a Stream<double> where the double is the progress).
  final StreamController<Stream<double>> syncs = StreamController.broadcast();

  WalletContext({
    super.key,
    required this.wallet,
    required this.masterAppkey,
    required Widget child,
  }) : super(child: child) {
    keyId = api.masterAppkeyExtToKeyId(masterAppkey: masterAppkey);
    txStream =
        wallet.subTxState(masterAppkey: masterAppkey).toBehaviorSubject();
  }

  static WalletContext? of(BuildContext context) {
    return context.dependOnInheritedWidgetOfExactType<WalletContext>();
  }

  // Allows children to start syncs The reason this is here rather than being
  // called directly by button onPressed for example is so we can trigger it in other ways.
  Stream<double> startFullSync({BuildContext? context}) {
    final progress = wallet
        .sync(masterAppkey: masterAppkey)
        .asBroadcastStream()
        .handleError((error) {
      if (context != null && context.mounted) {
        showErrorSnackbarBottom(context, "sync failed: $error");
      }
    });

    syncs.add(progress.asBroadcastStream());
    return progress;
  }

  Stream<bool> syncStartStopStream() {
    return syncs.stream.asyncExpand((syncStream) async* {
      yield true;
      try {
        // wait for the sync to finish
        await syncStream.toList();
      } catch (e) {
        // do nothing
      }

      yield false;
    });
  }

  @override
  bool updateShouldNotify(WalletContext oldWidget) {
    // never updates
    return false;
  }
}

class WalletPage extends StatelessWidget {
  final Wallet wallet;
  final MasterAppkey masterAppkey;

  const WalletPage(
      {super.key, required this.wallet, required this.masterAppkey});

  @override
  Widget build(BuildContext context) {
    return WalletContext(
        wallet: wallet,
        masterAppkey: masterAppkey,
        child: WalletSyncTrigger(child: WalletHome()));
  }
}

class WalletHome extends StatefulWidget {
  const WalletHome({super.key});

  @override
  State<WalletHome> createState() => _WalletHomeState();
}

class _WalletHomeState extends State<WalletHome> {
  int _selectedIndex = 0; // Tracks the current index for BottomNavigationBar

  @override
  void initState() {
    super.initState();
  }

  // The widget options to display based on the selected index
  // A method that returns the correct widget based on the selected index
  Widget _getSelectedWidget() {
    final walletCtx = WalletContext.of(context)!;
    switch (_selectedIndex) {
      case 0:
        // Pass any required parameters to the WalletActivity widget
        return WalletActivity();
      case 1:
        // Placeholder for the Send page
        return WalletSend(onBroadcastNewTx: () {
          setState(() {
            _selectedIndex = 0;
          });
        });
      case 2:
        return WalletReceive(
            wallet: walletCtx.wallet, masterAppkey: walletCtx.masterAppkey);
      default:
        return Text('Page not found');
    }
  }

  void _onItemTapped(int index) {
    setState(() {
      _selectedIndex = index;
    });
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: FsAppBar(title: Text('Bitcoin Wallet')),
      body: Center(
          // Display the widget based on the current index
          child: _getSelectedWidget()),
      bottomNavigationBar: BottomNavigationBar(
        items: const <BottomNavigationBarItem>[
          BottomNavigationBarItem(icon: Icon(Icons.list), label: 'Activity'),
          BottomNavigationBarItem(icon: Icon(Icons.send), label: 'Send'),
          BottomNavigationBarItem(
              icon: Icon(Icons.account_balance_wallet), label: 'Receive'),
        ],
        currentIndex: _selectedIndex,
        onTap: _onItemTapped,
      ),
    );
  }
}

void copyToClipboard(BuildContext context, String copyText) {
  Clipboard.setData(ClipboardData(text: copyText)).then((_) {
    if (context.mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('Copied to clipboard!')),
      );
    }
  });
}

class WalletActivity extends StatelessWidget {
  const WalletActivity({super.key});

  @override
  Widget build(BuildContext context) {
    final walletContext = WalletContext.of(context)!;
    return Scaffold(
      floatingActionButton: FloatingActionButton(
          child: SpinningSyncIcon(
            spinStream: walletContext.syncStartStopStream(),
          ),
          onPressed: () async {
            walletContext.startFullSync(context: context);
          }),
      body: Stack(children: [
        StreamBuilder(
            stream: walletContext.syncs.stream,
            builder: (context, snap) {
              if (!snap.hasData) {
                return SizedBox();
              }
              // we need make sure we don't use the old FloatingProgress widget for each sync
              UniqueKey floatingProgressKey = UniqueKey();
              return FloatingProgress(
                  key: floatingProgressKey, progressStream: snap.data!);
            }),
        Column(children: const [UpdatingBalance(), Expanded(child: TxList())]),
      ]),
    );
  }
}

class FloatingProgress extends StatefulWidget {
  final Stream<double> progressStream;

  const FloatingProgress({super.key, required this.progressStream});

  @override
  State<FloatingProgress> createState() => _FloatingProgress();
}

class _FloatingProgress extends State<FloatingProgress>
    with SingleTickerProviderStateMixin {
  late AnimationController _progressFadeController;
  double progress = 0.0;

  @override
  initState() {
    super.initState();
    _progressFadeController =
        AnimationController(vsync: this, duration: Duration(seconds: 2));
    widget.progressStream.listen((event) {
      setState(() {
        progress = event;
      });
    }, onDone: () {
      // trigger rebuild to start the animation
      setState(() {
        _progressFadeController.forward();
      });
    });
  }

  @override
  void dispose() {
    _progressFadeController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Positioned(
      top: 0,
      left: 0,
      right: 0,
      child: Container(
        alignment: Alignment.center,
        child: AnimatedOpacity(
          opacity: _progressFadeController.isAnimating ? 0.0 : 1.0,
          duration: _progressFadeController.duration!,
          child: LinearProgressIndicator(
            value: progress,
            backgroundColor: backgroundSecondaryColor,
            valueColor: AlwaysStoppedAnimation<Color>(textColor),
          ),
        ),
      ),
    );
  }
}

class TxList extends StatelessWidget {
  const TxList({super.key});

  @override
  Widget build(BuildContext context) {
    final walletContext = WalletContext.of(context)!;
    return StreamBuilder<TxState>(
      stream: walletContext.txStream,
      builder: (context, snapshot) {
        if (!snapshot.hasData) {
          return Center(child: FsProgressIndicator());
        }
        final transactions = snapshot.data!.txs;
        return RefreshIndicator(
            child: ListView.builder(
              itemCount: transactions.length,
              itemBuilder: (context, index) {
                final transaction = transactions[index];
                String confirmationText;

                if (transaction.confirmationTime != null) {
                  DateTime confirmationDateTime =
                      DateTime.fromMillisecondsSinceEpoch(
                          transaction.confirmationTime!.time * 1000);
                  String formattedTime =
                      '${confirmationDateTime.year}-${confirmationDateTime.month.toString().padLeft(2, '0')}-${confirmationDateTime.day.toString().padLeft(2, '0')} ${confirmationDateTime.hour.toString().padLeft(2, '0')}:${confirmationDateTime.minute.toString().padLeft(2, '0')}';
                  confirmationText =
                      'Confirmation: ${transaction.confirmationTime!.height} ($formattedTime)';
                } else {
                  confirmationText = 'Unconfirmed';
                }

                return Card(
                  child: ListTile(
                    leading: Icon(
                      transaction.netValue > 0
                          ? Icons.arrow_downward
                          : Icons.arrow_upward,
                      color:
                          transaction.netValue > 0 ? successColor : errorColor,
                    ),
                    title: Row(
                      children: <Widget>[
                        Expanded(
                          child: NetValue(transaction.netValue),
                        ),
                        if (transaction.confirmationTime == null)
                          SpinningSyncButton(onPressed: () async {
                            final stream = walletContext.wallet.syncTxids(
                                masterAppkey: walletContext.masterAppkey,
                                txids: [transaction.txid()]);
                            await stream.toList();
                          }),
                        IconButton(
                          icon: Icon(Icons.copy),
                          onPressed: () {
                            Clipboard.setData(
                                ClipboardData(text: transaction.txid()));
                            ScaffoldMessenger.of(context).showSnackBar(
                              SnackBar(
                                content:
                                    Text('Transaction ID copied to clipboard'),
                              ),
                            );
                          },
                        ),
                      ],
                    ),
                    subtitle: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: <Widget>[
                        Text('ID: ${transaction.txid()}'),
                        Text(
                          confirmationText,
                          style: TextStyle(fontSize: 12),
                        ),
                      ],
                    ),
                  ),
                );
              },
            ),
            onRefresh: () async {
              final stream = walletContext.startFullSync(context: context);
              await stream.toList();
            });
      },
    );
  }
}

class WalletReceive extends StatefulWidget {
  final MasterAppkey masterAppkey;
  final Wallet wallet;

  const WalletReceive(
      {super.key, required this.masterAppkey, required this.wallet});

  @override
  State<WalletReceive> createState() => _WalletReceiveState();
}

class _WalletReceiveState extends State<WalletReceive> {
  final GlobalKey<AnimatedListState> _listKey = GlobalKey<AnimatedListState>();
  List<Address> _addresses = [];

  @override
  void initState() {
    super.initState();
    _addresses =
        widget.wallet.addressesState(masterAppkey: widget.masterAppkey);
  }

  Future<Address> _addAddress(BuildContext context) async {
    final nextAddressInfo =
        await widget.wallet.nextAddress(masterAppkey: widget.masterAppkey);
    final Address newAddress = nextAddressInfo;

    if (context.mounted) {
      if (context.mounted) {
        setState(() {
          _addresses.insert(0, newAddress);
          _listKey.currentState?.insertItem(0);
        });
      }
    }
    return nextAddressInfo;
  }

  @override
  Widget build(BuildContext context) {
    final walletCtx = WalletContext.of(context)!;
    final frostKey = coord.getFrostKey(keyId: walletCtx.keyId)!;
    final accessStructureRef =
        frostKey.accessStructures()[0].accessStructureRef();

    return Scaffold(
        body: Padding(
      padding: const EdgeInsets.all(10.0),
      child: Column(children: [
        Padding(
          padding: const EdgeInsets.all(10.0),
          child: ElevatedButton(
            onPressed: () async {
              final address = await _addAddress(context);
              if (context.mounted) {
                Navigator.push(
                  context,
                  MaterialPageRoute(
                    builder: (context) => WalletContext(
                      wallet: walletCtx.wallet,
                      masterAppkey: walletCtx.masterAppkey,
                      child: AddressPage(
                        masterAppkey: walletCtx.masterAppkey,
                        address: address,
                        accessStructureRef: accessStructureRef,
                      ),
                    ),
                  ),
                );
              }
            },
            child: Text('Get New Address'),
          ),
        ),
        Expanded(
          child: AnimatedList(
            key: _listKey,
            initialItemCount: _addresses.length,
            itemBuilder: (context, index, animation) {
              return _buildAddressItem(context, _addresses[index], animation);
            },
          ),
        ),
      ]),
    ));
  }

  Widget _buildAddressItem(
      BuildContext context, Address address, Animation<double> animation) {
    final walletCtx = WalletContext.of(context)!;
    final frostKey = coord.getFrostKey(keyId: walletCtx.keyId)!;
    final accessStructureRef =
        frostKey.accessStructures()[0].accessStructureRef();

    return SizeTransition(
      sizeFactor: animation,
      child: Padding(
        padding: const EdgeInsets.only(
            bottom: 4.0), // Adjust the padding/margin here
        child: Card(
          child: ListTile(
            tileColor: address.used
                ? backgroundSecondaryColor
                : backgroundTertiaryColor, // This changes the background color of the ListTile
            title: Text(
              '${address.index}: ${address.addressString}',
              style: TextStyle(
                fontFamily: 'Courier',
              ),
            ),
            trailing: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                IconButton(
                  icon: Icon(Icons.policy),
                  onPressed: () async {
                    Navigator.push(
                        context,
                        MaterialPageRoute(
                          builder: (context) => WalletContext(
                            wallet: widget.wallet,
                            masterAppkey: widget.masterAppkey,
                            child: AddressPage(
                              masterAppkey: widget.masterAppkey,
                              address: address,
                              accessStructureRef: accessStructureRef,
                            ),
                          ),
                        ));
                  },
                ),
                IconButton(
                  icon: Icon(Icons.copy),
                  onPressed: () async {
                    Clipboard.setData(
                        ClipboardData(text: address.addressString));
                    ScaffoldMessenger.of(context).showSnackBar(
                        SnackBar(content: Text('Address copied to clipboard')));
                  },
                ),
              ],
            ),
          ),
        ),
      ),
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
              wallet: walletCtx.wallet,
              masterAppkey: walletCtx.masterAppkey,
            );
          }));
        },
        child: Text(
          "Load PSBT",
        ));

    return Scaffold(
      body: Container(
          padding: const EdgeInsets.all(16.0),
          child: Column(children: [
            Expanded(
                child: Form(
              key: _formKey,
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: <Widget>[
                  UpdatingBalance(),
                  TextFormField(
                    decoration: InputDecoration(labelText: 'Address'),
                    validator: (value) {
                      // Use the provided predicate for address validation
                      return walletCtx.wallet.network
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
                      return walletCtx.wallet.network
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
                          style: TextStyle(color: textSecondaryColor),
                        ),
                      ),
                    ],
                  ),
                  Divider(
                      height: 20.0,
                      thickness: 2.0,
                      color: backgroundSecondaryColor),
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
                                  final unsignedTx = await walletCtx.wallet
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
                                        wallet: walletCtx.wallet,
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
            )),
            // const SizedBox(height: 15),
            signPsbtButton,
          ])),
    );
  }
}

Future<void> signAndBroadcastWorkflowDialog(
    {required BuildContext context,
    required Stream<SigningState> signingStream,
    required UnsignedTx unsignedTx,
    required Wallet wallet,
    required MasterAppkey masterAppkey,
    Function()? onBroadcastNewTx}) async {
  final effect =
      unsignedTx.effect(masterAppkey: masterAppkey, network: wallet.network);

  final signatures = await showSigningProgressDialog(
    context,
    signingStream,
    describeEffect(effect),
  );
  if (signatures != null) {
    final signedTx = await unsignedTx.complete(signatures: signatures);
    if (context.mounted) {
      final wasBroadcast = await showBroadcastConfirmDialog(context,
          masterAppkey: masterAppkey, tx: signedTx, wallet: wallet);
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
            child: NetValue(-value),
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
              padding: const EdgeInsets.all(8.0), child: NetValue(-effect.fee)),
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
            child: NetValue(effect.netValue),
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
        describeEffect(effect),
        Divider(),
        effectTable,
      ],
    );

    return effectWidget;
  }
}

Widget describeEffect(EffectOfTx effect) {
  final Widget description;

  if (effect.foreignReceivingAddresses.length == 1) {
    final (dest, amount) = effect.foreignReceivingAddresses[0];
    description = RichText(
      text: TextSpan(
        style: TextStyle(color: textColor, fontSize: 16), // Default text style
        children: <TextSpan>[
          TextSpan(text: 'Sending '),
          TextSpan(
              text: formatSatoshi(amount),
              style: TextStyle(fontWeight: FontWeight.bold)),
          TextSpan(text: ' to '),
          TextSpan(text: dest, style: TextStyle(fontWeight: FontWeight.bold)),
        ],
      ),
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
    required Wallet wallet}) async {
  final wasBroadcast = await showDialog<bool>(
      context: context,
      barrierDismissible: false,
      builder: (dialogContext) {
        final effect =
            tx.effect(masterAppkey: masterAppkey, network: wallet.network);
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
                      await wallet.broadcastTx(
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

class UpdatingBalance extends StatelessWidget {
  const UpdatingBalance({Key? key}) : super(key: key);

  @override
  Widget build(BuildContext context) {
    final walletContext = WalletContext.of(context)!;
    return StreamBuilder<TxState>(
      stream: walletContext.txStream,
      builder: (context, snapshot) {
        if (snapshot.hasError) {
          return Text('Error: ${snapshot.error}');
        }
        final transactions = snapshot.data?.txs ?? [];
        int balance = transactions.fold(0, (sum, tx) => sum + tx.netValue);
        final balanceInBTC = formatSatoshi(balance);
        return Padding(
          padding: const EdgeInsets.all(20.0),
          child: Text(
            balanceInBTC,
            style: TextStyle(fontSize: 36, fontWeight: FontWeight.bold),
          ),
        );
      },
    );
  }
}

class NetValue extends StatelessWidget {
  final int netValue;
  const NetValue(this.netValue, {super.key});

  @override
  Widget build(BuildContext context) {
    return Text(
      formatSatoshi(netValue), // Display net value in BTC
      style: TextStyle(
        color: netValue > 0 ? successColor : errorColor,
      ),
    );
  }
}

String formatSatoshi(int satoshis) {
  // Convert satoshis to BTC as a double
  double btcAmount = satoshis / 100000000.0;

  // Convert to string with 8 decimal places
  String btcString = btcAmount.toStringAsFixed(8);

  // Split the string into two parts: before and after the decimal
  var parts = btcString.split('.');

  // Format the fractional part into segments
  String fractionalPart =
      "${parts[1].substring(0, 2)} ${parts[1].substring(2, 5)} ${parts[1].substring(5)}";

  // Combine the whole number part with the formatted fractional part
  return '${parts[0]}.$fractionalPart\u20BF';
}

class WalletSyncTrigger extends StatefulWidget {
  final Widget child;
  final bool enabled;
  const WalletSyncTrigger(
      {super.key, required this.child, this.enabled = true});

  @override
  State<WalletSyncTrigger> createState() => _WalletSyncTrigger();
}

class _WalletSyncTrigger extends State<WalletSyncTrigger> {
  bool hasTriggered = false;

  // WalletContext not available in initState() so we use this
  @override
  void didChangeDependencies() {
    super.didChangeDependencies();

    if (!widget.enabled) {
      return;
    }

    if (hasTriggered) {
      return;
    }

    var walletCtx = WalletContext.of(context);
    if (walletCtx == null) {
      return;
    }

    hasTriggered = true;

    // we want to trigger it
    Future.microtask(() {
      if (context.mounted) {
        walletCtx.startFullSync();
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    return widget.child;
  }
}
