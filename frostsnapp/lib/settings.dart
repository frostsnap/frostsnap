import 'dart:async';
import 'dart:io' show Platform;

import 'package:collection/collection.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnap/address.dart';
import 'package:frostsnap/access_structures.dart';
import 'package:frostsnap/backup_workflow.dart';
import 'package:frostsnap/bullet_list.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/device_action_fullscreen_dialog.dart';
import 'package:frostsnap/electrum_server_settings.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/logs.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/bitcoin.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/src/rust/api/settings.dart';
import 'package:frostsnap/theme.dart';
import 'package:rxdart/rxdart.dart';
import 'package:frostsnap/todo.dart';
import 'package:frostsnap/udev_setup.dart';
import 'package:frostsnap/wallet.dart';
import 'package:pretty_qr_code/pretty_qr_code.dart';
import 'package:frostsnap/stream_ext.dart';
import 'package:frostsnap/icons.dart';
import 'package:url_launcher/url_launcher.dart';
import 'package:flutter/gestures.dart';

const settingsMaxWidth = 580.0;

class SettingsContent extends StatelessWidget {
  final Widget child;

  const SettingsContent({super.key, required this.child});

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Container(
        constraints: BoxConstraints(maxWidth: settingsMaxWidth),
        child: child,
      ),
    );
  }
}

class SettingsContext extends InheritedWidget {
  final Settings settings;
  late final Stream<DeveloperSettings> developerSettings;
  late final Stream<ElectrumSettings> electrumSettings;
  late final Stream<DisplaySettings> displaySettings;
  late final List<(BitcoinNetwork, Stream<ChainStatus>)> chainStatuses;

  SettingsContext({super.key, required this.settings, required super.child}) {
    developerSettings = settings.subDeveloperSettings().toBehaviorSubject();
    electrumSettings = settings.subElectrumSettings().toBehaviorSubject();
    displaySettings = settings.subDisplaySettings().toBehaviorSubject();
    chainStatuses = [];
  }

  static SettingsContext? of(BuildContext context) {
    //
    return context.getInheritedWidgetOfExactType<SettingsContext>();
  }

  @override
  bool updateShouldNotify(SettingsContext oldWidget) {
    // updates are obtained through the streams
    return false;
  }

  Stream<ChainStatus> chainStatusStream(BitcoinNetwork network) {
    final stream = chainStatuses.firstWhereOrNull((record) {
      return record.$1.name() == network.name();
    })?.$2;

    if (stream == null) {
      final stream = this.settings
          .subscribeChainStatus(network: network)
          .toBehaviorSubject();
      this.chainStatuses.add((network, stream));
      return stream;
    } else {
      return stream;
    }
  }

  /// Returns existing chain status stream if one has already been created,
  /// without triggering a new connection.
  Stream<ChainStatus>? maybeGetChainStatusStream(BitcoinNetwork network) {
    return chainStatuses.firstWhereOrNull((record) {
      return record.$1.name() == network.name();
    })?.$2;
  }

  Wallet? loadWallet({required KeyId keyId}) {
    final frostKey = coord.getFrostKey(keyId: keyId);
    if (frostKey == null) {
      return null;
    }
    final masterAppkey = frostKey.masterAppkey();
    final network = frostKey.bitcoinNetwork();
    if (network == null) {
      return null;
    }
    final superWallet = settings.getSuperWallet(network: network);
    return Wallet(superWallet: superWallet, masterAppkey: masterAppkey);
  }
}

class SettingsPage extends StatelessWidget {
  const SettingsPage({super.key});

  @override
  Widget build(BuildContext context) {
    final walletCtx = WalletContext.of(context);
    final logCtx = FrostsnapContext.of(context);

    return Scaffold(
      appBar: AppBar(title: Text('Settings')),
      body: SettingsContent(
        child: ListView(
          children: [
            if (walletCtx != null)
              SettingsCategory(
                title: "Wallet",
                items: [
                  SettingsItem(
                    title: Text('External wallet'),
                    icon: Icons.qr_code,
                    bodyBuilder: (context) {
                      final wallet = walletCtx.wallet;
                      return ExportDescriptorPage(
                        walletDescriptor: walletCtx.network.descriptorForKey(
                          masterAppkey: wallet.masterAppkey,
                        ),
                      );
                    },
                  ),
                  SettingsItem(
                    title: Text("Keys"),
                    icon: Icons.key_sharp,
                    bodyBuilder: (context) {
                      return KeysSettings();
                    },
                  ),
                  SettingsItem(
                    title: Text('Backup Checklist'),
                    icon: Icons.assignment,
                    bodyBuilder: (context) {
                      final frostKey = walletCtx.wallet.frostKey();
                      if (frostKey != null) {
                        return BackupChecklist(
                          accessStructure: frostKey.accessStructures()[0],
                        );
                      }
                    },
                  ),
                  SettingsItem(
                    title: Text('Check address'),
                    icon: Icons.policy,
                    bodyBuilder: (context) {
                      return CheckAddressPage();
                    },
                  ),
                  SettingsItem(
                    title: Text(
                      "Delete wallet",
                      style: TextStyle(color: Colors.redAccent),
                    ),
                    icon: Icons.delete_forever,
                    bodyBuilder: (context) {
                      return DeleteWalletPage();
                    },
                    onClose: () {
                      if (context.mounted) {
                        Navigator.pop(context);
                      }
                    },
                  ),
                ],
              ),
            SettingsCategory(
              title: 'General',
              items: [
                SettingsItem(
                  title: Text('About'),
                  icon: Icons.info_outline,
                  bodyBuilder: (context) {
                    return AboutPage();
                  },
                ),
                SettingsItem(
                  title: Text('Electrum server'),
                  icon: Icons.cloud,
                  bodyBuilder: (context) {
                    return ElectrumServerSettingsPage();
                  },
                ),
                if (Platform.isLinux)
                  SettingsItem(
                    title: Text('USB device setup'),
                    icon: Icons.usb,
                    bodyBuilder: (context) {
                      return UdevSetupPage();
                    },
                  ),
              ],
            ),
            SettingsCategory(
              title: 'Advanced',
              items: [
                if (logCtx != null)
                  SettingsItem(
                    title: Text("Logs"),
                    icon: Icons.list_alt,
                    bodyBuilder: (context) {
                      return LogPane(logStream: logCtx.logStream);
                    },
                  ),
                SettingsItem(
                  title: Text("Developer mode"),
                  icon: Icons.developer_mode,
                  builder: (context, title, icon) {
                    final settingsCtx = SettingsContext.of(context)!;
                    return StreamBuilder(
                      stream: settingsCtx.developerSettings,
                      builder: (context, snap) {
                        return Tooltip(
                          message: "enables wallets on Bitcoin test networks",
                          child: SwitchListTile(
                            title: title,
                            onChanged: (value) async {
                              await settingsCtx.settings.setDeveloperMode(
                                value: value,
                              );
                            },
                            value: snap.data?.developerMode ?? false,
                          ),
                        );
                      },
                    );
                  },
                ),
                SettingsItem(
                  title: Text(
                    "Erase multiple devices",
                    style: TextStyle(color: Colors.redAccent),
                  ),
                  icon: Icons.delete_sweep,
                  builder: (context, title, icon) {
                    final settingsCtx = SettingsContext.of(context)!;
                    return StreamBuilder(
                      stream: settingsCtx.developerSettings,
                      builder: (context, snap) {
                        final isDeveloperMode =
                            snap.data?.developerMode ?? false;

                        if (!isDeveloperMode) {
                          return SizedBox.shrink();
                        }

                        return ListTile(
                          contentPadding: const EdgeInsets.symmetric(
                            horizontal: 16,
                          ),
                          leading: Icon(icon, color: Colors.redAccent),
                          title: title,
                          onTap: () async {
                            await _showEraseAllDialog(context);
                          },
                        );
                      },
                    );
                  },
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

class FsAppBar extends StatelessWidget implements PreferredSizeWidget {
  final Widget title;
  final List<Widget> actions;
  final PreferredSizeWidget? bottom;
  final Color? backgroundColor;
  final bool? centerTitle;

  const FsAppBar({
    super.key,
    required this.title,
    this.bottom,
    this.backgroundColor,
    this.centerTitle,
    this.actions = const [],
  });

  @override
  Widget build(BuildContext context) {
    final walletCtx = WalletContext.of(context);
    final settingsCtx = SettingsContext.of(context)!;

    return AppBar(
      title: title,
      centerTitle: centerTitle,
      bottom: bottom,
      backgroundColor: backgroundColor,
      surfaceTintColor: backgroundColor,
      actions: [
        ...actions,
        if (walletCtx != null)
          StreamBuilder(
            stream: settingsCtx.chainStatusStream(walletCtx.network),
            builder: (context, snap) {
              if (!snap.hasData) {
                return SizedBox();
              }
              final chainStatus = snap.data!;
              return ChainStatusIcon(chainStatus: chainStatus);
            },
          ),
        IconButton(
          icon: Icon(Icons.settings),
          onPressed: () {
            Navigator.push(
              context,
              MaterialPageRoute(
                builder: (context) {
                  Widget page = SettingsPage();
                  page = walletCtx?.wrap(page) ?? page;
                  return page;
                },
              ),
            );
          },
        ),
      ],
    );
  }

  @override
  Size get preferredSize => Size.fromHeight(kToolbarHeight);
}

class SettingsCategory extends StatelessWidget {
  final String title;
  final List<SettingsItem> items;

  const SettingsCategory({super.key, required this.title, required this.items});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.only(bottom: 8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.center,
        children: [
          ListTile(
            title: Text(
              title,
              style: TextStyle(
                color: theme.colorScheme.secondary,
                fontWeight: FontWeight.bold,
              ),
            ),
            dense: true,
          ),
          ...items.map((item) {
            if (item.builder != null) {
              return item.builder!.call(context, item.title, item.icon);
            } else {
              return ListTile(
                contentPadding: const EdgeInsets.symmetric(horizontal: 16),
                leading: Icon(item.icon),
                title: item.title,
                trailing: Icon(Icons.chevron_right),
                onTap: () async {
                  final walletContext = WalletContext.of(context);
                  final keyContext = KeyContext.of(context);
                  await Navigator.push(
                    context,
                    PageRouteBuilder(
                      pageBuilder: (context, animation, secondaryAnimation) {
                        Widget body =
                            item.bodyBuilder?.call(context) ?? SizedBox();
                        if (walletContext != null) {
                          body = walletContext.wrap(body);
                        } else if (keyContext != null) {
                          body = keyContext.wrap(body);
                        }
                        return Scaffold(
                          appBar: AppBar(title: item.title),
                          body: body,
                        );
                      },
                      transitionsBuilder:
                          (context, animation, secondaryAnimation, child) {
                            const begin = Offset(1.0, 0.0);
                            const end = Offset.zero;
                            const curve = Curves.ease;

                            var tween = Tween(
                              begin: begin,
                              end: end,
                            ).chain(CurveTween(curve: curve));

                            return SlideTransition(
                              position: animation.drive(tween),
                              child: child,
                            );
                          },
                    ),
                  );
                  item.onClose?.call();
                },
              );
            }
          }),
        ],
      ),
    );
  }
}

class SettingsItem {
  final Widget title;
  final IconData icon;
  final Function(BuildContext)? bodyBuilder;
  final Function(BuildContext, Widget title, IconData icon)? builder;
  final Function()? onClose;

  SettingsItem({
    required this.title,
    required this.icon,
    this.onClose,
    this.bodyBuilder,
    this.builder,
  });
}

class ExportDescriptorPage extends StatefulWidget {
  final String walletDescriptor;

  const ExportDescriptorPage({super.key, required this.walletDescriptor});

  @override
  State<ExportDescriptorPage> createState() => _ExportDescriptorPageState();
}

class _ExportDescriptorPageState extends State<ExportDescriptorPage>
    with SingleTickerProviderStateMixin {
  bool _showQrCode = false;
  bool _signingOnly = false;
  late AnimationController _controller;
  late Animation<double> _fadeAnimation;

  @override
  void initState() {
    super.initState();
    _controller = AnimationController(
      duration: Duration(milliseconds: 600),
      vsync: this,
    );
    _fadeAnimation = CurvedAnimation(parent: _controller, curve: Curves.easeIn);
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  void _toggleQrCode(bool value) {
    setState(() {
      _showQrCode = value;
      if (_showQrCode) {
        _controller.forward();
      } else {
        _signingOnly = false;
        _controller.reverse();
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    final qrCode = QrCode(8, QrErrorCorrectLevel.L);
    qrCode.addData(widget.walletDescriptor);
    final qrImage = QrImage(qrCode);
    return Padding(
      padding: const EdgeInsets.all(16.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.center,
        children: [
          Text(
            "To show the wallet’s descriptor put it into “externally managed mode”",
            style: Theme.of(context).textTheme.bodyLarge,
          ),
          SizedBox(height: 16.0),
          SwitchListTile(
            title: Text('Externally managed mode'),
            subtitle: Text(
              _showQrCode
                  ? "This wallet is being managed by an external app"
                  : "This wallet is being managed by this app only",
            ),
            value: _showQrCode,
            onChanged: _toggleQrCode,
          ),
          SizedBox(height: 10.0),
          SwitchListTile(
            title: Text("Signing only mode"),
            subtitle: Text(
              _signingOnly
                  ? "This app's wallet is disabled"
                  : "This app is managing a wallet in addition to the external wallet",
            ),
            value: _signingOnly,
            onChanged: _showQrCode
                ? (value) {
                    setState(() {
                      _signingOnly = value;
                    });
                  }
                : null,
          ),
          SizedBox(height: 16.0),
          if (_showQrCode)
            FadeTransition(
              opacity: _fadeAnimation,
              child: Column(
                children: [
                  Container(
                    constraints: BoxConstraints(maxWidth: 300),
                    padding: EdgeInsets.all(16.0),
                    decoration: BoxDecoration(
                      color: Colors.white,
                      borderRadius: BorderRadius.circular(8.0),
                      boxShadow: const [
                        BoxShadow(
                          color: Colors.black12,
                          blurRadius: 8.0,
                          spreadRadius: 2.0,
                        ),
                      ],
                    ),
                    child: PrettyQrView(
                      qrImage: qrImage,
                      decoration: const PrettyQrDecoration(
                        shape: PrettyQrSmoothSymbol(),
                      ),
                    ),
                  ),
                  SizedBox(height: 16.0),
                  ElevatedButton.icon(
                    icon: Icon(Icons.copy),
                    label: Text('Copy to Clipboard'),
                    onPressed: () {
                      Clipboard.setData(
                        ClipboardData(text: widget.walletDescriptor),
                      );
                      ScaffoldMessenger.of(context).showSnackBar(
                        SnackBar(
                          content: Text('Descriptor copied to clipboard'),
                        ),
                      );
                    },
                  ),
                ],
              ),
            ),
          Todo("""
              We're meant to save this as a setting somewhere in the sql database.
              Showing the descriptor should change the syncing mode so that it syncs
              past the revelation index in case the user has given out addresses on
              another system.
              """),
        ],
      ),
    );
  }
}

class SetNetworkPage extends StatelessWidget {
  const SetNetworkPage({super.key});

  @override
  Widget build(BuildContext context) {
    return Todo(
      "A page that lets you choose which network the wallet will be on",
    );
  }
}

class ChainStatusIcon extends StatelessWidget {
  final ChainStatus chainStatus;

  const ChainStatusIcon({super.key, required this.chainStatus});

  @override
  Widget build(BuildContext context) {
    IconData iconData;
    Color iconColor;
    final double iconSize = IconTheme.of(context).size ?? 30.0;
    final String statusName;
    final theme = Theme.of(context);

    final currentUrl = chainStatus.onBackup
        ? chainStatus.backupUrl
        : chainStatus.primaryUrl;

    switch (chainStatus.state) {
      case ChainStatusState.connected:
        statusName = "Connected";
        iconData = Icons.link;
        iconColor = theme.colorScheme.primary;
        break;
      case ChainStatusState.connecting:
        statusName = "Connecting";
        iconData = Icons.link_off;
        iconColor = theme.colorScheme.tertiary;
        break;
      case ChainStatusState.disconnected:
        statusName = "Disconnected";
        iconData = Icons.link_off;
        iconColor = theme.colorScheme.error;
        break;
      case ChainStatusState.idle:
        statusName = "Idle";
        iconData = Icons.link_off;
        iconColor = theme.colorScheme.outline;
        break;
    }

    final onBackup =
        chainStatus.state == ChainStatusState.connected && chainStatus.onBackup;

    return Tooltip(
      message: "$statusName: $currentUrl",
      child: Stack(
        clipBehavior: Clip.none,
        children: [
          IconButton(
            iconSize: iconSize,
            icon: Icon(iconData, color: iconColor),
            onPressed: () => _showServerStatusSheet(context),
          ),
          if (chainStatus.state == ChainStatusState.connecting)
            Positioned(
              bottom: 0,
              right: 0,
              child: SpinningSyncIcon.always(size: iconSize * 0.7),
            ),
          if (onBackup)
            Positioned(
              top: 2,
              right: 2,
              child: Container(
                padding: const EdgeInsets.all(2),
                decoration: BoxDecoration(
                  color: theme.colorScheme.tertiary,
                  shape: BoxShape.circle,
                ),
                child: Icon(
                  Icons.priority_high,
                  size: iconSize * 0.4,
                  color: theme.colorScheme.onTertiary,
                ),
              ),
            ),
        ],
      ),
    );
  }

  void _showServerStatusSheet(BuildContext context) {
    final walletCtx = WalletContext.of(context);
    if (walletCtx == null) return;

    final network = walletCtx.superWallet.network;
    final settingsCtx = SettingsContext.of(context);
    if (settingsCtx == null) return;

    final combinedStream = Rx.combineLatest2(
      settingsCtx.chainStatusStream(network),
      settingsCtx.electrumSettings,
      (ChainStatus status, ElectrumSettings settings) {
        final server = settings.electrumServers.firstWhereOrNull(
          (s) => s.network == network,
        );
        return (
          status: status,
          enabled: server?.enabled ?? ElectrumEnabled.all,
        );
      },
    );

    showModalBottomSheet(
      context: context,
      builder: (sheetContext) => StreamBuilder(
        stream: combinedStream,
        builder: (context, snapshot) {
          final status = snapshot.data?.status ?? chainStatus;
          final enabled = snapshot.data?.enabled ?? ElectrumEnabled.all;
          final primaryEnabled = enabled != ElectrumEnabled.none;
          final backupEnabled = enabled == ElectrumEnabled.all;

          return SafeArea(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Padding(
                  padding: const EdgeInsets.fromLTRB(16, 16, 16, 8),
                  child: Text(
                    'Server Status',
                    style: Theme.of(context).textTheme.titleMedium?.copyWith(
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                ),
                _ServerStatusTile(
                  label: 'Primary Server',
                  url: status.primaryUrl,
                  status: _getServerStatusFor(status, false),
                  enabled: primaryEnabled,
                  onTap: primaryEnabled
                      ? () {
                          Navigator.pop(sheetContext);
                          settingsCtx.settings.connectTo(
                            network: network,
                            useBackup: false,
                          );
                        }
                      : null,
                  onEnabledChanged: (value) async {
                    final newEnabled = value
                        ? ElectrumEnabled.primaryOnly
                        : ElectrumEnabled.none;
                    await settingsCtx.settings.setElectrumEnabled(
                      network: network,
                      enabled: newEnabled,
                    );
                  },
                ),
                _ServerStatusTile(
                  label: 'Backup Server',
                  url: status.backupUrl,
                  status: _getServerStatusFor(status, true),
                  enabled: backupEnabled,
                  onTap: backupEnabled
                      ? () {
                          Navigator.pop(sheetContext);
                          settingsCtx.settings.connectTo(
                            network: network,
                            useBackup: true,
                          );
                        }
                      : null,
                  onEnabledChanged: primaryEnabled
                      ? (value) async {
                          final newEnabled = value
                              ? ElectrumEnabled.all
                              : ElectrumEnabled.primaryOnly;
                          await settingsCtx.settings.setElectrumEnabled(
                            network: network,
                            enabled: newEnabled,
                          );
                        }
                      : null,
                ),
                const SizedBox(height: 8),
              ],
            ),
          );
        },
      ),
    );
  }

  static ChainStatusState _getServerStatusFor(
    ChainStatus status,
    bool isBackup,
  ) {
    final state = status.state;
    if (state == ChainStatusState.idle) return ChainStatusState.idle;
    if (state == ChainStatusState.connected) {
      return status.onBackup == isBackup
          ? ChainStatusState.connected
          : ChainStatusState.idle;
    }
    if (state == ChainStatusState.connecting) {
      return ChainStatusState.connecting;
    }
    return state;
  }
}

class _ServerStatusTile extends StatelessWidget {
  final String label;
  final String url;
  final ChainStatusState status;
  final bool enabled;
  final VoidCallback? onTap;
  final ValueChanged<bool>? onEnabledChanged;

  const _ServerStatusTile({
    required this.label,
    required this.url,
    required this.status,
    required this.enabled,
    this.onTap,
    this.onEnabledChanged,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    final Color statusColor;
    if (!enabled) {
      statusColor = theme.colorScheme.outline;
    } else {
      switch (status) {
        case ChainStatusState.connected:
          statusColor = theme.colorScheme.primary;
          break;
        case ChainStatusState.connecting:
          statusColor = theme.colorScheme.tertiary;
          break;
        case ChainStatusState.disconnected:
          statusColor = theme.colorScheme.error;
          break;
        case ChainStatusState.idle:
          statusColor = theme.colorScheme.outline;
          break;
      }
    }

    return ListTile(
      leading: Container(
        width: 12,
        height: 12,
        decoration: BoxDecoration(shape: BoxShape.circle, color: statusColor),
      ),
      title: Text(label),
      subtitle: Text(url, maxLines: 1, overflow: TextOverflow.ellipsis),
      trailing: Switch(value: enabled, onChanged: onEnabledChanged),
      enabled: enabled,
      onTap: onTap,
    );
  }
}

class DeleteWalletPage extends StatelessWidget {
  const DeleteWalletPage({super.key});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final walletCtx = WalletContext.of(context);
    final keyId = KeyContext.of(context)!.keyId;
    final frostKey = coord.getFrostKey(keyId: keyId)!;

    final body = Padding(
      padding: EdgeInsets.all(16.0),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          DefaultTextStyle(
            textAlign: TextAlign.left,
            style: Theme.of(context).textTheme.titleMedium!,
            child: BulletList(const [
              Text(
                'This only deletes knowledge of the wallet from this app.',
                softWrap: true,
              ),
              Text(
                'No secret keys will be deleted from devices.',
                softWrap: true,
              ),
              Text(
                'The wallet can be restored from Frostsnap devices and/or backups.',
                softWrap: true,
              ),
            ]),
          ),
          SizedBox(height: 52),
          if (walletCtx != null)
            Card(
              shape: RoundedRectangleBorder(
                borderRadius: BorderRadiusGeometry.all(Radius.circular(28)),
              ),
              margin: EdgeInsets.zero,
              child: Padding(
                padding: EdgeInsets.all(16),
                child: Column(
                  spacing: 12,
                  children: [
                    Text(
                      frostKey.keyName(),
                      style: theme.textTheme.headlineSmall,
                    ),
                    StreamBuilder(
                      stream: walletCtx.txStream,
                      builder: (context, snapshot) => SatoshiText(
                        value: snapshot.data?.balance ?? 0,
                        style: theme.textTheme.headlineSmall,
                      ),
                    ),
                    // Hold-to-Delete Button
                    Center(
                      child: Padding(
                        padding: const EdgeInsets.all(16),
                        child: HoldToDeleteButton(
                          buttonText: Text(
                            style: TextStyle(fontWeight: FontWeight.bold),
                            softWrap: true,
                            textAlign: TextAlign.center,
                            "Hold to Delete",
                          ),
                          onComplete: () async {
                            await coord.deleteKey(keyId: keyId);
                            Navigator.popUntil(context, (r) => r.isFirst);
                            if (context.mounted) {
                              showDialog(
                                context: context,
                                barrierDismissible: false,
                                builder: (context) => AlertDialog(
                                  title: Text('Wallet Deleted'),
                                  content: Text(
                                    'The wallet has been successfully deleted.',
                                  ),
                                  actions: [
                                    TextButton(
                                      onPressed: () => Navigator.popUntil(
                                        context,
                                        (r) => r.isFirst,
                                      ),
                                      child: Text('Ok'),
                                    ),
                                  ],
                                ),
                              );
                            }
                          },
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ),
        ],
      ),
    );

    final scrollView = CustomScrollView(
      shrinkWrap: true,
      slivers: [
        TopBarSliver(
          title: Text('Delete wallet?'),
          leading: IconButton(
            icon: Icon(Icons.arrow_back),
            onPressed: () => Navigator.pop(context),
          ),
          showClose: false,
        ),
        SliverToBoxAdapter(child: body),
        SliverToBoxAdapter(child: SizedBox(height: 16)),
      ],
    );

    return SafeArea(child: scrollView);
  }
}

class HoldToDeleteButton extends StatefulWidget {
  final VoidCallback onComplete;
  final Widget buttonText;

  const HoldToDeleteButton({
    super.key,
    required this.onComplete,
    this.buttonText = const Text("Hold to Delete"),
  });

  @override
  State<HoldToDeleteButton> createState() => _HoldToDeleteButtonState();
}

class _HoldToDeleteButtonState extends State<HoldToDeleteButton> {
  double _progress = 0.0;
  Timer? _timer;
  bool _isPressed = false;

  void _startProgress() {
    setState(() {
      _isPressed = true;
    });
    const holdDuration = Duration(seconds: 3);
    const tick = Duration(milliseconds: 50);
    int ticks = holdDuration.inMilliseconds ~/ tick.inMilliseconds;
    int currentTick = 0;

    _timer = Timer.periodic(tick, (timer) {
      setState(() {
        currentTick++;
        _progress = currentTick / ticks;
        if (_progress >= 1.0) {
          _timer?.cancel();
          widget.onComplete();
        }
      });
    });
  }

  void _stopProgress() {
    _timer?.cancel();
    setState(() {
      _isPressed = false;
      _progress = 0.0;
    });
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return GestureDetector(
      onTapDown: (_) => _startProgress(),
      onTapUp: (_) => _stopProgress(),
      onTapCancel: () => _stopProgress(),
      child: AnimatedContainer(
        duration: Duration(milliseconds: 100),
        width: 120,
        height: 120,
        decoration: BoxDecoration(
          color: ElevationOverlay.applySurfaceTint(
            theme.colorScheme.error,
            Colors.black,
            _isPressed ? 10.0 : 0.0,
          ),
          shape: BoxShape.circle,
          boxShadow: _isPressed
              ? []
              : [
                  BoxShadow(
                    color: Colors.black26,
                    offset: Offset(0, 4),
                    blurRadius: 4.0,
                  ),
                ],
        ),
        child: Stack(
          alignment: Alignment.center,
          children: [
            SizedBox(
              width: 120,
              height: 120,
              child: CircularProgressIndicator(
                value: _progress,
                backgroundColor: theme.colorScheme.error,
                valueColor: AlwaysStoppedAnimation<Color>(
                  theme.colorScheme.onError,
                ),
              ),
            ),
            DefaultTextStyle(
              style: TextStyle(
                color: _isPressed
                    ? theme.colorScheme.onError
                    : theme.colorScheme.onError,
              ),
              child: widget.buttonText,
            ),
          ],
        ),
      ),
    );
  }
}

class KeysSettings extends StatelessWidget {
  const KeysSettings({super.key});

  @override
  Widget build(BuildContext context) {
    final keyCtx = KeyContext.of(context)!;
    final keyName = keyCtx.name;
    final keyId = keyCtx.keyId;

    final body = Padding(
      padding: EdgeInsets.all(16.0).copyWith(top: 0),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        spacing: 16,
        children: [
          Text("The ‘$keyName’ wallet can be unlocked with:"),
          StreamBuilder(
            stream: GlobalStreams.keyStateSubject,
            builder: (context, snap) {
              if (!snap.hasData) return SizedBox();
              final frostKey = snap.data!.keys.firstWhereOrNull(
                (frostkey) => keyIdEquals(frostkey.keyId(), keyId),
              );
              final accessStructures = frostKey?.accessStructures();
              return AccessStructureListWidget(
                accessStructures: accessStructures ?? [],
              );
            },
          ),
        ],
      ),
    );

    final scrollView = CustomScrollView(
      shrinkWrap: true,
      slivers: [
        TopBarSliver(
          title: Text('Keys'),
          leading: IconButton(
            icon: Icon(Icons.arrow_back),
            onPressed: () => Navigator.pop(context),
          ),
          showClose: false,
        ),
        SliverToBoxAdapter(child: body),
        SliverToBoxAdapter(child: SizedBox(height: 16)),
      ],
    );

    return SafeArea(child: scrollView);
  }
}

class BitcoinNetworkChooser extends StatelessWidget {
  final BitcoinNetwork value;
  final ValueChanged<BitcoinNetwork> onChanged;

  const BitcoinNetworkChooser({
    super.key,
    required this.value,
    required this.onChanged,
  });

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: [
        const SizedBox(height: 20),
        const Text("(developer) Choose the network:"),
        const SizedBox(height: 10),
        DropdownButton<String>(
          hint: const Text('Choose a network'),
          value: value.name(),
          onChanged: (String? newValue) {
            if (newValue != null) {
              final network = BitcoinNetwork.fromString(string: newValue)!;
              onChanged(network);
            }
          },
          items: BitcoinNetwork.supportedNetworks().map((network) {
            final name = network.name();
            return DropdownMenuItem<String>(
              value: name,
              child: Text(name == "bitcoin" ? "Bitcoin (BTC)" : network.name()),
            );
          }).toList(),
        ),
      ],
    );
  }
}

Future<void> _showEraseAllDialog(BuildContext context) async {
  final currentUpdate = await GlobalStreams.deviceListSubject.first;
  final devicesToErase = currentUpdate.state.devices
      // intentionally erasing all and not filtering out "blank"
      .map((device) => device.id)
      .toList();

  if (devicesToErase.isEmpty) {
    if (context.mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('Connect one or more devices to erase')),
      );
    }
    return;
  }

  late final FullscreenActionDialogController controller;

  controller = FullscreenActionDialogController(
    title: 'Erase Multiple Devices',
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
                'This will wipe all keys from devices.',
                style: TextStyle(fontWeight: FontWeight.bold),
              ),
              subtitle: Padding(
                padding: EdgeInsets.only(top: 6),
                child: Text(
                  'Devices will be rendered blank.\nThis action cannot be reverted, and the only way to restore keys is by loading their backups.',
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
      OutlinedButton(
        child: Text('Cancel'),
        onPressed: () async {
          await coord.sendCancelAll();
          await controller.clearAllActionsNeeded();
        },
      ),
      DeviceActionHint(),
    ],
    onDismissed: () async {
      await coord.sendCancelAll();
    },
  );

  // Listen for devices being wiped/disconnected and remove them from action list
  final subscription = GlobalStreams.deviceListChangeStream.listen((change) {
    if (change.kind == DeviceListChangeKind.removed) {
      controller.removeActionNeeded(change.device.id);
    }
  });

  // Show dialog and perform wipe
  final dialogFuture = controller.batchAddActionNeeded(context, devicesToErase);
  await coord.wipeAllDevices();

  // Wait for dialog to dismiss
  await dialogFuture;

  // Clean up
  await subscription.cancel();
  controller.dispose();
}

class AboutPage extends StatelessWidget {
  const AboutPage({super.key});

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.all(16.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Padding(
            padding: const EdgeInsets.symmetric(vertical: 16.0),
            child: Text.rich(
              TextSpan(
                style: Theme.of(context).textTheme.bodyLarge,
                children: [
                  TextSpan(text: 'Frostsnap is an '),
                  TextSpan(
                    text: 'open source',
                    style: TextStyle(
                      color: Theme.of(context).colorScheme.primary,
                      decoration: TextDecoration.underline,
                    ),
                    recognizer: TapGestureRecognizer()
                      ..onTap = () async {
                        final url = Uri.parse(
                          'https://github.com/frostsnap/frostsnap',
                        );
                        if (await canLaunchUrl(url)) {
                          await launchUrl(
                            url,
                            mode: LaunchMode.externalApplication,
                          );
                        }
                      },
                  ),
                  TextSpan(
                    text:
                        ' bitcoin wallet that uses FROST threshold signatures to secure your funds across multiple signing devices.',
                  ),
                ],
              ),
            ),
          ),
          Builder(
            builder: (context) {
              const buildVersion = String.fromEnvironment(
                'BUILD_VERSION',
                defaultValue: 'unknown',
              );

              return ListTile(
                leading: Icon(Icons.info_outline),
                title: Text('App version'),
                subtitle: Text(buildVersion, style: monospaceTextStyle),
                trailing: buildVersion != 'unknown'
                    ? Icon(Icons.copy, size: 20)
                    : null,
                onTap: buildVersion != 'unknown'
                    ? () {
                        Clipboard.setData(ClipboardData(text: buildVersion));
                        ScaffoldMessenger.of(context).showSnackBar(
                          SnackBar(
                            content: Text('Copied version to clipboard'),
                          ),
                        );
                      }
                    : null,
              );
            },
          ),
          Builder(
            builder: (context) {
              final firmwareVersion =
                  coord.upgradeFirmwareVersionName() ?? "No firmware bundled";
              final firmwareHash = coord.upgradeFirmwareDigest();
              final canCopy = firmwareHash != null;

              return ListTile(
                leading: Icon(Icons.memory),
                title: Text('Bundled firmware version'),
                subtitle: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Text(firmwareVersion, style: monospaceTextStyle),
                    if (firmwareHash != null) ...[
                      SizedBox(height: 4),
                      Text(
                        firmwareHash,
                        style: monospaceTextStyle.copyWith(fontSize: 11),
                      ),
                    ],
                  ],
                ),
                trailing: canCopy ? Icon(Icons.copy, size: 20) : null,
                onTap: canCopy
                    ? () {
                        Clipboard.setData(ClipboardData(text: firmwareHash));
                        ScaffoldMessenger.of(context).showSnackBar(
                          SnackBar(
                            content: Text('Copied firmware hash to clipboard'),
                          ),
                        );
                      }
                    : null,
              );
            },
          ),
          Builder(
            builder: (context) {
              const buildCommit = String.fromEnvironment(
                'BUILD_COMMIT',
                defaultValue: 'unknown',
              );
              final commitHash = buildCommit.endsWith('-modified')
                  ? buildCommit.substring(0, buildCommit.length - 9)
                  : buildCommit;

              return ListTile(
                leading: Icon(Icons.commit),
                title: Text('Build commit'),
                subtitle: Text(buildCommit, style: monospaceTextStyle),
                trailing: buildCommit != 'unknown'
                    ? Icon(Icons.more_horiz, size: 20)
                    : null,
                onTap: buildCommit != 'unknown'
                    ? () {
                        showModalBottomSheet(
                          context: context,
                          builder: (context) => SafeArea(
                            child: Column(
                              mainAxisSize: MainAxisSize.min,
                              children: [
                                ListTile(
                                  leading: Icon(Icons.copy),
                                  title: Text('Copy commit hash'),
                                  onTap: () {
                                    Clipboard.setData(
                                      ClipboardData(text: buildCommit),
                                    );
                                    Navigator.pop(context);
                                    ScaffoldMessenger.of(context).showSnackBar(
                                      SnackBar(
                                        content: Text(
                                          'Copied commit hash to clipboard',
                                        ),
                                      ),
                                    );
                                  },
                                ),
                                ListTile(
                                  leading: Icon(Icons.open_in_new),
                                  title: Text('View on GitHub'),
                                  onTap: () async {
                                    Navigator.pop(context);
                                    final url = Uri.parse(
                                      'https://github.com/frostsnap/frostsnap/commit/$commitHash',
                                    );
                                    if (await canLaunchUrl(url)) {
                                      await launchUrl(
                                        url,
                                        mode: LaunchMode.externalApplication,
                                      );
                                    }
                                  },
                                ),
                              ],
                            ),
                          ),
                        );
                      }
                    : null,
              );
            },
          ),
        ],
      ),
    );
  }
}
