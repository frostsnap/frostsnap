import 'dart:async';

import 'package:flutter/material.dart';
import 'package:frostsnapp/animated_check.dart';
import 'package:frostsnapp/bridge_definitions.dart';
import 'package:frostsnapp/device.dart';
import 'package:frostsnapp/device_action.dart';
import 'package:frostsnapp/device_id_ext.dart';
import 'package:frostsnapp/device_list.dart';
import 'package:frostsnapp/device_setup.dart';
import 'package:frostsnapp/ffi.dart';
import 'package:frostsnapp/global.dart';
import 'package:frostsnapp/settings.dart';
import 'package:frostsnapp/show_backup.dart';
import 'package:frostsnapp/snackbar.dart';
import 'package:frostsnapp/theme.dart';

class DeviceSettingsPage extends StatelessWidget {
  const DeviceSettingsPage({
    super.key,
  });

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: FsAppBar(title: const Text('Device Settings')),
      body: DeviceList(),
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
  ConnectedDevice? device;

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
          child: Column(children: const [
        Text(
          'Waiting for device to reconnect',
          style: TextStyle(color: uninterestedColor, fontSize: 24.0),
        ),
        FsProgressIndicator(),
      ]));
    } else {
      final device_ = device!;
      Widget keyList = ListView.builder(
        shrinkWrap: true,
        itemCount: deviceKeys.length,
        itemBuilder: (context, index) {
          final keyId = deviceKeys[index];
          final keyName = coord.getKeyName(keyId: keyId)!;
          return Padding(
              padding: const EdgeInsets.only(
                  bottom: 4.0), // Adjust the padding/margin here
              child: ListTile(
                  title: Row(
                      mainAxisAlignment: MainAxisAlignment.start,
                      children: [
                    Column(children: [
                      Text(
                        keyName,
                        style: const TextStyle(fontSize: 20.0),
                      ),
                    ]),
                    SizedBox(width: 10),
                    ElevatedButton(
                      onPressed: () async {
                        Navigator.push(context,
                            MaterialPageRoute(builder: (context) {
                          return BackupSettingsPage(
                            context: context,
                            id: device_.id,
                            deviceName: device_.name ?? "unamed",
                            keyId: keyId,
                            keyName: keyName,
                          );
                        }));
                      },
                      child: Text("Backup"),
                    ),
                  ])));
        },
      );

      if (deviceKeys.isEmpty) {
        keyList = Text(
          'No keys on this device',
          style: TextStyle(color: uninterestedColor, fontSize: 20.0),
        );
      }
      final deviceFirmwareDigest = device_.firmwareDigest;

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
                child: Text(
                    coord.upgradeFirmwareDigest() ??
                        "<app compiled withotu firmware>",
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      fontFamily: 'RobotoMono', // Using a monospaced font
                      fontSize: 16,
                    ))),
          ],
        ),
      ]);

      final settings = SettingsSection(settings: [
        ("Name", DeviceNameField(id: device_.id, existingName: device_.name)),
        ("Keys", keyList),
        ("Nonces", NonceCounterDisplay(id: device_.id)),
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
      appBar: FsAppBar(
        title: Text(
          "Device Settings",
        ),
      ),
      body: Padding(
        padding: EdgeInsets.only(left: 16, bottom: 16),
        child: body,
      ),
    );
  }
}

class NonceCounterDisplay extends StatelessWidget {
  final DeviceId id;
  const NonceCounterDisplay({super.key, required this.id});

  @override
  Widget build(BuildContext context) {
    return KeyValueListWidget(data: {
      'Current nonce': coord.currentNonce(id: id).toString(),
      'Nonces left': coord.noncesAvailable(id: id).toString(),
    });
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
              Divider(thickness: 2, color: backgroundSecondaryColor),
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
  const FirmwareUpgradeDialog({super.key});

  @override
  State<FirmwareUpgradeDialog> createState() => _FirmwareUpgradeDialogState();

  static void show(BuildContext context) {
    showDeviceActionDialog(
        context: context,
        builder: (context) {
          return FirmwareUpgradeDialog();
        }).then((result) {
      if (result == null) {
        coord.cancelProtocol();
      }
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
        if (mounted) {
          showErrorSnackbarBottom(context, "Firmware upgrade aborted");
          Navigator.pop(context);
        }

        return;
      }

      if (newState.upgradeReadyToStart) {
        if (mounted && progress == null) {
          final progressStream = coord.enterFirmwareUpgradeMode();
          progressStream.listen((progress_) {
            setState(() => progress = progress_);
          }).onDone(() {
            if (mounted) {
              if (progress != 1.0) {
                showErrorSnackbarBottom(context, "Firmware upgrade failed");
              }
              Navigator.pop(context);
            }
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
      return FsProgressIndicator();
    }
    final confirmations = deviceIdSet(state!.confirmations);
    final needUpgrade = deviceIdSet(state!.needUpgrade);
    final text = progress == null
        ? "Confirm upgrade on devices"
        : "Wait for upgrade to complete";

    return Column(children: [
      DialogHeader(child: Text(text)),
      Expanded(child: DeviceListWithIcons(iconAssigner: (context, deviceId) {
        Widget? icon;

        if (needUpgrade.contains(deviceId)) {
          if (progress == null) {
            if (confirmations.contains(deviceId)) {
              icon = AnimatedCheckCircle();
            } else {
              icon = ConfirmPrompt();
            }
          } else {
            icon = Container(
                padding: EdgeInsets.symmetric(vertical: 5.0, horizontal: 5.0),
                child: LinearProgressIndicator(
                  value: progress!,
                  backgroundColor: backgroundSecondaryColor,
                  minHeight: 10.0,
                  valueColor: AlwaysStoppedAnimation<Color>(textColor),
                ));
          }
        }

        return (null, icon);
      }))
    ]);
  }
}

class KeyValueListWidget extends StatelessWidget {
  final Map<String, String> data;

  const KeyValueListWidget({super.key, required this.data});

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.all(16.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: data.entries.map((entry) {
          return Padding(
            padding: const EdgeInsets.symmetric(vertical: 4.0),
            child: Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [
                Text(
                  entry.key,
                  style: TextStyle(fontWeight: FontWeight.bold),
                ),
                Text(entry.value),
              ],
            ),
          );
        }).toList(),
      ),
    );
  }
}

class BackupSettingsPage extends StatelessWidget {
  final DeviceId id;
  final String deviceName;
  final KeyId keyId;
  final String keyName;

  const BackupSettingsPage({
    super.key,
    required BuildContext context,
    required this.id,
    required this.deviceName,
    required this.keyId,
    required this.keyName,
  });

  @override
  Widget build(BuildContext context) {
    return Scaffold(
        appBar: AppBar(title: Text('Key Share Backup')),
        body: Center(
            child: Padding(
                padding: EdgeInsets.all(8.0),
                child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        'Key:',
                        textAlign: TextAlign.left,
                        style: TextStyle(
                          fontSize: 16,
                        ),
                      ),
                      Text(
                        keyName,
                        style: TextStyle(fontWeight: FontWeight.bold),
                        textAlign: TextAlign.left,
                      ),
                      SizedBox(height: 8),
                      Text(
                        'Device:',
                        textAlign: TextAlign.left,
                        style: TextStyle(
                          fontSize: 16,
                        ),
                      ),
                      Text(
                        deviceName,
                        style: TextStyle(fontWeight: FontWeight.bold),
                        textAlign: TextAlign.left,
                      ),
                      SizedBox(height: 24),
                      Text(
                        'Display this device\'s share backup for this key:',
                        textAlign: TextAlign.left,
                      ),
                      SizedBox(height: 8),
                      ElevatedButton(
                        onPressed: () async {
                          await doBackupWorkflow(context,
                              devices: [id], keyId: keyId);
                        },
                        child: Text("Show Backup"),
                      ),
                      SizedBox(height: 24),
                      Text(
                          "Check this backup by re-entering it on the device:"),
                      SizedBox(height: 8),
                      ElevatedButton(
                        onPressed: () async {
                          final shareRestoreStream = coord
                              .checkShareOnDevice(deviceId: id, keyId: keyId)
                              .asBroadcastStream();

                          final aborted = shareRestoreStream
                              .firstWhere((state) => state.abort != null)
                              .then((state) {
                            if (context.mounted) {
                              showErrorSnackbarBottom(context, state.abort!);
                            }
                            return null;
                          });

                          final result = await showDeviceActionDialog<bool>(
                            context: context,
                            complete: aborted,
                            builder: (BuildContext context) {
                              return StreamBuilder(
                                  stream: shareRestoreStream,
                                  builder: (context, snapshot) {
                                    final outcome = snapshot.data?.outcome;
                                    return Column(children: [
                                      DialogHeader(
                                          child: Text(
                                              "Enter the backup on the device.")),
                                      Expanded(
                                        child: DeviceListWithIcons(
                                          iconAssigner: (context, deviceId) {
                                            if (deviceIdEquals(deviceId, id)) {
                                              final Widget icon = DevicePrompt(
                                                  icon: Icon(Icons.keyboard),
                                                  text: "");
                                              return (null, icon);
                                            } else {
                                              return (null, null);
                                            }
                                          },
                                        ),
                                      ),
                                      DialogFooter(
                                          child: ElevatedButton(
                                        onPressed: () {
                                          Navigator.pop(context, outcome);
                                        },
                                        style: ElevatedButton.styleFrom(
                                            backgroundColor: switch (outcome) {
                                          true => Colors.green,
                                          false => Colors.red,
                                          null => null,
                                        }),
                                        child: Text(switch (outcome) {
                                          true => "Your backup is valid",
                                          false => "Your backup is invalid",
                                          null => "Cancel"
                                        }),
                                      ))
                                    ]);
                                  });
                            },
                          );
                          if (result == null) {
                            coord.cancelProtocol();
                          }
                        },
                        child: Text('Check Backup'),
                      )
                    ]))));
  }
}
