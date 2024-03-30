import 'package:flutter/material.dart';
import 'package:frostsnapp/device_id_ext.dart';
import 'package:frostsnapp/device_list.dart';
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
            child: DeviceListContainer(child: DeviceList())),
      ),
    );
  }
}

class DeviceSettings extends StatelessWidget {
  final DeviceId id;
  const DeviceSettings({Key? key, required this.id}) : super(key: key);

  @override
  Widget build(BuildContext context) {
    return StreamBuilder(
        stream: deviceListSubject,
        builder: (context, snapshot) {
          final device = snapshot.data?.state.getDevice(id: id);
          final body;
          final deviceKeys = coord.keysForDevice(deviceId: id);

          if (!snapshot.hasData) {
            body = [CircularProgressIndicator()];
          } else if (device == null) {
            body = [
              Center(
                  child: Text(
                'Device disconnected',
                style: TextStyle(
                  color: Colors.grey,
                  fontSize: 24.0,
                ),
              ))
            ];
          } else {
            body = [
              Text(
                device.name!,
                style: TextStyle(
                  fontSize: 32,
                  fontWeight: FontWeight.bold,
                ),
              ),
              SizedBox(height: 10),
              DeviceNameField(id: device.id, existingName: device.name),
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
                          Expanded(
                              child: Text(
                            toHex(Uint8List.fromList(keyId.field0)),
                            overflow: TextOverflow.ellipsis,
                            textAlign: TextAlign.center,
                            maxLines: 1,
                            style: const TextStyle(
                              fontSize: 14,
                              fontFamily: 'Monospace',
                            ),
                          )),
                          SizedBox(width: 4),
                          ElevatedButton(
                            onPressed: () async {
                              await coord.displayBackup(
                                id: device.id,
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
            ];
          }

          return Scaffold(
            appBar: AppBar(
              title: Text(
                "Device Settings",
              ),
            ),
            body: Material(
              child: Column(
                children: body,
              ),
            ),
          );
        });
  }
}
