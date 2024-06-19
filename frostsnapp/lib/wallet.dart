import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnapp/device_action.dart';
import 'package:frostsnapp/device_id_ext.dart';
import 'package:frostsnapp/global.dart';
import 'package:frostsnapp/psbt.dart';
import 'package:frostsnapp/sign_message.dart';
import 'package:frostsnapp/stream_ext.dart';

import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';

class WalletHome extends StatefulWidget {
  final KeyId keyId;

  const WalletHome({super.key, required this.keyId});

  @override
  _WalletHomeState createState() => _WalletHomeState();
}

class _WalletHomeState extends State<WalletHome> {
  int _selectedIndex = 0; // Tracks the current index for BottomNavigationBar
  late Stream<TxState> txStream;

  @override
  void initState() {
    super.initState();
    txStream = wallet.subTxState(keyId: widget.keyId).toBehaviorSubject();
  }

  // The widget options to display based on the selected index
  // A method that returns the correct widget based on the selected index
  Widget _getSelectedWidget() {
    switch (_selectedIndex) {
      case 0:
        // Pass any required parameters to the WalletActivity widget
        return WalletActivity(keyId: widget.keyId, txStream: txStream);
      case 1:
        // Placeholder for the Send page
        return WalletSend(
            keyId: widget.keyId,
            txStream: txStream,
            onBroadcastNewTx: () {
              setState(() {
                _selectedIndex = 0;
              });
            });
      case 2:
        // Placeholder for the Receive page
        return WalletReceive(keyId: widget.keyId);
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
      appBar: AppBar(
        title: Text('Bitcoin Wallet'),
      ),
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

// Renaming WalletHomePage to WalletActivity
class WalletActivity extends StatefulWidget {
  final KeyId keyId;
  final Stream<TxState> txStream;

  const WalletActivity(
      {super.key, required this.keyId, required this.txStream});

  @override
  State<WalletActivity> createState() => _WalletActivity();
}

class _WalletActivity extends State<WalletActivity> {
  Stream<double>? syncProgressStream;

  // we need this so that flutter actually rebuilds the progress bar when you
  // click the button again.
  UniqueKey floatingProgressKey = UniqueKey();

  @override
  void initState() {
    super.initState();
  }

  @override
  Widget build(BuildContext context) {
    // Your existing WalletActivity implementation
    return Scaffold(
      floatingActionButton: SpinningSyncButton(onPressed: () async {
        final progressStream =
            wallet.sync(keyId: widget.keyId).asBroadcastStream();
        setState(() {
          syncProgressStream = progressStream;
          floatingProgressKey = UniqueKey();
        });
        // this waits until stream is done so the button will keep spinning.
        await progressStream.toList();
      }),
      body: Stack(children: [
        if (syncProgressStream != null)
          FloatingProgress(
              key: floatingProgressKey, progressStream: syncProgressStream!),
        Column(children: [
          Balance(txStream: widget.txStream),
          Expanded(
              child: TxList(keyId: widget.keyId, txStream: widget.txStream))
        ]),
      ]),
    );
  }
}

class SpinningSyncButton extends StatefulWidget {
  final SpinningOnPressed onPressed;
  const SpinningSyncButton({super.key, required this.onPressed});
  @override
  State<SpinningSyncButton> createState() => _SpinningSyncButton();
}

typedef SpinningOnPressed = Future Function();

class _SpinningSyncButton extends State<SpinningSyncButton>
    with SingleTickerProviderStateMixin {
  late AnimationController _animationController;

  @override
  void initState() {
    super.initState();
    _animationController = AnimationController(
      duration: const Duration(seconds: 2),
      vsync: this,
    );
  }

  @override
  void dispose() {
    _animationController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return FloatingActionButton(
      onPressed: () async {
        if (_animationController.isAnimating) {
          return;
        }
        final finishedWhen = widget.onPressed();
        _animationController.repeat();
        await finishedWhen;
        if (mounted) {
          _animationController.reset();
          _animationController.stop();
        }
      },
      child: RotationTransition(
        turns: _animationController,
        child: Icon(Icons.sync),
      ),
    );
  }
}

class SpinningSyncIcon extends StatefulWidget {
  final SpinningOnPressed onPressed;
  const SpinningSyncIcon({super.key, required this.onPressed});
  @override
  State<SpinningSyncIcon> createState() => _SpinningSyncIcon();
}

class _SpinningSyncIcon extends State<SpinningSyncIcon>
    with SingleTickerProviderStateMixin {
  late AnimationController _animationController;

  @override
  void initState() {
    super.initState();
    _animationController = AnimationController(
      duration: const Duration(seconds: 2),
      vsync: this,
    );
  }

  @override
  void dispose() {
    _animationController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return IconButton(
      onPressed: () async {
        if (_animationController.isAnimating) {
          return;
        }
        final finishedWhen = widget.onPressed();
        _animationController.repeat();
        await finishedWhen;
        if (mounted) {
          _animationController.reset();
          _animationController.stop();
        }
      },
      icon: RotationTransition(
        turns: _animationController,
        child: Icon(Icons.sync),
      ),
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
    }).onDone(() {
      _progressFadeController.forward();
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
            backgroundColor: Colors.grey[200],
            valueColor: AlwaysStoppedAnimation<Color>(Colors.blue),
          ),
        ),
      ),
    );
  }
}

class TxList extends StatelessWidget {
  final KeyId keyId;
  final Stream<TxState> txStream;
  const TxList({super.key, required this.keyId, required this.txStream});

  @override
  Widget build(BuildContext context) {
    return StreamBuilder<TxState>(
      stream: txStream,
      builder: (context, snapshot) {
        if (!snapshot.hasData) {
          return Center(child: CircularProgressIndicator());
        }
        final transactions = snapshot.data!.txs;
        return ListView.builder(
          itemCount: transactions.length,
          itemBuilder: (context, index) {
            final transaction = transactions[index];
            String confirmationText;

            if (transaction.confirmationTime != null) {
              // Convert the Unix timestamp to a DateTime object
              DateTime confirmationDateTime =
                  DateTime.fromMillisecondsSinceEpoch(
                      transaction.confirmationTime!.time * 1000);
              // Format the DateTime object to a string
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
                  color: transaction.netValue > 0 ? Colors.green : Colors.red,
                ),
                title: Row(
                  children: <Widget>[
                    Expanded(
                      child: NetValue(transaction.netValue),
                    ),
                    if (transaction.confirmationTime == null)
                      SpinningSyncIcon(onPressed: () async {
                        final stream = wallet.syncTxids(
                            keyId: keyId, txids: [transaction.txid()]);
                        // wait till it's over
                        await stream.toList();
                      }),
                    IconButton(
                      icon: Icon(Icons.copy),
                      onPressed: () {
                        Clipboard.setData(
                            ClipboardData(text: transaction.txid()));
                        ScaffoldMessenger.of(context).showSnackBar(
                          SnackBar(
                            content: Text('Transaction ID copied to clipboard'),
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
                      confirmationText, // Display confirmation height and time or 'Unconfirmed'
                      style:
                          TextStyle(fontSize: 12), // Adjust the style as needed
                    ),
                  ],
                ),
              ),
            );
          },
        );
      },
    );
  }
}

class WalletReceive extends StatefulWidget {
  final KeyId keyId;

  const WalletReceive({super.key, required this.keyId});

  @override
  _WalletReceiveState createState() => _WalletReceiveState();
}

class _WalletReceiveState extends State<WalletReceive> {
  final GlobalKey<AnimatedListState> _listKey = GlobalKey<AnimatedListState>();
  List<Address> _addresses = [];

  @override
  void initState() {
    super.initState();
    _addresses = wallet.addressesState(keyId: widget.keyId);
  }

  void _addAddress() async {
    Address newAddress = await wallet.nextAddress(keyId: widget.keyId);
    _addresses.insert(0, newAddress);
    _listKey.currentState?.insertItem(0);
  }

  @override
  Widget build(BuildContext context) {
    final keyXpub = bitcoinContext.descriptorForKey(keyId: widget.keyId);
    return Scaffold(
        body: Padding(
      padding: const EdgeInsets.all(10.0),
      child: Column(children: [
        Padding(
          padding: const EdgeInsets.all(10.0),
          child: ElevatedButton(
            onPressed: _addAddress,
            child: Text('Get New Address'),
          ),
        ),
        Expanded(
          child: AnimatedList(
            key: _listKey,
            initialItemCount: _addresses.length,
            itemBuilder: (context, index, animation) {
              return _buildAddressItem(_addresses[index], animation);
            },
          ),
        ),
        Text(
          "Wallet Descriptor",
          textAlign: TextAlign.left,
          style: TextStyle(fontSize: 20.0),
        ),
        ListTile(
          title: Text(
            keyXpub,
            style: TextStyle(fontFamily: 'Monospace'),
          ),
          trailing: IconButton(
            icon: Icon(Icons.copy),
            onPressed: () {
              Clipboard.setData(ClipboardData(text: keyXpub));
              ScaffoldMessenger.of(context).showSnackBar(
                  SnackBar(content: Text('Descriptor copied to clipboard!')));
            },
          ),
        ),
        const SizedBox(width: 15),
      ]),
    ));
  }

  Widget _buildAddressItem(Address address, Animation<double> animation) {
    return SizeTransition(
      sizeFactor: animation,
      child: Card(
        color: address.used ? Colors.grey.shade300 : Colors.white,
        child: ListTile(
          title: Text(
            '${address.index}: ${address.addressString}',
            style: TextStyle(fontFamily: 'Monospace'),
          ),
          trailing: IconButton(
            icon: Icon(Icons.copy),
            onPressed: () {
              Clipboard.setData(ClipboardData(text: address.addressString));
              ScaffoldMessenger.of(context).showSnackBar(
                  SnackBar(content: Text('Address copied to clipboard')));
            },
          ),
        ),
      ),
    );
  }
}

class WalletSend extends StatefulWidget {
  final Stream<TxState> txStream;
  final KeyId keyId;
  final Function()? onBroadcastNewTx;

  const WalletSend({
    Key? key,
    required this.txStream,
    required this.keyId,
    this.onBroadcastNewTx,
  }) : super(key: key);

  @override
  _WalletSendState createState() => _WalletSendState();
}

class _WalletSendState extends State<WalletSend> {
  final _formKey = GlobalKey<FormState>();
  String _address = '';
  int _amount = 0;
  double _feerate = 1.0; // Default fee rate
  String _eta = "1 hr";
  Set<DeviceId> selectedDevices = deviceIdSet([]);

  void _updateETA() {
    // TODO: get ETA
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
    final frostKey = coord.getKey(keyId: widget.keyId)!;
    final enoughSelected = selectedDevices.length == frostKey.threshold();

    final Widget signPsbtButton = ElevatedButton(
        onPressed: () {
          Navigator.push(context, MaterialPageRoute(builder: (context) {
            return LoadPsbtPage(
              keyId: widget.keyId,
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
                  Balance(txStream: widget.txStream),
                  TextFormField(
                    decoration: InputDecoration(labelText: 'Address'),
                    validator: (value) {
                      // Use the provided predicate for address validation
                      return bitcoinContext.validateDestinationAddress(
                          address: value ?? '');
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
                      return bitcoinContext.validateAmount(
                          address: _address, value: amount);
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
                          style: TextStyle(color: Colors.grey),
                        ),
                      ),
                    ],
                  ),
                  Divider(height: 20.0, thickness: 2.0, color: Colors.grey),
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
                  Padding(
                      padding: const EdgeInsets.symmetric(vertical: 16.0),
                      child: ElevatedButton(
                        onPressed: !enoughSelected
                            ? null
                            : () async {
                                if (_formKey.currentState!.validate()) {
                                  _formKey.currentState!.save();
                                  final unsignedTx = await wallet.sendTo(
                                      keyId: widget.keyId,
                                      toAddress: _address,
                                      value: _amount,
                                      feerate: _feerate);
                                  final signingStream = coord.startSigningTx(
                                      keyId: widget.keyId,
                                      unsignedTx: unsignedTx,
                                      devices: selectedDevices.toList());
                                  if (context.mounted) {
                                    await signAndBroadcastWorkflowDialog(
                                        context: context,
                                        signingStream: signingStream,
                                        unsignedTx: unsignedTx,
                                        keyId: widget.keyId,
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
    required KeyId keyId,
    Function()? onBroadcastNewTx}) async {
  final effect =
      unsignedTx.effect(keyId: keyId, network: bitcoinContext.network);

  final signatures = await showSigningProgressDialog(
    context,
    signingStream,
    describeEffect(effect),
  );
  if (signatures != null) {
    final signedTx = await unsignedTx.complete(signatures: signatures);
    if (context.mounted) {
      final wasBroadcast =
          await showBroadcastConfirmDialog(context, keyId, signedTx);
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
        style:
            TextStyle(color: Colors.black, fontSize: 16), // Default text style
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
    description = Text("can't describe this yet");
  }

  return description;
}

Future<bool> showBroadcastConfirmDialog(
    BuildContext context, KeyId keyId, SignedTx tx) async {
  final wasBroadcast = await showDialog<bool>(
      context: context,
      barrierDismissible: false,
      builder: (dialogContext) {
        final effect = tx.effect(keyId: keyId, network: bitcoinContext.network);
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
                      await wallet.broadcastTx(keyId: keyId, tx: tx);
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
                  child: Text("Broadcast"))
            ]);
      });

  return wasBroadcast ?? false;
}

class Balance extends StatelessWidget {
  final Stream<TxState> txStream;

  const Balance({Key? key, required this.txStream}) : super(key: key);

  @override
  Widget build(BuildContext context) {
    return StreamBuilder<TxState>(
      stream: txStream,
      builder: (context, snapshot) {
        if (snapshot.hasError) {
          return Text('Error: ${snapshot.error}');
        }
        final transactions = snapshot.data?.txs ?? [];
        int balance = transactions.fold(0, (sum, tx) => sum + tx.netValue);
        // Assuming the balance is in satoshis, convert it to BTC for display
        final balanceInBTC = formatSatoshi(balance);
        return Padding(
          padding: const EdgeInsets.all(20.0),
          child: Text(
            balanceInBTC, // Unicode for Bitcoin symbol
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
        color: netValue > 0 ? Colors.green : Colors.red,
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
