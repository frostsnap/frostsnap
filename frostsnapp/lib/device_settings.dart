import 'package:flutter/material.dart';
import 'package:frostsnapp/device_id_ext.dart';
import 'package:frostsnapp/device_list_widget.dart';
import 'package:frostsnapp/device_setup.dart';
import 'package:frostsnapp/global.dart';
import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';

class DeviceSettingsPage extends StatelessWidget {
  const DeviceSettingsPage({super.key});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
        appBar: AppBar(title: const Text('Device Settings')),
        body: Center(
            child: Padding(
          padding: EdgeInsets.all(8.0),
          child: DeviceSettingsContent(),
        )));
  }
}

class DeviceSettingsContent extends StatelessWidget {
  const DeviceSettingsContent({Key? key}) : super(key: key);

  @override
  Widget build(BuildContext context) {
    return StreamBuilder(
      initialData: api.deviceListState(),
      stream: deviceListSubject.map((update) => update.state),
      builder: (context, snapshot) {
        final devicesPluggedIn = deviceIdSet();
        devicesPluggedIn
            .addAll(snapshot.data!.devices.map((device) => device.id));

        final deviceSettingsList = DeviceListContainer(
            child: DeviceListWithIcons(
                key: const Key("dialog-device-list"),
                iconAssigner: (context, id) {
                  return (
                    null,
                    Row(mainAxisSize: MainAxisSize.min, children: [
                      ElevatedButton(
                        onPressed: () async {
                          await handleDeviceRenaming(
                              context, api.getDevice(id: id));
                        },
                        child: Text("Rename"),
                      ),
                    ])
                  );
                }));

        return Column(mainAxisAlignment: MainAxisAlignment.center, children: [
          Text("Device Settings"),
          Divider(),
          MaybeExpandedVertical(child: deviceSettingsList),
        ]);
      },
    );
  }
}
