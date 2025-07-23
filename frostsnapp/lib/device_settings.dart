import 'dart:async';

import 'package:flutter/material.dart';
import 'package:frostsnap/animated_check.dart';
import 'package:frostsnap/device.dart';
import 'package:frostsnap/device_action.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/device_list.dart';
import 'package:frostsnap/device_setup.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/settings.dart';
import 'package:frostsnap/show_backup.dart';
import 'package:frostsnap/snackbar.dart';
import 'package:frostsnap/progress_indicator.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/src/rust/api/firmware_upgrade.dart';
import 'package:frostsnap/theme.dart';

class DeviceSettings extends StatefulWidget {
  final DeviceId id;
  const DeviceSettings({super.key, required this.id});

  @override
  State<DeviceSettings> createState() => _DeviceSettingsState();
}

class _DeviceSettingsState extends State<DeviceSettings> {
  late StreamSubscription _sub;
  ConnectedDevice? device;

  @override
  void initState() {
    super.initState();

    _sub = GlobalStreams.deviceListSubject.listen((event) {
      setState(() => device = event.state.getDevice(id: widget.id));

      final unplugged = event.changes.any(
        (change) =>
            change.kind == DeviceListChangeKind.removed &&
            deviceIdEquals(change.device.id, widget.id),
      );
      if (unplugged) {
        if (mounted) {
          Navigator.pop(context);
        }
      }
    });
  }

  @override
  void dispose() {
    super.dispose();
    _sub.cancel();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final Widget body;
    final keys = coord.keyState().keys;

    if (device == null) {
      body = Center(
        child: Column(
          children: [
            Text(
              'Waiting for device to reconnect',
              style: theme.textTheme.titleMedium,
            ),
            FsProgressIndicator(),
          ],
        ),
      );
    } else {
      final device_ = device!;
      final relevantDeviceKeys = keys.where((key) {
        final accessStructure = key.accessStructures().elementAtOrNull(0);
        if (accessStructure == null) return false;

        final devices = accessStructure.devices();
        return devices.any((d) => deviceIdEquals(d, device_.id));
      }).toList();

      Widget keyList = ListView.builder(
        shrinkWrap: true,
        itemCount: relevantDeviceKeys.length,
        itemBuilder: (context, index) {
          final key = relevantDeviceKeys[index];
          final accessStructureRef = key
              .accessStructures()
              .elementAtOrNull(0)
              ?.accessStructureRef();
          final keyName = key.keyName();
          return Padding(
            padding: EdgeInsets.symmetric(horizontal: 16.0, vertical: 4.0),
            child: ListTile(
              title: Text(keyName, style: const TextStyle(fontSize: 20.0)),
              trailing: ElevatedButton(
                onPressed: accessStructureRef == null
                    ? null
                    : () async {
                        Navigator.push(
                          context,
                          MaterialPageRoute(
                            builder: (context) {
                              return BackupSettingsPage(
                                context: context,
                                id: device_.id,
                                deviceName: device_.name ?? "??",
                                accessStructureRef: accessStructureRef,
                                keyName: keyName,
                              );
                            },
                          ),
                        );
                      },
                child: Text("Backup"),
              ),
            ),
          );
        },
      );

      if (relevantDeviceKeys.isEmpty) {
        keyList = Text(
          'No keys on this device',
          style: theme.textTheme.titleMedium,
        );
      }
      final deviceFirmwareDigest = device_.firmwareDigest;

      final firmwareSettings = Column(
        children: [
          Row(
            children: <Widget>[
              Text(
                'Device firmware: ',
                style: TextStyle(fontWeight: FontWeight.bold, fontSize: 16),
              ),
              Expanded(
                child: Text(
                  deviceFirmwareDigest,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    fontSize: 16,
                    fontFamily: monospaceTextStyle.fontFamily,
                  ),
                ),
              ),
            ],
          ),
          SizedBox(height: 5),
          Row(
            children: <Widget>[
              Text(
                'Latest firmware: ',
                style: TextStyle(fontWeight: FontWeight.bold, fontSize: 16),
              ),
              Expanded(
                child: Text(
                  coord.upgradeFirmwareDigest() ??
                      "<app compiled without firmware>",
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    fontSize: 16,
                    fontFamily: monospaceTextStyle.fontFamily,
                  ),
                ),
              ),
            ],
          ),
        ],
      );

      final wipeDeviceSettings = Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            'Delete all device data, making it a blank new device.',
            style: TextStyle(),
          ),
          SizedBox(height: 8),
          ElevatedButton(
            onPressed: () async {
              coord.wipeDeviceData(deviceId: device_.id);
            },
            child: Text("Wipe"),
          ),
          SizedBox(height: 8),
          ElevatedButton(
            onPressed: () async {
              coord.wipeAllDevices();
            },
            child: Text("Wipe All"),
          ),
        ],
      );

      final settings = SettingsSection(
        settings: [
          (
            "Name",
            DeviceNameField(id: device_.id, mode: DeviceNameMode.rename),
          ),
          ("Key Shares", keyList),
          ("Nonces", NonceCounterDisplay(id: device_.id)),
          ("Upgrade Firmware", firmwareSettings),
          ("Wipe Device", wipeDeviceSettings),
        ],
      );

      body = Column(
        children: [
          Text(
            device_.name!,
            style: TextStyle(fontSize: 32, fontWeight: FontWeight.bold),
          ),
          Expanded(child: settings),
        ],
      );
    }

    return Scaffold(
      appBar: FsAppBar(title: Text("Device Settings")),
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
    return KeyValueListWidget(
      data: {'Nonces left': coord.noncesAvailable(id: id).toString()},
    );
  }
}

class SettingsSection extends StatelessWidget {
  final List<(String, Widget)> settings;

  const SettingsSection({super.key, required this.settings});

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
              Text(settings[index].$1, style: TextStyle(fontSize: 32)),
              Divider(),
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

  static Future<void> show(BuildContext context) async {
    final result = await showDeviceActionDialog(
      context: context,
      builder: (context) => FirmwareUpgradeDialog(),
    );
    if (result == null) await coord.cancelProtocol();
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
          progressStream
              .listen((progress_) {
                setState(() => progress = progress_);
              })
              .onDone(() {
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

    return Column(
      children: [
        DialogHeader(child: Text(text)),
        Expanded(
          child: DeviceListWithIcons(
            iconAssigner: (context, deviceId) {
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
                    padding: EdgeInsets.symmetric(
                      vertical: 5.0,
                      horizontal: 5.0,
                    ),
                    child: LinearProgressIndicator(
                      value: progress!,
                      minHeight: 10.0,
                    ),
                  );
                }
              }

              return (null, icon);
            },
          ),
        ),
      ],
    );
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
                Text(entry.key, style: TextStyle(fontWeight: FontWeight.bold)),
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
  final AccessStructureRef accessStructureRef;
  final String keyName;

  const BackupSettingsPage({
    super.key,
    required BuildContext context,
    required this.id,
    required this.deviceName,
    required this.accessStructureRef,
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
                style: TextStyle(fontSize: 16),
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
                style: TextStyle(fontSize: 16),
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
                  await backupDeviceDialog(
                    context,
                    deviceId: id,
                    accessStructure: coord.getAccessStructure(
                      asRef: accessStructureRef,
                    )!,
                  );
                },
                child: Text("Show Backup"),
              ),
              SizedBox(height: 24),
              Text("Check this backup by re-entering it on the device:"),
              SizedBox(height: 8),
              ElevatedButton(
                onPressed: () async {
                  await verifyBackup(
                    context,
                    id,
                    coord
                        .getAccessStructure(asRef: accessStructureRef)!
                        .accessStructureRef(),
                  );
                },
                child: const Text("Verify Backup"),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
