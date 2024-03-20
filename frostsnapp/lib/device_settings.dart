import 'package:flutter/material.dart';
import 'package:frostsnapp/device_id_ext.dart';
import 'package:frostsnapp/device_list_widget.dart';
import 'package:frostsnapp/device_setup.dart';
import 'package:frostsnapp/ffi.dart';
import 'package:frostsnapp/global.dart';
import 'package:frostsnapp/hex.dart';
import 'dart:typed_data';

class DeviceSettingsPage extends StatelessWidget {
  const DeviceSettingsPage({
    super.key,
  });

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Device Settings')),
      body: Center(
        child: Padding(
          padding: EdgeInsets.all(8.0),
          child: DeviceSettingsContent(),
        ),
      ),
    );
  }
}

class DeviceSettingsContent extends StatelessWidget {
  const DeviceSettingsContent({Key? key}) : super(key: key);

  @override
  Widget build(BuildContext context) {
    return StreamBuilder(
      stream: deviceListSubject.map((update) => update.state),
      builder: (context, snapshot) {
        final data = snapshot.data;
        if (data == null) {
          return CircularProgressIndicator();
        } else {
          return Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              Text("Device Settings"),
              Divider(),
              Expanded(
                child: DeviceListContainer(
                  child: DeviceListWithIcons(
                    key: const Key("dialog-device-list"),
                    iconAssigner: (context, id) {
                      final device = api.getDevice(id: id);
                      return (
                        null,
                        Row(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            ElevatedButton(
                              onPressed: () {
                                Navigator.push(context,
                                    MaterialPageRoute(builder: (context) {
                                  return IndividualDeviceSettingsPage(
                                      device: device);
                                }));
                              },
                              child: Text("Settings"),
                            ),
                          ],
                        ),
                      );
                    },
                  ),
                ),
              ),
            ],
          );
        }
      },
    );
  }
}

class IndividualDeviceSettingsPage extends StatefulWidget {
  final Device device;
  const IndividualDeviceSettingsPage({Key? key, required this.device})
      : super(key: key);

  @override
  _IndividualDeviceSettingsPageState createState() =>
      _IndividualDeviceSettingsPageState();
}

class _IndividualDeviceSettingsPageState
    extends State<IndividualDeviceSettingsPage> {
  late Device maybeRefreshedDevice;

  @override
  void initState() {
    super.initState();
    maybeRefreshedDevice = widget.device;

    // listen for renaming
    deviceListChangeStream.listen((change) {
      if (change.kind == DeviceListChangeKind.Named &&
          deviceIdEquals(widget.device.id, change.device.id)) {
        setState(() {
          maybeRefreshedDevice = change.device;
        });
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    final deviceKeys = coord.keysForDevice(deviceId: widget.device.id);

    return Scaffold(
      appBar: AppBar(
        title: Text(
          "Device Settings",
        ),
      ),
      body: Material(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Center(
              child: Text(
                "${maybeRefreshedDevice.name}",
                style: TextStyle(
                  fontSize: 24,
                  fontWeight: FontWeight.bold,
                ),
              ),
            ),
            SizedBox(height: 10),
            Center(
              child: ElevatedButton(
                onPressed: () async {
                  await handleDeviceRenaming(context, maybeRefreshedDevice);
                },
                child: Text("Rename Device"),
              ),
            ),
            SizedBox(height: 20),
            Text(
              "Keys",
              textAlign: TextAlign.center,
              style: TextStyle(
                fontSize: 16,
                fontWeight: FontWeight.bold,
              ),
            ),
            Divider(),
            Container(
              alignment: Alignment.center,
              child: ListView.builder(
                shrinkWrap: true,
                itemCount: deviceKeys.length,
                itemBuilder: (context, index) {
                  final keyId = deviceKeys[index];
                  return ListTile(
                    title: Row(
                      mainAxisAlignment: MainAxisAlignment.center,
                      children: [
                        Text(
                          toHex(Uint8List.fromList(
                              maybeRefreshedDevice.id.field0)),
                          textAlign: TextAlign.center,
                          style: const TextStyle(
                            fontSize: 14,
                            fontFamily: 'Monospace',
                          ),
                        ),
                        SizedBox(width: 4),
                        ElevatedButton(
                          onPressed: () async {
                            await coord.displayBackup(
                              id: maybeRefreshedDevice.id,
                              keyId: keyId,
                            );
                          },
                          child: Text("Backup"),
                        ),
                      ],
                    ),
                  );
                },
              ),
            ),
          ],
        ),
      ),
    );
  }
}
