import 'dart:async';
import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:google_fonts/google_fonts.dart';
import 'package:frostsnapp/id_ext.dart';
import 'package:frostsnapp/global.dart';
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
  final String walletName;
  late final KeyId keyId;
  late final Stream<TxState> txStream;
  // We have a contextual Stream of syncing events (each syncing event is
  // represented as a Stream<double> where the double is the progress).
  final StreamController<Stream<double>> syncs = StreamController.broadcast();

  WalletContext({
    super.key,
    required this.wallet,
    required this.masterAppkey,
    required this.walletName,
    required Widget child,
  }) : super(child: child) {
    keyId = api.masterAppkeyExtToKeyId(masterAppkey: masterAppkey);
    txStream =
        wallet.subTxState(masterAppkey: masterAppkey).toBehaviorSubject();
  }

  static WalletContext? of(BuildContext context) {
    return context.dependOnInheritedWidgetOfExactType<WalletContext>();
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
  final String walletName;

  const WalletPage(
      {super.key,
      required this.wallet,
      required this.masterAppkey,
      required this.walletName});

  @override
  Widget build(BuildContext context) {
    return WalletContext(
        wallet: wallet,
        masterAppkey: masterAppkey,
        walletName: walletName,
        child: WalletHome());
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

  @override
  void dispose() {
    super.dispose();
  }

  // The widget options to display based on the selected index
  // A method that returns the correct widget based on the selected index
  Widget _getSelectedWidget() {
    final walletCtx = WalletContext.of(context)!;
    switch (_selectedIndex) {
      case 0:
        // Pass any required parameters to the WalletActivity widget
        return TxList();
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

  @override
  Widget build(BuildContext context) {
    final walletCtx = WalletContext.of(context)!;
    final theme = Theme.of(context);

    return Scaffold(
      extendBody: true,
      backgroundColor: theme.colorScheme.secondary,
      appBar: FsAppBar(
        title: Text(walletCtx.walletName),
        backgroundColor: theme.colorScheme.surface,
      ),
      body: SafeArea(bottom: true, child: _getSelectedWidget()),
      resizeToAvoidBottomInset: true,
      bottomNavigationBar: NavigationBar(
        indicatorColor: theme.colorScheme.primary,
        onDestinationSelected: (int index) =>
            setState(() => _selectedIndex = index),
        selectedIndex: _selectedIndex,
        destinations: const <Widget>[
          NavigationDestination(icon: Icon(Icons.list), label: 'Activity'),
          NavigationDestination(icon: Icon(Icons.send), label: 'Send'),
          NavigationDestination(
              icon: Icon(Icons.account_balance_wallet), label: 'Receive'),
        ],
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
            valueColor: AlwaysStoppedAnimation<Color>(textPrimaryColor),
          ),
        ),
      ),
    );
  }
}

class TxItem extends StatelessWidget {
  final Transaction transaction;
  static const Map<int, String> monthMap = {
    1: 'Jan',
    2: 'Feb',
    3: 'Mar',
    4: 'Apr',
    5: 'May',
    6: 'Jun',
    7: 'Jul',
    8: 'Aug',
    9: 'Sep',
    10: 'Oct',
    11: 'Nov',
    12: 'Dec',
  };

  const TxItem({super.key, required this.transaction});

  @override
  Widget build(BuildContext context) {
    final walletContext = WalletContext.of(context)!;

    final theme = Theme.of(context);
    final txid = transaction.txid();
    final txidText =
        '${txid.substring(0, 6)}...${txid.substring(txid.length - 6, txid.length)}';

    final blockHeight = walletContext.wallet.height();
    final blockCount = blockHeight + 1;
    final timestamp = transaction.timestamp();

    final confirmations =
        blockCount - (transaction.confirmationTime?.height ?? blockCount);

    final dateTime = (timestamp != null)
        ? DateTime.fromMillisecondsSinceEpoch(timestamp * 1000)
        : DateTime.now();
    final dayText = dateTime.day.toString();
    final monthText = monthMap[dateTime.month]!;
    final yearText = dateTime.year.toString();
    final hourText = dateTime.hour.toString().padLeft(2, '0');
    final minuteText = dateTime.minute.toString().padLeft(2, '0');
    final dateText = '$monthText $dayText, $yearText';
    final timeText = (timestamp != null) ? '$hourText:$minuteText' : '??:??';

    final Widget icon = Icon(
      transaction.netValue > 0 ? Icons.south_east : Icons.north_east,
      color: (confirmations == 0)
          ? Colors.white38
          : transaction.netValue > 0
              ? theme.colorScheme.primary
              : theme.colorScheme.error,
    );

    final tile = Padding(
        padding: EdgeInsets.symmetric(vertical: 4.0, horizontal: 24.0),
        child: Row(
          mainAxisSize: MainAxisSize.max,
          spacing: 8.0,
          children: [
            icon,
            Expanded(
              child: Padding(
                padding: EdgeInsets.symmetric(vertical: 12.0),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  mainAxisAlignment: MainAxisAlignment.spaceEvenly,
                  mainAxisSize: MainAxisSize.max,
                  spacing: 4.0,
                  children: [
                    Text(
                      dateText,
                      softWrap: false,
                      overflow: TextOverflow.fade,
                      style: theme.textTheme.titleMedium,
                    ),
                    Text(
                      timeText,
                      softWrap: false,
                      overflow: TextOverflow.fade,
                      style: theme.textTheme.titleSmall
                          ?.copyWith(color: Colors.white38),
                    ),
                  ],
                ),
              ),
            ),
            Expanded(
              child: Padding(
                padding: EdgeInsets.symmetric(vertical: 12.0),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.end,
                  mainAxisAlignment: MainAxisAlignment.spaceEvenly,
                  mainAxisSize: MainAxisSize.max,
                  spacing: 4.0,
                  children: [
                    SatoshiText(
                        value: transaction.netValue,
                        showSign: true,
                        style: theme.textTheme.titleMedium),
                    Text(
                      txidText,
                      softWrap: false,
                      overflow: TextOverflow.ellipsis,
                      style: GoogleFonts.sourceCodePro(
                          textStyle: theme.textTheme.titleSmall
                              ?.copyWith(color: Colors.white38)),
                    ),
                  ],
                ),
              ),
            ),
          ],
        ));

    rebroadcastAction(BuildContext context) {
      walletContext.wallet.rebroadcast(txid: txid);
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text('Transaction rebroadcasted'),
        ),
      );
    }

    copyAction(BuildContext context) {
      Clipboard.setData(ClipboardData(text: transaction.txid()));
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text('Transaction ID copied to clipboard'),
        ),
      );
    }

    final screenWidth = MediaQuery.of(context).size.width;

    return MenuAnchor(
      alignmentOffset: const Offset(32.0, -8.0),
      menuChildren: [
        MenuItemButton(
          onPressed: () => copyAction(context),
          leadingIcon: const Icon(Icons.copy),
          child: SizedBox(
            width: screenWidth * 2 / 3,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              spacing: 4.0,
              children: [
                Text('Copy Transaction ID'),
                Text(
                  txid,
                  softWrap: true,
                  maxLines: 3,
                  overflow: TextOverflow.ellipsis,
                  style: GoogleFonts.sourceCodePro(
                      textStyle: theme.textTheme.titleSmall
                          ?.copyWith(color: Colors.white38)),
                )
              ],
            ),
          ),
        ),
        (transaction.confirmationTime == null)
            ? MenuItemButton(
                onPressed: () => rebroadcastAction(context),
                leadingIcon: const Icon(Icons.publish),
                child: const Text('Rebroadcast'),
              )
            : SizedBox.shrink(),
      ],
      builder: (_, MenuController controller, Widget? child) {
        return InkWell(
          //focusNode: _inkWellFocusNode,
          onLongPress: () {
            if (controller.isOpen) {
              controller.close();
            } else {
              controller.open();
            }
          },
          child: tile,
        );
      },
    );
  }
}

class TxList extends StatelessWidget {
  const TxList({super.key});

  @override
  Widget build(BuildContext context) {
    final walletContext = WalletContext.of(context)!;

    return CustomScrollView(slivers: <Widget>[
      SliverToBoxAdapter(
        child: Column(
          children: [
            Container(
              color: Theme.of(context).colorScheme.surface,
              child: StreamBuilder(
                stream: walletContext.txStream,
                builder: (context, snapshot) => UpdatingBalance(),
              ),
              //child: UpdatingBalance(),
            ),
            Container(
              width: double.infinity,
              height: 16.0,
              color: Theme.of(context).colorScheme.surface,
              foregroundDecoration: BoxDecoration(
                color: Theme.of(context).colorScheme.secondary,
                borderRadius: BorderRadius.only(
                  topLeft: Radius.circular(16),
                  topRight: Radius.circular(16),
                ),
              ),
            ),
            //Padding(
            //  padding:
            //      const EdgeInsets.symmetric(horizontal: 24.0, vertical: 4.0)
            //          .copyWith(top: 8.0),
            //  child: Row(
            //    children: const [Expanded(child: Text('Transactions'))],
            //  ),
            //),
          ],
        ),
      ),
      StreamBuilder(
        stream: walletContext.txStream,
        builder: (context, snapshot) {
          final transactions = snapshot.data?.txs ?? [];

          return SliverList.builder(
            itemCount: transactions.length,
            itemBuilder: (context, index) =>
                TxItem(transaction: transactions[index]),
            //separatorBuilder: (BuildContext _, int index) => const Divider(),
          );
        },
      ),
    ]);
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

    return Padding(
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
                      walletName: walletCtx.walletName,
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
    );
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
              style: GoogleFonts.sourceCodePro(),
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
                            walletName: walletCtx.walletName,
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

    return CustomScrollView(
      slivers: <Widget>[
        SliverToBoxAdapter(
          child: Container(
            color: Theme.of(context).colorScheme.surface,
            child: UpdatingBalance(),
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
    required Wallet wallet,
    required MasterAppkey masterAppkey,
    Function()? onBroadcastNewTx}) async {
  final effect =
      unsignedTx.effect(masterAppkey: masterAppkey, network: wallet.network);

  final signatures = await showSigningProgressDialog(
    context,
    signingStream,
    describeEffect(context, effect),
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
    final balanceTextStyle = DefaultTextStyle.of(context)
        .style
        .copyWith(fontSize: 36.0, fontWeight: FontWeight.w600);
    final padding = EdgeInsets.all(24.0).copyWith(top: 16.0);

    return StreamBuilder<TxState>(
      stream: walletContext.txStream,
      builder: (context, snapshot) {
        if (snapshot.hasError) {
          return Text('Error: ${snapshot.error}', style: balanceTextStyle);
        }
        final transactions = snapshot.data?.txs ?? [];

        var pendingIncomingBalance = 0;
        var avaliableBalance = 0;
        for (final tx in transactions) {
          if (tx.confirmationTime == null && tx.netValue > 0) {
            pendingIncomingBalance += tx.netValue;
          } else {
            avaliableBalance += tx.netValue;
          }
        }

        return Padding(
          padding: padding,
          child: Row(
            mainAxisSize: MainAxisSize.max,
            children: [
              Expanded(
                child: Column(
                  spacing: 8.0,
                  children: [
                    SatoshiText(
                      value: avaliableBalance,
                      style: balanceTextStyle,
                      letterSpacingReductionFactor: 0.02,
                    ),
                    pendingIncomingBalance == 0
                        ? SizedBox.shrink()
                        : Row(
                            mainAxisSize: MainAxisSize.min,
                            spacing: 4.0,
                            children: [
                              Icon(Icons.hourglass_top, size: 12.0),
                              SatoshiText(
                                  value: pendingIncomingBalance,
                                  showSign: true),
                            ],
                          ),
                  ],
                ),
              )
            ],
          ),
        );
      },
    );
  }
}

class SatoshiText extends StatelessWidget {
  final int value;
  final bool showSign;
  final double opacityChangeFactor;
  final double letterSpacingReductionFactor;
  final TextStyle? style;

  const SatoshiText({
    Key? key,
    required this.value,
    this.showSign = false,
    this.opacityChangeFactor = 0.5,
    this.letterSpacingReductionFactor = 0.0,
    this.style,
  }) : super(key: key);

  const SatoshiText.withSign({
    Key? key,
    required int value,
  }) : this(key: key, value: value, showSign: true);

  @override
  Widget build(BuildContext context) {
    final baseStyle = GoogleFonts.inter(
        textStyle: style ?? DefaultTextStyle.of(context).style);
    // We reduce the line spacing by the percentage from the fontSize (as per design specs).
    final baseLetterSpacing = (baseStyle.letterSpacing ?? 0.0) -
        (baseStyle.fontSize ?? 0.0) * letterSpacingReductionFactor;

    final activeStyle = baseStyle.copyWith(letterSpacing: baseLetterSpacing);
    final inactiveStyle = baseStyle.copyWith(
      letterSpacing: baseLetterSpacing,
      // Reduce text opacity by `opacityChangeFactor` initially.
      color: baseStyle.color!.withAlpha(
          Color.getAlphaFromOpacity(baseStyle.color!.a * opacityChangeFactor)),
    );

    // Convert to BTC string with 8 decimal places
    String btcString = (value / 100000000.0).toStringAsFixed(8);
    // Split the string into two parts, removing - sign: before and after the decimal
    final parts = btcString.replaceFirst(r'-', '').split('.');
    // Format the fractional part into segments
    final String fractionalPart =
        "${parts[1].substring(0, 2)} ${parts[1].substring(2, 5)} ${parts[1].substring(5)}";
    // Combine the whole number part with the formatted fractional part
    btcString = '${parts[0]}.$fractionalPart \u20BF';
    // Add sign if required.
    if (showSign || !showSign && value.isNegative) {
      btcString = value.isNegative ? '- $btcString' : '+ $btcString';
    }

    var activeIndex = btcString.indexOf(RegExp(r'[1-9]'));
    if (activeIndex == -1) activeIndex = btcString.length - 1;
    final inactiveString = btcString.substring(0, activeIndex);
    final activeString = btcString.substring(activeIndex);

    return Text.rich(
      TextSpan(children: <TextSpan>[
        TextSpan(text: inactiveString, style: inactiveStyle),
        TextSpan(text: activeString, style: activeStyle),
      ]),
      textAlign: TextAlign.right,
      softWrap: false,
      overflow: TextOverflow.fade,
    );
  }
}
