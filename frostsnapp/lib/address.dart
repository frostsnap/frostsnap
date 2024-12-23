import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnapp/bridge_definitions.dart';
import 'package:frostsnapp/contexts.dart';
import 'package:frostsnapp/device_action.dart';
import 'package:frostsnapp/device_list.dart';
import 'package:frostsnapp/global.dart';
import 'package:frostsnapp/hex.dart';
import 'package:frostsnapp/id_ext.dart';
import 'package:frostsnapp/stream_ext.dart';
import 'package:frostsnapp/theme.dart';
import 'package:frostsnapp/wallet.dart';
import 'package:pretty_qr_code/pretty_qr_code.dart';

class AddressPage extends StatelessWidget {
  final Address address;

  const AddressPage({
    super.key,
    required this.address,
  });

  void _showQrDialog(BuildContext context) {
    final qrCode = QrCode(8, QrErrorCorrectLevel.L);
    qrCode.addData(address.addressString);
    final qrImage = QrImage(qrCode);
    showDialog(
      context: context,
      builder: (context) => Dialog(
          child: Padding(
        padding: const EdgeInsets.all(16.0),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Card(
              color: Colors.white,
              child: Padding(
                padding: const EdgeInsets.all(16.0),
                child: ConstrainedBox(
                  constraints: BoxConstraints(
                    maxWidth: 450,
                    maxHeight: 450,
                  ),
                  child: PrettyQrView(
                    qrImage: qrImage,
                    decoration: const PrettyQrDecoration(
                      shape: PrettyQrSmoothSymbol(),
                    ),
                  ),
                ),
              ),
            ),
            TextButton(
              onPressed: () => Navigator.pop(context),
              child: Text('Close'),
            ),
          ],
        ),
      )),
    );
  }

  @override
  Widget build(BuildContext context) {
    final walletCtx = WalletContext.of(context)!;

    return Scaffold(
      appBar: AppBar(
        title: Text('Address'),
      ),
      body: Column(
        children: [
          Expanded(
            child: SingleChildScrollView(
              child: Padding(
                padding: const EdgeInsets.all(16.0),
                child: Column(
                  children: [
                    Text(
                      "Address",
                      style: TextStyle(
                        fontSize: 24,
                        fontWeight: FontWeight.bold,
                        color: textColor,
                      ),
                    ),
                    Text(
                      "Derivation path: ${address.derivationPath}",
                      style: TextStyle(color: textSecondaryColor),
                    ),
                    SizedBox(height: 16),
                    GestureDetector(
                      child: chunkedAddressFormat(address.addressString,
                          backgroundColor: backgroundSecondaryColor,
                          textColor: textColor),
                      onTap: () {
                        copyToClipboard(context, address.addressString);
                      },
                    ),
                    SizedBox(height: 16),
                    Row(
                      mainAxisAlignment: MainAxisAlignment.center,
                      children: [
                        IconButton(
                          iconSize: 30.0,
                          onPressed: () =>
                              copyToClipboard(context, address.addressString),
                          icon: Icon(Icons.copy),
                        ),
                        SizedBox(width: 16),
                        IconButton(
                          onPressed: () => _showQrDialog(context),
                          icon: Icon(Icons.qr_code),
                        ),
                      ],
                    ),
                    SizedBox(height: 32),
                    Text(
                        "After giving this address to the sender you can verify it was securely transmitted by checking it against a device."),
                    SizedBox(height: 8),
                    ElevatedButton(
                      onPressed: () async {
                        // copy regardless in case the user forgot
                        Clipboard.setData(
                            ClipboardData(text: address.addressString));
                        await _showVerificationDialog(
                          context,
                          walletCtx.wallet.keyId(),
                          address.index,
                        );
                      },
                      child: Text('Verify Address'),
                    )
                  ],
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

Future<void> _showVerificationDialog(
    BuildContext context, KeyId keyId, int index) async {
  final verifyAddressStream = coord
      .verifyAddress(
        keyId: keyId,
        addressIndex: index,
      )
      .toBehaviorSubject();

  await showDeviceActionDialog(
    context: context,
    builder: (context) {
      return Column(
        children: [
          DialogHeader(
            child: Column(
              children: const [
                Text("Plug in a device to verify this address"),
              ],
            ),
          ),
          SizedBox(height: 16),
          VerifyAddressProgress(
            stream: verifyAddressStream,
            addressIndex: index,
          ),
          SizedBox(height: 16),
          StreamBuilder<VerifyAddressProtocolState>(
            stream: verifyAddressStream,
            builder: (context, snapshot) {
              return ElevatedButton(
                onPressed: () {
                  Navigator.of(context).pop();
                },
                child: Text('Done'),
              );
            },
          ),
        ],
      );
    },
  );
  coord.cancelProtocol();
}

class VerifyAddressProgress extends StatelessWidget {
  final Stream<VerifyAddressProtocolState> stream;
  final int addressIndex;

  const VerifyAddressProgress({
    Key? key,
    required this.stream,
    required this.addressIndex,
  }) : super(key: key);

  @override
  Widget build(BuildContext context) {
    return StreamBuilder<DeviceListState>(
      stream: deviceListSubject.map((update) => update.state),
      builder: (context, deviceListSnapshot) {
        if (!deviceListSnapshot.hasData) {
          return FsProgressIndicator();
        }
        final devicesPluggedIn = deviceIdSet(deviceListSnapshot.data!.devices
            .map((device) => device.id)
            .toList());

        return StreamBuilder<VerifyAddressProtocolState>(
          stream: stream,
          builder: (context, verifyAddressSnapshot) {
            if (!verifyAddressSnapshot.hasData) {
              return FsProgressIndicator();
            }
            final state = verifyAddressSnapshot.data!;

            bool targetConnected =
                state.targetDevices.any((id) => devicesPluggedIn.contains(id));

            final deviceProgress = ListView.builder(
              shrinkWrap: true,
              itemCount: state.targetDevices.length,
              itemBuilder: (context, index) {
                final id = state.targetDevices[index];
                final name = coord.getDeviceName(id: id);

                Widget icon;
                if (devicesPluggedIn.contains(id)) {
                  icon =
                      Icon(Icons.policy, color: awaitingColor, size: iconSize);
                } else {
                  icon = Icon(Icons.circle_outlined,
                      color: textColor, size: iconSize);
                }

                return ListTile(
                  title: Text(name ?? "<unknown>"),
                  trailing: SizedBox(height: iconSize, child: icon),
                );
              },
            );

            return Column(children: [
              deviceProgress,
              if (targetConnected)
                DialogFooter(
                  child: Column(children: const [
                    Row(
                      mainAxisAlignment: MainAxisAlignment.center,
                      children: [
                        Icon(
                          Icons.info_outline,
                          size: 20,
                        ),
                        SizedBox(width: 8),
                        Flexible(
                          child: Text(
                            "Check that the sender can see the same address as shown on the device.",
                          ),
                        ),
                      ],
                    ),
                    SizedBox(height: 16),
                    Row(
                      mainAxisAlignment: MainAxisAlignment.center,
                      children: [
                        Icon(
                          Icons.info_outline,
                          size: 20,
                        ),
                        SizedBox(width: 8),
                        Flexible(
                          child: Text(
                            "Two random chunks have been highlighted for convenience.",
                          ),
                        ),
                      ],
                    ),
                  ]),
                ),
            ]);
          },
        );
      },
    );
  }
}

class CheckAddressPage extends StatefulWidget {
  const CheckAddressPage({
    Key? key,
  }) : super(key: key);

  @override
  State<CheckAddressPage> createState() => _CheckAddressPageState();
}

class _CheckAddressPageState extends State<CheckAddressPage> {
  late final TextEditingController textInputController;
  Future<SearchResult>? searchFuture;
  int currentDepth = 0;
  int searchSize = 100;

  @override
  void initState() {
    super.initState();
    textInputController = TextEditingController();
  }

  @override
  void dispose() {
    textInputController.dispose();
    super.dispose();
  }

  Future<SearchResult> searchAddress() async {
    final walletContext = WalletContext.of(context)!;
    if (currentDepth >= 1000) {
      searchSize = 1000;
    }

    final address = await walletContext.wallet.superWallet.searchForAddress(
      masterAppkey: walletContext.wallet.masterAppkey,
      addressStr: textInputController.text,
      start: currentDepth,
      stop: currentDepth + searchSize,
    );

    currentDepth += searchSize;

    return SearchResult(
      depth: currentDepth,
      address: address,
    );
  }

  Widget _buildSearchResults(SearchResult? result) {
    if (result == null) return const SizedBox.shrink();

    final children = <Widget>[
      Text(result.address != null
          ? "Found!"
          : "Address not found in first ${result.depth} addresses."),
      const SizedBox(height: 8),
    ];

    if (result.address != null) {
      children.addAll([
        Text(
            "This address belongs to us at ${result.address?.derivationPath ?? ""}"),
        const SizedBox(height: 16),
        ElevatedButton(
          onPressed: () => _navigateToAddressPage(result.address!),
          child: const Text("Address info"),
        ),
      ]);
    } else {
      if (result.depth < 10000) {
        if (result.depth >= 1000) {
          children.addAll([
            const Text(
              "This address almost certainly doesn't belong to this wallet under any normal usage.",
            ),
            const SizedBox(height: 8),
            const Text("It's not yours or check another wallet."),
          ]);
        }
      } else {
        children.add(const Text("Look elsewhere... "));
      }
      children.addAll([
        const SizedBox(height: 16),
        ElevatedButton(
          onPressed: () {
            setState(() {
              searchFuture = searchAddress();
            });
          },
          child: const Text('Search deeper'),
        ),
      ]);
    }

    return Column(children: children);
  }

  void _navigateToAddressPage(Address address) {
    Navigator.push(
      context,
      MaterialPageRoute(
        builder: (context) => AddressPage(
          address: address,
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: Center(
        child: SingleChildScrollView(
          child: Padding(
            padding: const EdgeInsets.all(16.0),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                const Text('Check whether an address belongs to this wallet'),
                const SizedBox(height: 16),
                SizedBox(
                  width: 400,
                  child: TextFormField(
                    controller: textInputController,
                    minLines: 2,
                    maxLines: 6,
                    decoration: const InputDecoration(
                      counterText: '',
                    ),
                  ),
                ),
                const SizedBox(height: 32),
                ElevatedButton(
                  onPressed: () {
                    currentDepth = 0;
                    searchSize = 100;
                    setState(() {
                      searchFuture = searchAddress();
                    });
                  },
                  child: const Text('Look for address'),
                ),
                const SizedBox(height: 16),
                FutureBuilder<SearchResult>(
                  future: searchFuture,
                  builder: (context, snapshot) {
                    if (snapshot.connectionState == ConnectionState.waiting) {
                      return const Center(
                        child: Padding(
                          padding: EdgeInsets.all(16.0),
                          child: CircularProgressIndicator(),
                        ),
                      );
                    }

                    if (snapshot.hasError) {
                      return Text('Error: ${snapshot.error}');
                    }

                    return _buildSearchResults(snapshot.data);
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

class SearchResult {
  final int depth;
  final Address? address;

  const SearchResult({
    required this.depth,
    required this.address,
  });
}
