import 'dart:async';
import 'dart:io';
import 'package:flutter/material.dart';
import 'package:frostsnap/device.dart';
import 'package:frostsnap/device_action_fullscreen_dialog.dart';
import 'package:frostsnap/device_settings.dart';
import 'package:frostsnap/device_setup.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/theme.dart';
import 'package:frostsnap/wallet_create.dart';
import 'package:frostsnap/wallet_device_list.dart';
import 'package:sliver_tools/sliver_tools.dart';
import 'global.dart';
import 'maybe_fullscreen_dialog.dart';
import 'wallet_list_controller.dart';

typedef RemovedDeviceBuilder =
    Widget Function(
      BuildContext context,
      ConnectedDevice device,
      Animation<double> animation,
    );

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
          'New Device',
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

  late final FullscreenActionDialogController<void> _upgradeController;
  final _needsUpgrade = ValueNotifier(0);
  final _upgradeProgress = ValueNotifier(FirmwareUpgradeState.empty());

  @override
  void initState() {
    super.initState();
    _upgradeController = FullscreenActionDialogController(
      title: 'Upgrade Firmware',
      body: (context) => Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        spacing: 12,
        children: [
          Card(
            margin: EdgeInsets.zero,
            child: ListTile(
              title: Text('Firmware Digest'),
              subtitle: Text(
                coord.upgradeFirmwareDigest() ?? '',
                style: monospaceTextStyle,
              ),
            ),
          ),
        ],
      ),
      actionButtons: [
        ValueListenableBuilder(
          valueListenable: _upgradeProgress,
          builder: (context, state, _) => switch (state.stage) {
            FirmwareUpgradeStage.Acks => OutlinedButton(
              child: Text('Cancel'),
              onPressed: () async => await coord.cancelProtocol(),
            ),
            FirmwareUpgradeStage.Progress => SizedBox.shrink(),
          },
        ),
        ValueListenableBuilder(
          valueListenable: _upgradeProgress,
          builder: (context, state, _) {
            final theme = Theme.of(context);
            return Row(
              mainAxisSize: MainAxisSize.min,
              spacing: 12,
              children: [
                ...switch (state.stage) {
                  FirmwareUpgradeStage.Acks => [
                    Text(
                      'Confirm on device',
                      style: theme.textTheme.labelMedium?.copyWith(
                        color: theme.colorScheme.onSurfaceVariant,
                      ),
                    ),
                    LargeCircularProgressIndicator(
                      size: 36,
                      progress: state.acks ?? 0,
                      total: state.neededAcks ?? 1,
                    ),
                  ],
                  FirmwareUpgradeStage.Progress => [
                    Text(
                      'Upgrading...',
                      style: theme.textTheme.labelMedium?.copyWith(
                        color: theme.colorScheme.onSurfaceVariant,
                      ),
                    ),
                    SizedBox(
                      width: 100,
                      child: LinearProgressIndicator(value: state.progress),
                    ),
                  ],
                },
              ],
            );
          },
        ),
      ],
    );
  }

  @override
  void dispose() {
    _scrollController.dispose();
    _upgradeController.dispose();
    _needsUpgrade.dispose();
    _upgradeProgress.dispose();
    _upgradeProgress.dispose();
    super.dispose();
  }

  Widget _buildDevice(BuildContext context, ConnectedDevice device) {
    final theme = Theme.of(context);
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
              ? 'Not recovered'
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
              Icon(Icons.warning_rounded, color: theme.colorScheme.primary),
            Icon(Icons.chevron_right),
          ],
        ),
        contentPadding: EdgeInsets.symmetric(horizontal: 16),
        onTap: () async => await showBottomSheetOrDialog(
          context,
          titleText: 'Device Details',
          builder: (context, controller) => DeviceDetails(
            deviceId: device.id,
            firmwareUpgrade: showUpgradeFirmwareDialog,
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
        ? SliverAppBar.large(
            title: Text(titleText),
            leading: IconButton(
              icon: Icon(Icons.close_rounded),
              onPressed: () => Navigator.pop(context),
            ),
            pinned: true,
          )
        : SliverPinnedHeader(
            child: TopBar(
              titleText: titleText,
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
            child: ValueListenableBuilder(
              valueListenable: _needsUpgrade,
              builder: (context, needsUpgrade, _) => needsUpgrade > 0
                  ? Padding(
                      padding: const EdgeInsets.only(bottom: 8),
                      child: ListTile(
                        title: Text(
                          'Upgrade $needsUpgrade device${needsUpgrade > 1 ? 's' : ''}',
                        ),
                        leading: Icon(Icons.warning_rounded),
                        trailing: Icon(Icons.chevron_right_rounded),
                        contentPadding: EdgeInsets.symmetric(horizontal: 24),
                        textColor: theme.colorScheme.primary,
                        iconColor: theme.colorScheme.primary,
                        onTap: () async => showUpgradeFirmwareDialog(),
                      ),
                    )
                  : SizedBox.shrink(),
            ),
          ),
        ),
        SliverPadding(
          padding: EdgeInsets.symmetric(horizontal: 16).copyWith(bottom: 16),
          sliver: SliverDeviceList(
            deviceBuilder: _buildDevice,
            onDeviceListChange: (state) => _needsUpgrade.value = state.devices
                .where((device) => device.needsFirmwareUpgrade())
                .length,
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

  Future<bool> showUpgradeFirmwareDialog() async {
    _upgradeProgress.value = FirmwareUpgradeState.empty();

    final upgradeStream = coord.startFirmwareUpgrade();

    coord.upgradeFirmwareDigest();

    await for (final state in upgradeStream) {
      _needsUpgrade.value = state.needUpgrade.length;
      _upgradeProgress.value = FirmwareUpgradeState.acks(
        neededAcks: state.needUpgrade.length,
        acks: state.confirmations.length,
      );

      for (final id in state.needUpgrade) {
        _upgradeController.addActionNeeded(context, id);
      }
      if (state.abort) {
        _upgradeController.clearAllActionsNeeded();
        return false;
      }
      if (state.upgradeReadyToStart) {
        break;
      }
    }

    final progressStream = coord.enterFirmwareUpgradeMode();
    var finalProgress = 0.0;
    await for (final progress in progressStream) {
      finalProgress = progress;
      _upgradeProgress.value = FirmwareUpgradeState.progress(
        progress: progress,
      );
    }

    _upgradeController.clearAllActionsNeeded();
    return finalProgress == 1.0;
  }
}

enum FirmwareUpgradeStage { Acks, Progress }

class FirmwareUpgradeState {
  final FirmwareUpgradeStage stage;
  final int? neededAcks;
  final int? acks;
  final double? progress;

  const FirmwareUpgradeState.empty()
    : stage = FirmwareUpgradeStage.Acks,
      neededAcks = null,
      acks = null,
      progress = null;

  const FirmwareUpgradeState.acks({required int neededAcks, required int acks})
    : stage = FirmwareUpgradeStage.Acks,
      progress = null,
      neededAcks = neededAcks,
      acks = acks;

  const FirmwareUpgradeState.progress({required double progress})
    : stage = FirmwareUpgradeStage.Progress,
      progress = progress,
      neededAcks = null,
      acks = null;

  @override
  bool operator ==(Object o) =>
      o is FirmwareUpgradeState &&
      o.stage == stage &&
      o.neededAcks == neededAcks &&
      o.acks == acks &&
      o.progress == progress;
}
