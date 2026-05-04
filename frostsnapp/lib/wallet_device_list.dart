import 'dart:async';
import 'package:flutter/material.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:sliver_tools/sliver_tools.dart';

typedef DeviceBuilder = Widget Function(BuildContext, ConnectedDevice);

/// Animated sliver that renders a list of devices driven entirely by
/// an externally-provided [updates] stream. Dumb renderer: does not
/// subscribe to globals or call `coord` itself.
///
/// Contract for [updates]:
/// - Emits [DeviceListUpdate]s where `state` is the authoritative
///   full list and `changes` is the delta that produced it.
/// - The first emission received seeds the rendered list from its
///   `state`. Its `changes` are ignored (already reflected in state),
///   so producers may replay a cached update without double-counting.
/// - Subsequent emissions animate: `added`/`removed` drive a slide
///   via [SliverAnimatedList]; `named`/`recoveryMode` update in place
///   and trigger a rebuild (no size animation).
/// - Should be a broadcast stream that replays the latest value to
///   late subscribers (e.g. a `BehaviorSubject<DeviceListUpdate>`).
///   `GlobalStreams.deviceListSubject` and
///   `DeviceSetupController.updates` both satisfy this.
///
/// Passing a different [updates] stream post-mount rebinds the
/// subscription and resets the list.
class SliverDeviceList extends StatefulWidget {
  const SliverDeviceList({
    super.key,
    required this.updates,
    required this.deviceBuilder,
    this.noDeviceWidget,
  });

  final Stream<DeviceListUpdate> updates;
  final DeviceBuilder deviceBuilder;
  final Widget? noDeviceWidget;

  @override
  State<SliverDeviceList> createState() => _SliverDeviceListState();
}

class _SliverDeviceListState extends State<SliverDeviceList> {
  final GlobalKey<SliverAnimatedListState> _listKey = GlobalKey();
  List<ConnectedDevice>? _current;
  StreamSubscription<DeviceListUpdate>? _sub;

  @override
  void initState() {
    super.initState();
    _sub = widget.updates.listen(_handleUpdate);
  }

  @override
  void didUpdateWidget(SliverDeviceList oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!identical(oldWidget.updates, widget.updates)) {
      _sub?.cancel();
      _current = null;
      _sub = widget.updates.listen(_handleUpdate);
    }
  }

  @override
  void dispose() {
    _sub?.cancel();
    super.dispose();
  }

  void _handleUpdate(DeviceListUpdate update) {
    if (!mounted) return;
    if (_current == null) {
      // Seed: state is ground truth; ignore this emission's changes
      // (they're already reflected in state).
      setState(() => _current = List.of(update.state.devices));
      return;
    }
    if (update.changes.isEmpty) return;
    final listState = _listKey.currentState;
    var rebuiltInPlace = false;
    for (final change in update.changes) {
      switch (change.kind) {
        case DeviceListChangeKind.added:
          _current!.insert(change.index, change.device);
          listState?.insertItem(change.index);
        case DeviceListChangeKind.removed:
          // Capture the outgoing device now so the exit animation
          // renders the row being removed, not whatever shifts into
          // its slot afterwards.
          final removed = _current!.removeAt(change.index);
          listState?.removeItem(
            change.index,
            (ctx, anim) => _buildItem(ctx, removed, anim),
          );
        case DeviceListChangeKind.named:
        case DeviceListChangeKind.recoveryMode:
          if (change.index < _current!.length) {
            _current![change.index] = change.device;
            rebuiltInPlace = true;
          }
      }
    }
    // In-place updates need a rebuild to re-run `itemBuilder`.
    // Insert/remove don't — `SliverAnimatedList` rebuilds itself on
    // `insertItem`/`removeItem`, and the widget-level placeholder
    // below (`noDeviceWidget`) only toggles, which `insert`/`remove`
    // already triggered through `_current.length`'s effect on our
    // build. Still, calling `setState` keeps the placeholder's
    // visibility in sync — and is a no-op if nothing else changed.
    if (rebuiltInPlace ||
        update.changes.any(
          (c) =>
              c.kind == DeviceListChangeKind.added ||
              c.kind == DeviceListChangeKind.removed,
        )) {
      setState(() {});
    }
  }

  Widget _buildItem(
    BuildContext context,
    ConnectedDevice? device,
    Animation<double> animation,
  ) => SizeTransition(
    sizeFactor: animation.drive(
      CurveTween(curve: Curves.easeInOutCubicEmphasized),
    ),
    child: device == null
        ? const SizedBox.shrink()
        : widget.deviceBuilder(context, device),
  );

  @override
  Widget build(BuildContext context) {
    final current = _current;
    if (current == null) {
      // Pre-first-emit placeholder. Once we seed `_current`, this
      // widget tree gets replaced below with a persistently-mounted
      // `SliverAnimatedList` so insert/remove animations work at
      // the 0 ⇄ 1 boundary (widget swap here would unmount the list
      // mid-animation).
      return SliverToBoxAdapter(child: widget.noDeviceWidget);
    }
    return MultiSliver(
      children: [
        SliverAnimatedList(
          key: _listKey,
          itemBuilder: (ctx, index, anim) =>
              _buildItem(ctx, current.elementAtOrNull(index), anim),
          initialItemCount: current.length,
        ),
        if (current.isEmpty && widget.noDeviceWidget != null)
          SliverToBoxAdapter(child: widget.noDeviceWidget),
      ],
    );
  }
}
