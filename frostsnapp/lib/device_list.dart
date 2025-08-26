import 'dart:async';
import 'dart:io';
import 'package:flutter/material.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/device.dart';
import 'package:frostsnap/device_action_upgrade.dart';
import 'package:frostsnap/device_settings.dart';
import 'package:frostsnap/device_setup.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/theme.dart';
import 'package:frostsnap/wallet_device_list.dart';
import 'package:sliver_tools/sliver_tools.dart';
import 'global.dart';
import 'maybe_fullscreen_dialog.dart';
import 'wallet_list_controller.dart';

typedef DeviceBuilder =
    Widget Function(
      BuildContext context, {
      required ConnectedDevice device,
      required Orientation orientation,
      required Animation<double> animation,
      String? previewName,
    });

const double iconSize = 20.0;

class DeviceList extends StatefulWidget {
  final DeviceBuilder deviceBuilder;

  const DeviceList({super.key, this.deviceBuilder = buildInteractiveDevice});

  @override
  State<StatefulWidget> createState() => _DeviceListState();
}

class _DeviceListState extends State<DeviceList> {
  GlobalKey<AnimatedListState> deviceListKey = GlobalKey<AnimatedListState>();
  StreamSubscription? _subscription;
  late DeviceListState currentListState;

  @override
  void initState() {
    super.initState();
    currentListState = coord.deviceListState();
    _subscription = GlobalStreams.deviceListUpdateStream.listen((update) async {
      if (update.state.stateId != currentListState.stateId + 1) {
        // our states are out of sync somehow -- reset the list.
        //
        // NOTE: This should never happen in practice but I set up these state
        // ids while debugging to exclude states missing as a possible problem.
        setState(() {
          deviceListKey = GlobalKey();
        });
      } else {
        for (final change in update.changes) {
          switch (change.kind) {
            case DeviceListChangeKind.added:
              {
                deviceListKey.currentState!.insertItem(
                  change.index,
                  duration: const Duration(milliseconds: 800),
                );
              }
            case DeviceListChangeKind.removed:
              {
                deviceListKey.currentState!.removeItem(change.index, (
                  BuildContext context,
                  Animation<double> animation,
                ) {
                  return widget.deviceBuilder(
                    context,
                    device: change.device,
                    orientation: effectiveOrientation(context),
                    animation: animation,
                  );
                });
              }
            default:
              {
                /* nothing needs to be done for other states*/
              }
          }
        }
      }
      setState(() {
        currentListState = update.state;
      });
    });
  }

  @override
  void dispose() {
    _subscription?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final orientation = effectiveOrientation(context);

    final noDevices = StreamBuilder(
      stream: GlobalStreams.deviceListSubject,
      builder: (context, snapshot) {
        if (!snapshot.hasData || snapshot.data!.state.devices.isNotEmpty) {
          return SizedBox();
        } else {
          return Text(
            'No devices connected',
            style: theme.textTheme.titleMedium,
          );
        }
      },
    );
    final list = AnimatedList(
      primary:
          true, // I dunno but the scrollbar doesn't work unless you set this
      padding: EdgeInsets.symmetric(vertical: 5),
      shrinkWrap: true,
      key: deviceListKey,
      itemBuilder: (context, index, animation) {
        final device = currentListState.devices[index];
        return widget.deviceBuilder(
          context,
          device: device,
          orientation: orientation,
          animation: animation,
        );
      },
      initialItemCount: currentListState.devices.length,
      scrollDirection: orientation == Orientation.landscape
          ? Axis.horizontal
          : Axis.vertical,
    );

    return Stack(
      children: [
        Center(child: noDevices),
        Align(
          alignment: Alignment.topCenter,
          child: Scrollbar(thumbVisibility: true, child: list),
        ),
      ],
    );
  }
}

class DeviceBoxContainer extends StatelessWidget {
  final Animation<double> animation;
  final Widget child;
  final Orientation orientation;

  const DeviceBoxContainer({
    required this.child,
    required this.orientation,
    required this.animation,
    super.key,
  });

  @override
  Widget build(BuildContext context) {
    final animationBegin = orientation == Orientation.landscape
        ? const Offset(8.0, 0.0)
        : const Offset(0.0, 8.0);
    return SlideTransition(
      position: animation.drive(
        Tween(begin: animationBegin, end: const Offset(0.0, 0.0)),
      ),
      child: Center(child: DeviceWidget(child: child)),
    );
  }
}

class LabeledDeviceText extends StatelessWidget {
  final String? name;

  const LabeledDeviceText(this.name, {super.key});

  @override
  Widget build(BuildContext context) {
    // put a SizedBox to keep the same height even if the FittedBox shrinks the width
    return SizedBox(
      height: 25.0,
      child: FittedBox(
        fit: BoxFit.contain,
        child: Text(
          name ?? "<unamed>",
          style: TextStyle(fontWeight: FontWeight.bold),
        ),
      ),
    );
  }
}

// XXX: The orientation stuff has no effect at the moment it's just here in case
// we want to come back to it
Orientation effectiveOrientation(BuildContext context) {
  // return Orientation.landscape;
  return Platform.isAndroid
      ? MediaQuery.of(context).orientation
      : Orientation.portrait;
}

// LHS: Override the label, RHS: Set the icon
typedef IconAssigner = (Widget?, Widget?) Function(BuildContext, DeviceId);

class DeviceListWithIcons extends StatelessWidget {
  const DeviceListWithIcons({super.key, required this.iconAssigner});
  final IconAssigner iconAssigner;

  @override
  Widget build(BuildContext context) {
    return DeviceList(deviceBuilder: _builder);
  }

  Widget _builder(
    BuildContext context, {
    required ConnectedDevice device,
    required Orientation orientation,
    required Animation<double> animation,
    String? previewName,
  }) {
    final (overrideLabel, icon) = iconAssigner.call(context, device.id);
    final label = overrideLabel ?? LabeledDeviceText(device.name);
    return DeviceBoxContainer(
      animation: animation,
      orientation: orientation,
      child: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        crossAxisAlignment: CrossAxisAlignment.center,
        children: icon != null
            ? [
                label,
                SizedBox(height: 4),
                SizedBox(height: iconSize, child: icon),
                SizedBox(height: 4),
              ]
            : [label],
      ),
    );
  }
}

Widget buildInteractiveDevice(
  BuildContext context, {
  required ConnectedDevice device,
  required Orientation orientation,
  required Animation<double> animation,
  String? previewName,
}) {
  final theme = Theme.of(context);
  final List<Widget> children = [];
  final upToDate = device.firmwareDigest == coord.upgradeFirmwareDigest();
  if (device.name == null) {
    children.add(Spacer(flex: 6));
  } else {
    children.add(LabeledDeviceText(device.name!));
  }
  children.add(Spacer(flex: 3));
  final Widget interaction;
  if (upToDate) {
    if (device.name == null) {
      interaction = TextButton(
        style: ElevatedButton.styleFrom(
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(4.0), // Rectangular shape
          ),
        ),
        onPressed: () async {
          await Navigator.push(
            context,
            MaterialPageRoute(
              builder: (context) {
                return DeviceSetup(id: device.id);
              },
            ),
          );
        },
        child: Text(
          'Fresh Device',
          style: theme.textTheme.bodyMedium?.copyWith(
            fontWeight: FontWeight.bold,
          ),
          textAlign: TextAlign.center,
        ),
      );
    } else {
      interaction = IconButton(
        icon: Icon(Icons.settings),
        onPressed: () {
          Navigator.push(
            context,
            MaterialPageRoute(
              builder: (context) => DeviceSettings(id: device.id),
            ),
          );
        },
      );
    }
  } else {
    interaction = Column(
      mainAxisAlignment: MainAxisAlignment.center,
      mainAxisSize: MainAxisSize.max,
      children: [
        IconButton.outlined(
          onPressed: () {
            FirmwareUpgradeDialog.show(context);
          },
          icon: Icon(Icons.upgrade),
        ),
        SizedBox(height: 6.0),
        Text("Upgrade", style: theme.textTheme.bodyMedium),
        Text("Firmware", style: theme.textTheme.bodyMedium),
      ],
    );
  }
  children.add(interaction);
  children.add(Spacer(flex: 10));
  return DeviceBoxContainer(
    orientation: orientation,
    animation: animation,
    child: Column(mainAxisSize: MainAxisSize.max, children: children),
  );
}

class DeviceListPage extends StatefulWidget {
  late final Iterable<WalletItem> walletList;

  DeviceListPage();

  @override
  State<DeviceListPage> createState() => _DeviceListPageState();
}

class _DeviceListPageState extends State<DeviceListPage> {
  final _scrollController = ScrollController();
  final _upgradeController = DeviceActionUpgradeController();

  @override
  void initState() {
    super.initState();
  }

  @override
  void dispose() {
    _scrollController.dispose();
    _upgradeController.dispose();
    super.dispose();
  }

  Widget _buildDevice(BuildContext context, ConnectedDevice device) {
    final theme = Theme.of(context);
    final homeCtx = HomeContext.of(context)!;
    final needsUpgrade = device.needsFirmwareUpgrade();
    final walletName = coord
        .frostKeysInvolvingDevice(deviceId: device.id)
        .map((key) => key.keyName())
        .firstOrNull;
    final hasWallet = walletName != null;
    final hasKey = device.name != null;

    return Card.filled(
      margin: EdgeInsets.symmetric(vertical: 8),
      color: theme.colorScheme.surfaceContainerHigh,
      clipBehavior: Clip.hardEdge,
      child: ListTile(
        title: Text(
          device.name ?? 'Unnamed',
          style: monospaceTextStyle.copyWith(
            color: hasKey ? null : theme.disabledColor,
          ),
        ),
        subtitle: Text(
          device.name == null
              ? '~'
              : walletName == null
              ? 'Wallet available for recovery'
              : walletName,
          style: TextStyle(
            color: hasKey && hasWallet ? null : theme.disabledColor,
          ),
        ),
        leading: Icon(Icons.key),
        trailing: Row(
          mainAxisSize: MainAxisSize.min,
          spacing: 8,
          children: [
            if (needsUpgrade)
              Icon(Icons.system_update_alt, color: theme.colorScheme.primary),
            Icon(Icons.chevron_right),
          ],
        ),
        contentPadding: EdgeInsets.symmetric(horizontal: 16),
        onTap: () async => await showBottomSheetOrDialog(
          context,
          title: Text('Device Details'),
          builder: (context, controller) => homeCtx.wrap(
            DeviceDetails(
              deviceId: device.id,
              firmwareUpgrade: _upgradeController.run,
            ),
          ),
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final windowSize = WindowSizeContext.of(context);
    final isFullscreen = windowSize == WindowSizeClass.compact;

    const titleText = 'Connected Devices';
    final header = isFullscreen
        ? SliverAppBar.large(title: Text(titleText), pinned: true)
        : SliverPinnedHeader(
            child: TopBar(
              title: Text(titleText),
              scrollController: _scrollController,
            ),
          );

    final scrollView = CustomScrollView(
      controller: _scrollController,
      shrinkWrap: !isFullscreen,
      slivers: [
        header,
        SliverToBoxAdapter(
          child: AnimatedSize(
            duration: Durations.long1,
            curve: Curves.easeInOutCubicEmphasized,
            child: ListenableBuilder(
              listenable: _upgradeController,
              builder: (context, _) {
                final count = _upgradeController.count;
                return count > 0
                    ? Padding(
                        padding: const EdgeInsets.only(bottom: 8),
                        child: ListTile(
                          title: Text(
                            'Upgrade $count device${count > 1 ? 's' : ''}',
                          ),
                          leading: Icon(Icons.system_update_alt),
                          trailing: Icon(Icons.chevron_right_rounded),
                          contentPadding: EdgeInsets.symmetric(horizontal: 24),
                          textColor: theme.colorScheme.primary,
                          iconColor: theme.colorScheme.primary,
                          onTap: () async =>
                              await _upgradeController.run(context),
                        ),
                      )
                    : SizedBox.shrink();
              },
            ),
          ),
        ),
        SliverPadding(
          padding: EdgeInsets.symmetric(horizontal: 16).copyWith(bottom: 16),
          sliver: SliverDeviceList(
            deviceBuilder: _buildDevice,
            noDeviceWidget: Padding(
              padding: EdgeInsets.symmetric(vertical: 40),
              child: Center(
                heightFactor: 2.1,
                child: Column(
                  spacing: 12,
                  children: [
                    Icon(
                      Icons.sentiment_dissatisfied,
                      color: theme.colorScheme.onSurfaceVariant,
                      size: 64,
                    ),
                    Text(
                      'No devices connected',
                      style: theme.textTheme.titleMedium?.copyWith(
                        color: theme.colorScheme.onSurfaceVariant,
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ),
        ),
      ],
    );
    return SafeArea(child: scrollView);
  }
}
