import 'dart:async';

import 'package:collection/collection.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_svg/flutter_svg.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/device_action_fullscreen_dialog.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/theme.dart';

import 'global.dart';

class DeviceWidget extends StatelessWidget {
  final Widget child;
  const DeviceWidget({super.key, required this.child});
  static const String deviceSvg = '''
<svg width="34.986689421321785mm" height="42.99022935571762mm" viewBox="0 0 34.986689421321785 42.99022935571762" xmlns="http://www.w3.org/2000/svg" version="1.1">
<g id="Binder022" transform="translate(17.493344,21.543007) scale(1,-1)">
<path id="Binder022_f0000"  d="M -14.964990406185642 18.586987365833934 C -12.222647326109056 21.262466115746093 -6.102466677437631 21.177514052687993 0.0 21.19999999999999 C 6.121690939255835 21.173197742531492 12.231613033666312 21.274073642081408 14.969082389611895 18.60771394747341 C 17.192036356405605 16.42026948598908 17.145133620721403 12.729626455124311 17.150337936776445 9.214654998137103 C 17.15016857520842 9.062541610979817 17.149999999999988 8.910928801845651 17.15 8.759999999999998 L 17.15 -8.760000000000002 C 17.15 -8.914753765468582 17.150169136378064 -9.069446409953057 17.150338175634953 -9.2239777310766 C 17.142579739222196 -12.72727276368968 17.19842080113543 -16.18678348818877 15.049795021706236 -18.379036431181923 C 12.355533216618058 -21.090033156964946 6.171452934273059 -21.04122596621769 0.0023556717517863035 -21.0999930634531 C -6.101080496066673 -21.113188247010854 -12.221201449009136 -21.235110995509284 -14.9651450300972 -18.57374603173769 C -17.20075798684543 -16.36281066897254 -17.14189748249141 -12.78040602545065 -17.150337727621697 -9.223998902144816 C -17.1501688375326 -9.069320114984011 -17.14999999999999 -8.914616780668991 -17.15 -8.76 L -17.15 8.76 C -17.149999999999984 8.914615841852937 -17.150168835482276 9.06931823718443 -17.150337724548145 9.223997026763582 C -17.14207388097222 12.781031521903902 -17.20025878454716 16.36447394034848 -14.964990406185638 18.586987365833927 Z M -14.99999999999999 15.099999999999996 A 4 4 0 0 0 -11 19.1L 10.999999999999982 19.099999999999994 A 4 4 0 0 0 15 15.1L 14.999999999999991 -11.900000000000004 A 4 4 0 0 0 11 -15.9L -10.999999999999982 -15.900000000000002 A 4 4 0 0 0 -15 -11.9L -14.999999999999991 15.099999999999996 Z " stroke="#666666" stroke-width="0.35 px" style="stroke-width:0.35;stroke-miterlimit:4;stroke-dasharray:none;stroke-linecap:square;fill:#888888;fill-opacity:0.3;fill-rule: evenodd"/>
<title>b'Binder022'</title>
</g>
<g id="Binder023" transform="translate(17.493344,21.543007) scale(1,-1)">
<path id="Binder023_f0000"  d="M -10.999999999999979 19.099999999999998 A 4 4 0 0 1 -15 15.1L -15.0 -11.9 A 4 4 0 0 1 -11 -15.9L 10.999999999999982 -15.900000000000002 A 4 4 0 0 1 15 -11.9L 15.0 15.099999999999996 A 4 4 0 0 1 11 19.1L -10.99999999999998 19.099999999999998 Z " stroke="#666666" stroke-width="0.35 px" style="stroke-width:0.35;stroke-miterlimit:4;stroke-dasharray:none;stroke-linecap:square;fill:#888888;fill-opacity:0.1;fill-rule: evenodd"/>
<title>b'Binder023'</title>
</g>
</svg>
''';
  @override
  Widget build(BuildContext context) {
    const scale = 1.22;
    const height = 122.0 * scale;
    const width = 100.0 * scale;
    return SizedBox(
      width: width,
      height: height,
      child: Stack(
        alignment: Alignment.center,
        children: [
          SvgPicture.string(deviceSvg, width: width, height: height),
          Padding(
            padding: EdgeInsets.symmetric(
              vertical: 10 * scale,
              horizontal: 12 * scale,
            ),
            child: child,
          ),
        ],
      ),
    );
  }
}

class DevicePrompt extends StatelessWidget {
  final Widget icon;
  final String text;

  const DevicePrompt({super.key, required this.icon, required this.text});

  @override
  Widget build(BuildContext context) {
    return FittedBox(
      fit: BoxFit.contain,
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [icon, SizedBox(width: 4), Text(text)],
      ),
    );
  }
}

class ConfirmPrompt extends StatelessWidget {
  const ConfirmPrompt({super.key});

  @override
  Widget build(BuildContext context) {
    return DevicePrompt(icon: Icon(Icons.touch_app), text: "Confirm");
  }
}

class DeviceDetails extends StatefulWidget {
  final ScrollController? scrollController;
  final DeviceId deviceId;
  final Future<bool> Function(BuildContext context) firmwareUpgrade;

  const DeviceDetails({
    super.key,
    this.scrollController,
    required this.deviceId,
    required this.firmwareUpgrade,
  });

  @override
  State<DeviceDetails> createState() => _DeviceDetailsState();
}

class _DeviceDetailsState extends State<DeviceDetails> {
  late final StreamSubscription _sub;
  bool _showAdvanced = false;
  bool _gotFirstData = false;
  ConnectedDevice? _device;

  late final FullscreenActionDialogController<void> _eraseController;

  @override
  void initState() {
    super.initState();
    _sub = GlobalStreams.deviceListSubject.listen((
      DeviceListUpdate update,
    ) async {
      final device = update.state.devices.firstWhereOrNull(
        (device) => deviceIdEquals(device.id, widget.deviceId),
      );
      if (device?.name == null) await _eraseController.clearAllActionsNeeded();
      final _deviceWasSome = _device != null;
      final updateIsNone = device == null;
      if (_deviceWasSome && updateIsNone) {
        Navigator.pop(context);
        return;
      }
      setState(() {
        _gotFirstData = true;
        _device = device;
      });
    });
    _eraseController = FullscreenActionDialogController(
      title: 'Erase Device',
      body: (context) {
        final theme = Theme.of(context);
        return Card.filled(
          margin: EdgeInsets.zero,
          color: theme.colorScheme.errorContainer,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              ListTile(
                leading: Icon(Icons.warning_rounded),
                title: Text(
                  'This will wipe the key from the device.',
                  style: TextStyle(fontWeight: FontWeight.bold),
                ),
                subtitle: Padding(
                  padding: EdgeInsets.only(top: 6),
                  child: Text(
                    'The device will be rendered blank.\nThis action can not be reverted, and the only way to restore this key is by loading its backup.',
                  ),
                ),
                isThreeLine: true,
                textColor: theme.colorScheme.onErrorContainer,
                iconColor: theme.colorScheme.onErrorContainer,
                contentPadding: EdgeInsets.symmetric(horizontal: 16),
              ),
            ],
          ),
        );
      },
      actionButtons: [
        OutlinedButton(child: Text('Cancel'), onPressed: _onCancel),
        DeviceActionHint(),
      ],
      onDismissed: _onCancel,
    );
  }

  void _onCancel() async {
    final id = _device?.id;
    if (id != null) await coord.sendCancel(id: id);
    await _eraseController.clearAllActionsNeeded();
  }

  @override
  void dispose() {
    _sub.cancel();
    _eraseController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final device = _device;
    return CustomScrollView(
      controller: widget.scrollController,
      shrinkWrap: true,
      physics: ClampingScrollPhysics(),
      slivers: [
        if (_gotFirstData)
          SliverToBoxAdapter(
            child: device == null
                ? _buildDisconnectedWidget(context)
                : _buildColumn(context, device),
          ),
        SliverSafeArea(sliver: SliverToBoxAdapter(child: SizedBox(height: 12))),
      ],
    );
  }

  Widget _buildDisconnectedWidget(BuildContext context) {
    return Padding(
      padding: EdgeInsets.symmetric(vertical: 40),
      child: Center(heightFactor: 2.1, child: CircularProgressIndicator()),
    );
  }

  Widget _buildColumn(BuildContext context, ConnectedDevice device) {
    final theme = Theme.of(context);
    final homeCtx = HomeContext.of(context)!;
    final deviceName = device.name;
    final wallet = coord
        .frostKeysInvolvingDevice(deviceId: device.id)
        .firstOrNull;
    final isEmpty = deviceName == null;
    final hasWallet = wallet != null;
    final needsUpgrade = device.needsFirmwareUpgrade();
    final noncesAvailable = coord.noncesAvailable(id: device.id);

    final emptyRows = [
      ListTile(
        contentPadding: EdgeInsets.symmetric(horizontal: 16),
        title: Text('Fresh Device'),
        subtitle: Text('Can be used to create a wallet'),
        leading: Icon(Icons.ac_unit_rounded),
        enabled: false,
      ),
    ];

    final nonEmptyRows = [
      ListTile(
        contentPadding: EdgeInsets.symmetric(horizontal: 16),
        title: Text('Device Name'),
        subtitle: Text(
          deviceName ?? 'Unnamed',
          style: monospaceTextStyle.copyWith(
            color: device.name == null ? theme.disabledColor : null,
          ),
        ),
        leading: Icon(Icons.label_rounded),
        onTap: deviceName == null
            ? null
            : () => copyAction(context, 'Device name', deviceName),
      ),
      ListTile(
        contentPadding: EdgeInsets.symmetric(horizontal: 16),
        title: Text('Wallet'),
        subtitle: Text(
          wallet?.keyName() ?? 'Available for recovery',
          style: monospaceTextStyle.copyWith(
            color: hasWallet ? null : theme.disabledColor,
          ),
        ),
        leading: Icon(Icons.wallet_rounded),
        trailing: hasWallet ? Icon(Icons.chevron_right_rounded) : null,
        onTap: hasWallet
            ? () {
                Navigator.popUntil(context, (r) => r.isFirst);
                homeCtx.walletListController.selectWallet(wallet.keyId());
              }
            : null,
      ),
    ];

    final advancedHidden = [
      ListTile(
        contentPadding: EdgeInsets.symmetric(horizontal: 16),
        title: Text('Device ID'),
        subtitle: Text(
          device.id.toHex(),
          overflow: TextOverflow.ellipsis,
          style: monospaceTextStyle,
        ),
        leading: Icon(Icons.fingerprint_rounded),
        onTap: () => copyAction(context, 'Device ID', device.id.toHex()),
      ),
      ListTile(
        contentPadding: EdgeInsets.symmetric(horizontal: 16),
        title: Text('Nonces'),
        subtitle: Text('$noncesAvailable'),
        leading: Icon(Icons.numbers_rounded),
        onTap: () =>
            copyAction(context, 'Remaining nonces', '$noncesAvailable'),
      ),
      ListTile(
        contentPadding: EdgeInsets.symmetric(horizontal: 16),
        title: Text('Erase device'),
        subtitle: Text('Delete everything from this device'),
        leading: Icon(Icons.delete_forever_rounded),
        trailing: TextButton(
          onPressed: () => showEraseDialog(context, device.id),
          child: Text('Erase'),
          style: TextButton.styleFrom(foregroundColor: theme.colorScheme.error),
        ),
      ),
    ];

    final advancedRows = [
      ListTile(
        contentPadding: EdgeInsets.symmetric(horizontal: 16),
        title: Text('Advanced'),
        trailing: Icon(
          _showAdvanced ? Icons.expand_less_rounded : Icons.expand_more_rounded,
        ),
        onTap: () => setState(() => _showAdvanced = !_showAdvanced),
      ),
      AnimatedCrossFade(
        firstChild: Column(
          mainAxisSize: MainAxisSize.min,
          children: advancedHidden,
        ),
        secondChild: SizedBox(width: double.infinity),
        crossFadeState: _showAdvanced
            ? CrossFadeState.showFirst
            : CrossFadeState.showSecond,
        duration: Durations.medium2,
        sizeCurve: Curves.easeInOutCubicEmphasized,
      ),
    ];

    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        ...(isEmpty ? emptyRows : nonEmptyRows),
        ListTile(
          contentPadding: EdgeInsets.symmetric(horizontal: 16),
          leading: Icon(Icons.system_update_rounded),
          title: Text('Firmware'),
          subtitle: Text(
            device.firmwareDigest,
            style: monospaceTextStyle,
            overflow: TextOverflow.ellipsis,
          ),
          trailing: needsUpgrade
              ? TextButton.icon(
                  onPressed: () async => await widget.firmwareUpgrade(context),
                  label: Text('Upgrade'),
                )
              : Card.outlined(
                  child: Padding(
                    padding: const EdgeInsets.symmetric(
                      vertical: 4.0,
                      horizontal: 8.0,
                    ),
                    child: Text('Latest'),
                  ),
                ),
          onTap: () =>
              copyAction(context, "Device firmware", device.firmwareDigest),
        ),
        if (!isEmpty) ...advancedRows,
      ],
    );
  }

  copyAction(BuildContext context, String what, String data) {
    Clipboard.setData(ClipboardData(text: data));
    ScaffoldMessenger.of(
      context,
    ).showSnackBar(SnackBar(content: Text('$what copied to clipboard')));
  }

  void showEraseDialog(BuildContext context, DeviceId id) async {
    _eraseController.addActionNeeded(context, id);
    await coord.wipeDeviceData(deviceId: id);
  }
}
