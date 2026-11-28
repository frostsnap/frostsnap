import 'package:flutter/material.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/theme.dart';

/// Whether any plugged-in Frostsnap device is on a port another process is using.
Stream<bool> anyDevicePortBusy(Stream<DeviceListUpdate> updates) =>
    updates.map((update) => update.state.busyPorts.isNotEmpty).distinct();

/// Shows a banner above the whole app while a Frostsnap device is plugged in but another process
/// (usually another instance of this app) has its port open, so its devices can't show up here.
class DeviceBusyBanner extends StatelessWidget {
  static const message =
      'A Frostsnap device is connected, but another process is using it. '
      'Close the other app to use it here.';

  final Stream<bool> busy;
  final Widget child;

  const DeviceBusyBanner({super.key, required this.busy, required this.child});

  @override
  Widget build(BuildContext context) {
    return StreamBuilder<bool>(
      stream: busy,
      initialData: false,
      builder: (context, snapshot) {
        final show = snapshot.data ?? false;
        final theme = Theme.of(context);
        // Keep `child` at the same place in the tree whether or not the banner shows, so
        // showing it doesn't rebuild the app underneath.
        return Column(
          children: [
            AnimatedSize(
              duration: Durations.medium1,
              alignment: Alignment.topCenter,
              child: show
                  ? Material(
                      color: theme.colorScheme.surfaceContainerHigh,
                      child: SafeArea(
                        bottom: false,
                        child: ListTile(
                          dense: true,
                          leading: Icon(
                            Icons.usb_off_rounded,
                            color: cautionColor,
                          ),
                          title: Text(message),
                        ),
                      ),
                    )
                  : const SizedBox(width: double.infinity),
            ),
            Expanded(
              child: MediaQuery.removePadding(
                context: context,
                removeTop: show,
                child: child,
              ),
            ),
          ],
        );
      },
    );
  }
}
