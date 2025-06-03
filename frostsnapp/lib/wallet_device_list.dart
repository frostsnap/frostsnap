import 'dart:async';
import 'package:flutter/material.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'global.dart';

typedef OnDeviceListChange = Function(DeviceListState);

typedef DeviceBuilder = Widget Function(BuildContext, ConnectedDevice);

class SliverDeviceList extends StatefulWidget {
  final OnDeviceListChange? onDeviceListChange;
  final DeviceBuilder deviceBuilder;
  final Widget? noDeviceWidget;

  const SliverDeviceList({
    super.key,
    required this.deviceBuilder,
    this.onDeviceListChange,
    this.noDeviceWidget,
  });

  @override
  State<SliverDeviceList> createState() => _SliverDeviceListState();
}

class _SliverDeviceListState extends State<SliverDeviceList> {
  final GlobalKey<SliverAnimatedListState> _listKey = GlobalKey();
  late DeviceListState _state;
  StreamSubscription? _stateSub;

  @override
  void initState() {
    super.initState();
    _state = coord.deviceListState();
    final onDeviceListChange = widget.onDeviceListChange;
    if (onDeviceListChange != null) onDeviceListChange(_state);
    _stateSub = GlobalStreams.deviceListUpdateStream.listen(_deviceListOnData);
  }

  @override
  void dispose() {
    _stateSub?.cancel();
    super.dispose();
  }

  void _deviceListOnData(DeviceListUpdate update) {
    final onDeviceListChange = widget.onDeviceListChange;
    if (onDeviceListChange != null) onDeviceListChange(update.state);

    final listState = _listKey.currentState;
    if (listState != null) {
      for (final change in update.changes) {
        switch (change.kind) {
          case DeviceListChangeKind.added:
            listState.insertItem(change.index);
          case DeviceListChangeKind.removed:
            listState.removeItem(
              change.index,
              (context, animation) =>
                  _buildItem(context, change.index, animation),
            );
          default:
          // Nothing to do.
        }
      }
    }

    if (mounted) setState(() => _state = update.state);
  }

  @override
  Widget build(BuildContext context) {
    return SliverAnimatedList(
      key: _listKey,
      itemBuilder: _buildItem,
      initialItemCount: _state.devices.length,
    );
  }

  Widget _buildItem(
    BuildContext context,
    int index,
    Animation<double> animation,
  ) {
    final device = _state.devices.elementAtOrNull(index);
    return SizeTransition(
      sizeFactor: animation.drive(
        CurveTween(curve: Curves.easeInOutCubicEmphasized),
      ),
      child: device == null
          ? SizedBox()
          : widget.deviceBuilder(context, device),
    );
  }
}
