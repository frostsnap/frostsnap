import 'dart:async';
import 'dart:collection';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:frostsnapp/animated_check.dart';
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
        if (!_deviceRemoved.isCompleted) {
          _deviceRemoved.complete();
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
    final Widget body;
    final deviceKeys = coord.keysForDevice(deviceId: widget.id);
    if (device == null) {
      body = Center(
          child: Column(children: [
        Text(
          'Waiting for device to reconnect',
          style: TextStyle(color: Colors.grey, fontSize: 24.0),
        ),
        CircularProgressIndicator(),
      ]));
    } else {
      final device_ = device!;
      Widget keyList = ListView.builder(
        shrinkWrap: true,
        itemCount: deviceKeys.length,
        itemBuilder: (context, index) {
          final keyId = deviceKeys[index];
          return ListTile(
            title: Row(
              mainAxisAlignment: MainAxisAlignment.start,
              children: [
                Text(
                  toHex(Uint8List.fromList(keyId.field0)),
                  overflow: TextOverflow.ellipsis,
                  maxLines: 1,
                  style: const TextStyle(
                    fontSize: 14,
                    fontFamily: 'Monospace',
                  ),
                ),
                SizedBox(width: 10),
                ElevatedButton(
                  onPressed: () async {
                    final confirmed =
                        coord.displayBackup(id: widget.id, keyId: keyId).first;

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
                                        DeviceListWithIcons(
                                            iconAssigner: (context, deviceId) {
                                  if (deviceIdEquals(deviceId, widget.id)) {
                                    final label = LabeledDeviceText(
                                        device_.name ?? "<unamed>");
                                    final Widget icon;
                                    if (snapshot.connectionState ==
                                        ConnectionState.waiting) {
                                      icon = Row(
                                          mainAxisSize: MainAxisSize.min,
                                          children: const [
                                            Icon(Icons.visibility,
                                                color: Colors.orange),
                                            SizedBox(width: 4),
                                            Text("Confirm"),
                                          ]);
                                    } else {
                                      icon = Row(
                                          mainAxisSize: MainAxisSize.min,
                                          children: const [
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
                SizedBox(height: 10),
                ElevatedButton(
                    onPressed: () async {
                      final shareRestoreStream = coord.restoreShareOnDevice(
                          deviceId: widget.id, keyId: keyId);

                      shareRestoreStream.listen((event) {}).onDone(() {});

                      // shareRestoreStream.listen((progress_) {
                      //   setState(() => progress = progress_);
                      // }).onDone(() {
                      //   if (progress != 1.0) {
                      //     showErrorSnackbar(context, "Firmware upgrade failed");
                      //   }
                      //   if (mounted) {
                      //     Navigator.pop(context);
                      //   }
                      //   widget.onUpgradeFinished?.call();
                      // });
                    },
                    child: Text("Restore")),
              ],
            ),
          );
        },
      );

      if (deviceKeys.isEmpty) {
        keyList = Text(
          'No keys on this device',
          style: TextStyle(color: Colors.grey, fontSize: 20.0),
        );
      }
      final deviceFirmwareDigest = device_.firmwareDigest ?? 'factory';
      final canUpdate = coord.upgradeFirmwareDigest() != deviceFirmwareDigest;

      final firmwareSettings = Column(children: [
        Row(children: <Widget>[
          Text('Device firmware: ',
              style: TextStyle(
                fontWeight: FontWeight.bold,
                fontSize: 16,
              )),
          Expanded(
              child: Text(deviceFirmwareDigest,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    fontFamily: 'RobotoMono', // Using a monospaced font
                    fontSize: 16,
                  ))),
        ]),
        SizedBox(height: 5),
        Row(
          children: <Widget>[
            Text('Latest firmware: ',
                style: TextStyle(
                  fontWeight: FontWeight.bold,
                  fontSize: 16,
                )),
            Expanded(
                child: Text(coord.upgradeFirmwareDigest(),
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      fontFamily: 'RobotoMono', // Using a monospaced font
                      fontSize: 16,
                    ))),
          ],
        ),
        SizedBox(height: 20),
        ElevatedButton(
            onPressed: !canUpdate
                ? null
                : () {
                    FirmwareUpgradeDialog.show(context, onUpgradeFinished: () {
                      if (mounted) {
                        Navigator.pop(context);
                      }
                    });
                  },
            child: Text("Upgrade firmware"))
      ]);

      final settings = SettingsSection(settings: [
        ("Name", DeviceNameField(id: device_.id, existingName: device_.name)),
        ("Keys", keyList),
        ("Update Firmware", firmwareSettings)
      ]);

      body = Column(children: [
        Text(
          device_.name!,
          style: TextStyle(
            fontSize: 32,
            fontWeight: FontWeight.bold,
          ),
        ),
        Expanded(child: settings)
      ]);
    }

    return Scaffold(
        appBar: AppBar(
          title: Text(
            "Device Settings",
          ),
        ),
        body: Material(
          child: Padding(
            padding: EdgeInsets.only(left: 16, bottom: 16),
            child: body,
          ),
        ));
  }
}

class SettingsSection extends StatelessWidget {
  final List<(String, Widget)> settings;

  const SettingsSection({Key? key, required this.settings}) : super(key: key);

  @override
  Widget build(BuildContext context) {
    return ListView.builder(
      shrinkWrap: true,
      itemCount: settings.length,
      itemBuilder: (BuildContext context, int index) {
        return Padding(
          padding: const EdgeInsets.symmetric(vertical: 8.0),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: <Widget>[
              Text(
                settings[index].$1,
                style: TextStyle(
                  fontSize: 32,
                ),
              ),
              Divider(
                  thickness: 2,
                  color: Colors.black), // Adding a divider under the heading
              Padding(
                padding: const EdgeInsets.only(top: 8.0, bottom: 16.0),
                child: settings[index].$2,
              ),
            ],
          ),
        );
      },
    );
  }
}

class FirmwareUpgradeDialog extends StatefulWidget {
  Function()? onUpgradeFinished;

  @override
  State<FirmwareUpgradeDialog> createState() => _FirmwareUpgradeDialogState();

  static void show(BuildContext context, {Function()? onUpgradeFinished}) {
    showDialog(
        barrierDismissible: false,
        context: context,
        builder: (context) {
          return FirmwareUpgradeDialog();
        });
  }
}

class _FirmwareUpgradeDialogState extends State<FirmwareUpgradeDialog> {
  FirmwareUpgradeConfirmState? state;
  double? progress;
  late StreamSubscription<FirmwareUpgradeConfirmState> sub;

  @override
  void initState() {
    super.initState();
    final stream = coord.startFirmwareUpgrade();

    sub = stream.listen((newState) {
      setState(() {
        state = newState;
      });

      if (newState.abort) {
        showErrorSnackbar(context, "Firmware upgrade aborted");
        if (mounted) {
          Navigator.pop(context);
        }
        widget.onUpgradeFinished?.call();

        return;
      }

      if (newState.upgradeReadyToStart) {
        if (mounted && progress == null) {
          final progressStream = coord.enterFirmwareUpgradeMode();
          progressStream.listen((progress_) {
            setState(() => progress = progress_);
          }).onDone(() {
            if (progress != 1.0) {
              showErrorSnackbar(context, "Firmware upgrade failed");
            }
            if (mounted) {
              Navigator.pop(context);
            }
            widget.onUpgradeFinished?.call();
          });
        }
      }
    });
  }

  @override
  void dispose() {
    sub.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    if (state == null) {
      return AlertDialog(content: CircularProgressIndicator.adaptive());
    }
    final confirmations = deviceIdSet(state!.confirmations);
    final needUpgrade = deviceIdSet(state!.needUpgrade);
    return AlertDialog(
        actions: [
          if (progress == null)
            ElevatedButton(
                onPressed: () {
                  coord.cancelProtocol();
                  Navigator.pop(context);
                },
                child: const Text("Cancel"))
        ],
        content: DialogContainer(
            child: Column(children: [
          progress == null
              ? Text("Confirm upgrade on devices")
              : Text(
                  "Wait for upgrade to complete.\nDevices will restart once finished."),
          Divider(),
          MaybeExpandedVertical(child: DeviceListContainer(
              child: DeviceListWithIcons(iconAssigner: (context, deviceId) {
            Widget? icon;

            if (needUpgrade.contains(deviceId)) {
              if (progress == null) {
                if (confirmations.contains(deviceId)) {
                  icon = AnimatedCheckCircle();
                } else {
                  icon = Row(mainAxisSize: MainAxisSize.min, children: const [
                    Icon(Icons.touch_app, color: Colors.orange),
                    SizedBox(width: 4),
                    Text("Confirm"),
                  ]);
                }
              } else {
                icon = Container(
                    padding:
                        EdgeInsets.symmetric(vertical: 5.0, horizontal: 30.0),
                    child: LinearProgressIndicator(
                      value: progress!,
                      backgroundColor: Colors.grey[200],
                      minHeight: 10.0,
                      valueColor: AlwaysStoppedAnimation<Color>(Colors.blue),
                    ));
              }
            }

            return (null, icon);
          }))),
        ])));
  }
}
