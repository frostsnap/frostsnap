import 'dart:async';

import 'package:flutter/material.dart';
import 'package:frostsnap/device_action_upgrade.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';

/// Cleared whenever a device is plugged in, so the dismissal means "not now"
/// rather than "not this run". A notifier rather than a bare field because it
/// is library-level: every mounted prompt should react, not only the one whose
/// button was pressed.
final _dismissed = ValueNotifier(false);

/// Belongs in the page's own widget tree rather than on the root navigator or
/// an overlay — every fullscreen device dialog is a root-navigator route, so
/// living below them is what keeps this from ever covering a device prompt.
/// Ordering it explicitly would be a rule to get wrong; this cannot be.
class FirmwareUpgradeBubble extends StatefulWidget {
  const FirmwareUpgradeBubble({super.key});

  @override
  State<FirmwareUpgradeBubble> createState() => _FirmwareUpgradeBubbleState();
}

class _FirmwareUpgradeBubbleState extends State<FirmwareUpgradeBubble> {
  static const _maxWidth = 460.0;

  final _upgrades = DeviceActionUpgradeController();
  StreamSubscription<DeviceListUpdate>? _connects;
  bool _upgrading = false;

  @override
  void initState() {
    super.initState();
    // The upgrade controller answers "how many can be upgraded", not "did
    // something just arrive", so this is a separate question rather than a
    // second copy of the same one.
    _connects = GlobalStreams.deviceListSubject.listen((update) {
      final plugged = update.changes.any(
        (change) => change.kind == DeviceListChangeKind.added,
      );
      if (plugged) _dismissed.value = false;
    });
  }

  @override
  void dispose() {
    _connects?.cancel();
    _upgrades.dispose();
    super.dispose();
  }

  Future<void> _upgrade() async {
    setState(() => _upgrading = true);
    try {
      await _upgrades.run(context);
    } finally {
      if (mounted) setState(() => _upgrading = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return Positioned(
      top: 0,
      left: 0,
      right: 0,
      // Deliberately over the page's own `SliverAppBar`, drawer button
      // included: LLFourn's call, on the grounds that having to dismiss it to
      // reach the tray is acceptable pressure to deal with the upgrade. Note
      // it is the drawer button it lands on, so dismissing is the only way
      // back to the tray while it is up.
      child: SafeArea(
        bottom: false,
        child: Align(
          alignment: Alignment.topCenter,
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: _maxWidth),
            child: ListenableBuilder(
              listenable: Listenable.merge([_upgrades, _dismissed]),
              builder: (context, _) {
                final count = _upgrades.count;
                final visible = count > 0 && !_dismissed.value && !_upgrading;
                final label = count == 1
                    ? 'A device can be upgraded'
                    : '$count devices can be upgraded';

                return IgnorePointer(
                  ignoring: !visible,
                  child: AnimatedSlide(
                    offset: visible ? Offset.zero : const Offset(0, -1.4),
                    duration: Durations.medium4,
                    curve: Curves.easeInOutCubicEmphasized,
                    child: AnimatedOpacity(
                      opacity: visible ? 1 : 0,
                      duration: Durations.short4,
                      child: Padding(
                        padding: const EdgeInsets.fromLTRB(12, 10, 12, 0),
                        child: Material(
                          // Raised a tier above the page and given a real
                          // shadow: this is the only thing on screen that is
                          // floating rather than laid out, and it should read
                          // that way.
                          color: theme.colorScheme.surfaceContainerHigh,
                          elevation: 6,
                          borderRadius: BorderRadius.circular(28),
                          child: Padding(
                            padding: const EdgeInsets.fromLTRB(18, 6, 6, 6),
                            child: Row(
                              children: [
                                Icon(
                                  Icons.system_update_rounded,
                                  size: 20,
                                  color: theme.colorScheme.primary,
                                ),
                                const SizedBox(width: 14),
                                Expanded(
                                  child: Text(
                                    label,
                                    style: theme.textTheme.bodyMedium,
                                    overflow: TextOverflow.ellipsis,
                                  ),
                                ),
                                const SizedBox(width: 8),
                                TextButton(
                                  onPressed: _upgrade,
                                  child: const Text('Upgrade'),
                                ),
                                IconButton(
                                  icon: const Icon(Icons.close_rounded),
                                  iconSize: 18,
                                  visualDensity: VisualDensity.compact,
                                  color: theme.colorScheme.onSurfaceVariant,
                                  tooltip: 'Dismiss',
                                  onPressed: () => _dismissed.value = true,
                                ),
                              ],
                            ),
                          ),
                        ),
                      ),
                    ),
                  ),
                );
              },
            ),
          ),
        ),
      ),
    );
  }
}
