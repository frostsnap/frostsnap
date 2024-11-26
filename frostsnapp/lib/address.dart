import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnapp/bridge_definitions.dart';
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
  final MasterAppkey masterAppkey;
  final AccessStructureRef accessStructureRef;

  const AddressPage({
    Key? key,
    required this.address,
    required this.masterAppkey,
    required this.accessStructureRef,
  }) : super(key: key);
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

  Future<void> _showVerificationDialog(BuildContext context,
      AccessStructureRef accessStructureRef, Address address) async {
    final verifyAddressStream = coord
        .verifyAddress(
          accessStructureRef: accessStructureRef,
          addressIndex: address.index,
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
              addressIndex: address.index,
              address: address.addressString,
            ),
            SizedBox(height: 16),
            StreamBuilder<VerifyAddressProtocolState>(
              stream: verifyAddressStream,
              builder: (context, snapshot) {
                return ElevatedButton(
                  onPressed: () {
                    if (snapshot.hasData &&
                        snapshot.data?.sentToDevices != null &&
                        snapshot.data!.sentToDevices.isNotEmpty) {
                      // go back an extra window
                      Navigator.of(context).pop();
                    }
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

  @override
  Widget build(BuildContext context) {
    final walletCtx = WalletContext.of(context)!;
    final derivationPath = walletCtx.wallet
        .derivationPathForAddress(index: address.index, changeAddress: false);

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
                      "Derivation path: $derivationPath",
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
                      onPressed: () {
                        // copy regardless in case the user forgot
                        Clipboard.setData(
                            ClipboardData(text: address.addressString));
                        _showVerificationDialog(
                            context, accessStructureRef, address);
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

class VerifyAddressProgress extends StatelessWidget {
  final Stream<VerifyAddressProtocolState> stream;
  final String address;
  final int addressIndex;

  const VerifyAddressProgress({
    Key? key,
    required this.stream,
    required this.address,
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
