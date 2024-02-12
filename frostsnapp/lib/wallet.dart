import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnapp/global.dart';
import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';

class WalletHome extends StatefulWidget {
  final KeyId keyId;

  const WalletHome({super.key, required this.keyId});

  @override
  _WalletHomeState createState() => _WalletHomeState();
}

class _WalletHomeState extends State<WalletHome> {
  int _selectedIndex = 0; // Tracks the current index for BottomNavigationBar

  // The widget options to display based on the selected index
  // A method that returns the correct widget based on the selected index
  Widget _getSelectedWidget() {
    switch (_selectedIndex) {
      case 0:
        // Pass any required parameters to the WalletActivity widget
        return WalletActivity(keyId: widget.keyId);
      case 1:
        // Placeholder for the Send page
        return Text('TODO: Send Page');
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

  const WalletActivity({super.key, required this.keyId});

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
        final stream = wallet.sync(keyId: widget.keyId).asBroadcastStream();
        setState(() {
          syncProgressStream = stream;
          floatingProgressKey = UniqueKey();
        });
        // this waits until stream is done so the button will keep spinning.
        await stream.toList();
      }),
      body: Stack(children: [
        if (syncProgressStream != null)
          FloatingProgress(
              key: floatingProgressKey, progressStream: syncProgressStream!),
        TxList(keyId: widget.keyId),
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
    })
      ..onDone(() {
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
  const TxList({super.key, required this.keyId});

  @override
  Widget build(BuildContext context) {
    return StreamBuilder<TxState>(
      stream: wallet.subTxState(keyId: keyId),
      builder: (context, snapshot) {
        if (snapshot.hasData) {
          final transactions = snapshot.data!.txs;
          const int init = 0;
          int balance =
              transactions.fold(init, (sum, item) => sum + item.netValue);
          return Column(
            children: [
              Padding(
                padding: const EdgeInsets.all(20.0),
                child: Text(
                  '${balance / 100000000}\u20BF', // Display balance in BTC
                  style: TextStyle(fontSize: 36, fontWeight: FontWeight.bold),
                ),
              ),
              Expanded(
                child: ListView.builder(
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
                          color: transaction.netValue > 0
                              ? Colors.green
                              : Colors.red,
                        ),
                        title: Row(
                          children: <Widget>[
                            Expanded(
                              child: Text('ID: ${transaction.txid()}'),
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
                                    content: Text(
                                        'Transaction ID copied to clipboard'),
                                  ),
                                );
                              },
                            ),
                          ],
                        ),
                        subtitle: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: <Widget>[
                            Text(
                              'Value: ${transaction.netValue / 100000000}\u20BF', // Display net value in BTC
                              style: TextStyle(
                                color: transaction.netValue > 0
                                    ? Colors.green
                                    : Colors.red,
                              ),
                            ),
                            Text(
                              confirmationText, // Display confirmation height and time or 'Unconfirmed'
                              style: TextStyle(
                                  fontSize: 12), // Adjust the style as needed
                            ),
                          ],
                        ),
                      ),
                    );
                  },
                ),
              ),
            ],
          );
        } else {
          return Center(child: CircularProgressIndicator());
        }
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
    return Scaffold(
      body: Column(
        children: [
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
        ],
      ),
    );
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
