import 'dart:async';

import 'package:flutter/material.dart';
import 'package:frostsnapp/device_action.dart';
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

class DeviceSettings extends StatefulWidget {
  final DeviceId id;
  const DeviceSettings({Key? key, required this.id}) : super(key: key);

  @override
  State<DeviceSettings> createState() => _DeviceSettingsState();
}

class _DeviceSettingsState extends State<DeviceSettings> {
  late StreamSubscription _subscription;
  late Completer<void> _deviceRemoved;
  Device? device;

  @override
  void initState() {
    super.initState();
    _deviceRemoved = Completer();
    _subscription = deviceListSubject.listen((event) {
      setState(() {
        device = event.state.getDevice(id: widget.id);
      });
      if (device == null) {
        _deviceRemoved.complete();
        if (mounted) {
          Navigator.pop(context);
        }
      }
    });
  }

  @override
  void dispose() {
    _subscription.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final List<Widget> body;
    final deviceKeys = coord.keysForDevice(deviceId: widget.id);
    if (device == null) {
      body = [CircularProgressIndicator()];
    } else {
      final device_ = device!;
      body = [
        Text(
          device_.name!,
          style: TextStyle(
            fontSize: 32,
            fontWeight: FontWeight.bold,
          ),
        ),
        SizedBox(height: 10),
        DeviceNameField(id: device_.id, existingName: device_.name),
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
                        final confirmed = coord
                            .displayBackup(id: widget.id, keyId: keyId)
                            .first;

                        await showDeviceActionDialog(
                            context: context,
                            complete: _deviceRemoved.future,
                            content: FutureBuilder(
                                future: confirmed,
                                builder: (context, snapshot) {
                                  return Column(children: [
                                    Text(snapshot.connectionState ==
                                            ConnectionState.waiting
                                        ? "Confirm on device to show backup"
                                        : "Record backup displayed on device screen. Press cancel when finished."),
                                    Divider(),
                                    MaybeExpandedVertical(child:
                                        DeviceListContainer(child:
                                            DeviceListWithIcons(iconAssigner:
                                                (context, deviceId) {
                                      if (deviceIdEquals(deviceId, widget.id)) {
                                        final label = LabeledDeviceText(
                                            device_.name ?? "<unamed>");
                                        final Widget icon;
                                        if (snapshot.connectionState ==
                                            ConnectionState.waiting) {
                                          icon = Row(
                                              mainAxisSize: MainAxisSize.min,
                                              children: [
                                                Icon(Icons.visibility,
                                                    color: Colors.orange),
                                                SizedBox(width: 4),
                                                Text("Confirm"),
                                              ]);
                                        } else {
                                          icon = Row(
                                              mainAxisSize: MainAxisSize.min,
                                              children: [
                                                Icon(Icons.edit_document,
                                                    color: Colors.green),
                                                SizedBox(width: 4),
                                                Text("Record backup"),
                                              ]);
                                        }
                                        return (label, icon);
                                      } else {
                                        return (null, null);
                                      }
                                    })))
                                  ]);
                                }),
                            onCancel: () {
                              coord.cancelAll();
                            });
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
  }
}
