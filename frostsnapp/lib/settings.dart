import 'dart:async';

import 'package:collection/collection.dart';
import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnapp/electrum_server.dart';
import 'package:frostsnapp/electrum_server_settings.dart';
import 'package:frostsnapp/logs.dart';
import 'package:frostsnapp/main.dart';
import 'package:frostsnapp/serialport.dart';
import 'package:frostsnapp/todo.dart';
import 'package:frostsnapp/wallet.dart';
import 'package:pretty_qr_code/pretty_qr_code.dart';
import 'package:frostsnapp/stream_ext.dart';
import 'package:frostsnapp/icons.dart';
import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';

class SettingsContext extends InheritedWidget {
  final Settings settings;
  late final Stream<WalletSettings> walletSettings;
  late final Stream<DeveloperSettings> developerSettings;
  late final Stream<ElectrumSettings> electrumSettings;
  late final List<(BitcoinNetwork, Stream<ChainStatus>)> chainStatuses;

  SettingsContext({
    super.key,
    required this.settings,
    required Widget child,
  }) : super(child: child) {
    walletSettings = settings.subWalletSettings().toBehaviorSubject();
    developerSettings = settings.subDeveloperSettings().toBehaviorSubject();
    electrumSettings = settings.subElectrumSettings().toBehaviorSubject();
    chainStatuses = [];
  }

  static SettingsContext? of(BuildContext context) {
    //
    return context.dependOnInheritedWidgetOfExactType<SettingsContext>();
  }

  @override
  bool updateShouldNotify(SettingsContext oldWidget) {
    // updates are obtained through the streams
    return false;
  }

  Stream<ChainStatus> chainStatusStream(BitcoinNetwork network) {
    Stream<ChainStatus>? stream = chainStatuses.firstWhereOrNull((record) {
      return record.$1.name() == network.name();
    })?.$2;

    if (stream == null) {
      stream =
          settings.subscribeChainStatus(network: network).toBehaviorSubject();
      chainStatuses.add((network, stream));
    }

    return stream;
  }
}

class SettingsPage extends StatelessWidget {
  final WalletContext? walletContext;

  const SettingsPage({super.key, required this.walletContext});

  @override
  Widget build(BuildContext context) {
    final logContext = FrostsnapContext.of(context);
    return Scaffold(
      appBar: AppBar(
        title: Text('Settings'),
      ),
      body: Center(
          child: Container(
              constraints: BoxConstraints(maxWidth: 600),
              child: ListView(
                padding: const EdgeInsets.all(16.0),
                children: [
                  if (walletContext != null)
                    SettingsCategory(title: "Wallet", items: [
                      SettingsItem(
                          title: 'External wallet',
                          icon: Icons.qr_code,
                          bodyBuilder: (context) {
                            return ExportDescriptorPage(
                                walletDescriptor: walletContext!.wallet.network
                                    .descriptorForKey(
                                        keyId: walletContext!.keyId));
                          }),
                    ]),
                  SettingsCategory(
                    title: 'General',
                    items: [
                      SettingsItem(
                        title: 'Theme',
                        icon: Icons.color_lens,
                        bodyBuilder: (context) {
                          return Todo(
                              "theme settings like like currency denomination");
                        },
                      ),
                      SettingsItem(
                        title: 'Electrum server',
                        icon: Icons.cloud,
                        bodyBuilder: (context) {
                          return ElectrumServerSettingsPage();
                        },
                      ),
                    ],
                  ),
                  SettingsCategory(title: 'Advanced', items: [
                    if (logContext != null)
                      SettingsItem(
                          title: "Logs",
                          icon: Icons.list_alt,
                          bodyBuilder: (context) {
                            return LogPane(logStream: logContext!.logStream);
                          }),
                    SettingsItem(
                        title: "Developer mode",
                        icon: Icons.developer_mode,
                        builder: (context, title, icon) {
                          final settingsCtx = SettingsContext.of(context)!;
                          return StreamBuilder(
                              stream: settingsCtx.developerSettings,
                              builder: (context, snap) {
                                return Tooltip(
                                    message:
                                        "enables wallets on Bitcoin test networks",
                                    child: SwitchListTile(
                                      title: Text(title),
                                      onChanged: (value) async {
                                        await settingsCtx.settings
                                            .setDeveloperMode(value: value);
                                      },
                                      value: snap.data?.developerMode ?? false,
                                    ));
                              });
                        }),
                  ]),
                ],
              ))),
    );
  }
}

class FsAppBar extends StatelessWidget implements PreferredSizeWidget {
  final Widget title;
  final List<Widget> actions;

  const FsAppBar({super.key, required this.title, this.actions = const []});

  @override
  Widget build(BuildContext context) {
    final walletCtx = WalletContext.of(context);
    final settingsCtx = SettingsContext.of(context)!;

    return AppBar(
      title: title,
      actions: [
        ...actions,
        if (walletCtx != null)
          StreamBuilder(
              stream: settingsCtx.chainStatusStream(walletCtx.wallet.network),
              builder: (context, snap) {
                if (!snap.hasData) {
                  return SizedBox();
                }
                final chainStatus = snap.data!;
                return ChainStatusIcon(chainStatus: chainStatus);
              }),
        IconButton(
          icon: Icon(Icons.settings),
          onPressed: () {
            Navigator.push(
              context,
              MaterialPageRoute(
                  builder: (context) => SettingsPage(walletContext: walletCtx)),
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
    return Column(
      crossAxisAlignment: CrossAxisAlignment.center,
      children: [
        Text(
          title,
          style: Theme.of(context).textTheme.titleLarge,
          textAlign: TextAlign.center,
        ),
        SizedBox(height: 8.0),
        ...items.map((item) {
          if (item.builder != null) {
            return item.builder!.call(context, item.title, item.icon);
          } else {
            return ListTile(
              leading: Icon(item.icon),
              title: Text(item.title),
              trailing: Icon(Icons.chevron_right),
              onTap: () {
                Navigator.push(
                    context,
                    PageRouteBuilder(
                      pageBuilder: (context, animation, secondaryAnimation) {
                        return Scaffold(
                          appBar: AppBar(title: Text(item.title)),
                          body: item.bodyBuilder?.call(context) ?? SizedBox(),
                        );
                      },
                      transitionsBuilder:
                          (context, animation, secondaryAnimation, child) {
                        const begin = Offset(1.0, 0.0);
                        const end = Offset.zero;
                        const curve = Curves.ease;

                        var tween = Tween(begin: begin, end: end)
                            .chain(CurveTween(curve: curve));

                        return SlideTransition(
                          position: animation.drive(tween),
                          child: child,
                        );
                      },
                    ));
              },
            );
          }
        })
      ],
    );
  }
}

class SettingsItem {
  final String title;
  final IconData icon;
  final Function(BuildContext)? bodyBuilder;
  final Function(BuildContext, String title, IconData icon)? builder;

  SettingsItem(
      {required this.title,
      required this.icon,
      this.bodyBuilder,
      this.builder});
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
              subtitle: Text(_signingOnly
                  ? "This app's wallet is disabled"
                  : "This app is managing a wallet in addition to the external wallet"),
              value: _signingOnly,
              onChanged: _showQrCode
                  ? (value) {
                      setState(() {
                        _signingOnly = value;
                      });
                    }
                  : null),
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
                          ClipboardData(text: widget.walletDescriptor));
                      ScaffoldMessenger.of(context).showSnackBar(
                        SnackBar(
                            content: Text('Descriptor copied to clipboard')),
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
        "A page that lets you choose which network the wallet will be on");
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

    switch (chainStatus.state) {
      case ChainStatusState.Connected:
      case ChainStatusState.Syncing:
        statusName = "Connected";
        iconData = Icons.power;
        iconColor = Colors.green;
        break;
      case ChainStatusState.Connecting:
      case ChainStatusState.Disconnected:
        statusName = "Disconnected";
        iconData = Icons.power_off;
        iconColor = Colors.red;
        break;
    }

    return Tooltip(
      message: "$statusName: ${chainStatus.electrumUrl}",
      child: Stack(
        children: [
          Icon(
            iconData,
            color: iconColor,
            size: iconSize,
          ),
          if (chainStatus.state == ChainStatusState.Syncing ||
              chainStatus.state == ChainStatusState.Connecting)
            Positioned(
              bottom: 0,
              right: 0,
              child: SpinningSyncIcon.always(size: iconSize * 0.7),
            ),
        ],
      ),
    );
  }
}
