import 'dart:async';
import 'package:flutter/material.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:sliver_tools/sliver_tools.dart';

typedef DeviceBuilder = Widget Function(BuildContext, ConnectedDevice);

/// Animated sliver rendering the devices described by [updates], which defaults to
/// [GlobalStreams.deviceListSubject].
///
/// The first update received seeds the list from its `state`; its `changes` are ignored, so
/// [updates] may replay a cached update (e.g. a `BehaviorSubject`). Later updates animate their
/// `added`/`removed` changes and apply `named`/`recoveryMode`/`refreshed` in place.
class SliverDeviceList extends StatefulWidget {
  final Stream<DeviceListUpdate>? updates;
  final DeviceBuilder deviceBuilder;
  final Widget? noDeviceWidget;

  const SliverDeviceList({
    super.key,
    this.updates,
    required this.deviceBuilder,
    this.noDeviceWidget,
  });

  @override
  State<SliverDeviceList> createState() => _SliverDeviceListState();
}

class _SliverDeviceListState extends State<SliverDeviceList> {
  final GlobalKey<SliverAnimatedListState> _listKey = GlobalKey();
  List<ConnectedDevice>? _devices;
  StreamSubscription<DeviceListUpdate>? _sub;

  @override
  void initState() {
    super.initState();
    _sub = (widget.updates ?? GlobalStreams.deviceListSubject).listen(
      _onUpdate,
    );
  }

  @override
  void dispose() {
    _sub?.cancel();
    super.dispose();
  }

  void _onUpdate(DeviceListUpdate update) {
    if (!mounted) return;
    final devices = _devices;
    if (devices == null) {
      setState(() => _devices = List.of(update.state.devices));
      return;
    }
    if (update.changes.isEmpty) return;
    final listState = _listKey.currentState;
    for (final change in update.changes) {
      switch (change.kind) {
        case DeviceListChangeKind.added:
          devices.insert(change.index, change.device);
          listState?.insertItem(change.index);
        case DeviceListChangeKind.removed:
          // Captured now: once removed, `devices[change.index]` is whatever shifted into the slot.
          final removed = devices.removeAt(change.index);
          listState?.removeItem(
            change.index,
            (context, animation) => _buildItem(context, removed, animation),
          );
        case DeviceListChangeKind.named:
        case DeviceListChangeKind.recoveryMode:
        case DeviceListChangeKind.refreshed:
          devices[change.index] = change.device;
      }
    }
    setState(() {});
  }

  @override
  Widget build(BuildContext context) {
    final devices = _devices;
    if (devices == null) return const SliverToBoxAdapter();
    // The list stays mounted while empty so the 0⇄1 insert/remove still animates.
    return MultiSliver(
      children: [
        SliverAnimatedList(
          key: _listKey,
          initialItemCount: devices.length,
          itemBuilder: (context, index, animation) =>
              _buildItem(context, devices[index], animation),
        ),
        if (devices.isEmpty && widget.noDeviceWidget != null)
          SliverToBoxAdapter(child: widget.noDeviceWidget),
      ],
    );
  }

  Widget _buildItem(
    BuildContext context,
    ConnectedDevice device,
    Animation<double> animation,
  ) => SizeTransition(
    sizeFactor: animation.drive(
      CurveTween(curve: Curves.easeInOutCubicEmphasized),
    ),
    child: widget.deviceBuilder(context, device),
  );
}
