import 'dart:io';
import 'package:flutter/material.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/device.dart';
import 'package:frostsnap/device_action_upgrade.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/theme.dart';
import 'package:frostsnap/wallet_device_list.dart';
import 'package:sliver_tools/sliver_tools.dart';
import 'global.dart';
import 'maybe_fullscreen_dialog.dart';
import 'wallet_list_controller.dart';

// XXX: The orientation stuff has no effect at the moment it's just here in case
// we want to come back to it
Orientation effectiveOrientation(BuildContext context) {
  // return Orientation.landscape;
  return Platform.isAndroid
      ? MediaQuery.of(context).orientation
      : Orientation.portrait;
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

    final upgradeEligibility = device.firmwareUpgradeEligibility();
    final walletName = coord
        .frostKeysInvolvingDevice(deviceId: device.id)
        .map((key) => key.keyName())
        .firstOrNull;
    final hasWallet = walletName != null;
    final hasKey = device.name != null;

    final caseColor = device.caseColor;

    // Explicitly type these as Color to avoid the "Object" error
    final Color bgColor =
        caseColor?.toColor() ?? theme.colorScheme.surfaceContainer;
    final Color activeColor =
        caseColor?.onColor(context) ?? theme.colorScheme.onSurface;

    // Disabled state should be more muted
    final Color disabledColor = activeColor.withValues(alpha: 0.45);

    final card = Card.filled(
      margin: EdgeInsets.zero,
      color: bgColor,
      clipBehavior: Clip.hardEdge,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(12),
        side: caseColor != null
            ? BorderSide(color: activeColor.withValues(alpha: 0.1), width: 1.5)
            : BorderSide.none,
      ),
      child: ListTile(
        leading: Icon(Icons.key, color: hasKey ? activeColor : disabledColor),
        title: Text(
          device.name ?? 'Unnamed',
          style: monospaceTextStyle.copyWith(
            color: hasKey ? activeColor : disabledColor,
            fontWeight: FontWeight.bold,
          ),
        ),
        subtitle: Text(
          device.name == null
              ? '~'
              : walletName == null
              ? 'Wallet available for recovery'
              : walletName,
          style: TextStyle(
            color: (hasKey && hasWallet)
                ? activeColor.withValues(alpha: 0.7)
                : disabledColor,
          ),
        ),
        trailing: Row(
          mainAxisSize: MainAxisSize.min,
          spacing: 8,
          children: [
            upgradeEligibility.when(
              upToDate: () => const SizedBox.shrink(),
              canUpgrade: () =>
                  Icon(Icons.system_update_alt, color: activeColor),
              cannotUpgrade: (reason) => Tooltip(
                message: reason,
                child: Icon(Icons.warning, color: activeColor),
              ),
            ),
            Icon(
              Icons.chevron_right,
              color: activeColor.withValues(alpha: 0.5),
            ),
          ],
        ),
        contentPadding: const EdgeInsets.symmetric(horizontal: 16),
        onTap: () async => await showBottomSheetOrDialog(
          context,
          title: const Text('Device Details'),
          builder: (context, controller) => homeCtx.wrap(
            DeviceDetails(
              deviceId: device.id,
              firmwareUpgrade: _upgradeController.run,
            ),
          ),
        ),
      ),
    );

    if (caseColor != null) {
      final (alpha, blur) = caseColor.glowSettings();
      return Container(
        margin: const EdgeInsets.symmetric(vertical: 8),
        decoration: BoxDecoration(
          borderRadius: BorderRadius.circular(12),
          boxShadow: [
            BoxShadow(
              // Use withAlpha if your glowSettings returns an 0-255 integer
              color: bgColor.withAlpha(alpha.toInt()),
              blurRadius: blur.toDouble(),
              spreadRadius: 2,
            ),
          ],
        ),
        child: card,
      );
    }

    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 8),
      child: card,
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
