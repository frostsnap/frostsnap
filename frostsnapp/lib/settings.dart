import 'dart:async';

import 'package:collection/collection.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnap/address.dart';
import 'package:frostsnap/access_structures.dart';
import 'package:frostsnap/backup_workflow.dart';
import 'package:frostsnap/bullet_list.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/electrum_server_settings.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/logs.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/bitcoin.dart';
import 'package:frostsnap/src/rust/api/settings.dart';
import 'package:frostsnap/todo.dart';
import 'package:frostsnap/wallet.dart';
import 'package:pretty_qr_code/pretty_qr_code.dart';
import 'package:frostsnap/stream_ext.dart';
import 'package:frostsnap/icons.dart';

class SettingsContext extends InheritedWidget {
  final Settings settings;
  late final Stream<DeveloperSettings> developerSettings;
  late final Stream<ElectrumSettings> electrumSettings;
  late final List<(BitcoinNetwork, Stream<ChainStatus>)> chainStatuses;

  SettingsContext({super.key, required this.settings, required super.child}) {
    developerSettings = settings.subDeveloperSettings().toBehaviorSubject();
    electrumSettings = settings.subElectrumSettings().toBehaviorSubject();
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
      body: Center(
        child: Container(
          constraints: BoxConstraints(maxWidth: 580),
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
                          final backupManager = FrostsnapContext.of(
                            context,
                          )!.backupManager;
                          return BackupChecklist(
                            backupManager: backupManager,
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
                    title: Text('Theme'),
                    icon: Icons.color_lens,
                    bodyBuilder: (context) {
                      return Todo(
                        "theme settings like like currency denomination",
                      );
                    },
                  ),
                  SettingsItem(
                    title: Text('Electrum server'),
                    icon: Icons.cloud,
                    bodyBuilder: (context) {
                      return ElectrumServerSettingsPage();
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
                ],
              ),
            ],
          ),
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
    final VoidCallback? onPressed;
    final theme = Theme.of(context);

    if (chainStatus.state == ChainStatusState.connected) {
      onPressed = () {
        WalletContext.of(context)?.superWallet.reconnect();
      };
    } else {
      onPressed = null;
    }

    switch (chainStatus.state) {
      case ChainStatusState.connected:
        statusName = "Connected";
        iconData = Icons.link_rounded;
        iconColor = theme.colorScheme.primary;
        break;
      case ChainStatusState.connecting:
      case ChainStatusState.disconnected:
        statusName = "Disconnected";
        iconData = Icons.link_off_rounded;
        iconColor = theme.colorScheme.error;
        break;
    }

    return Tooltip(
      message: "$statusName: ${chainStatus.electrumUrl}",
      child: Stack(
        children: [
          IconButton(
            iconSize: iconSize,
            icon: Icon(iconData, color: iconColor),
            onPressed: onPressed,
          ),
          if (chainStatus.state == ChainStatusState.connecting)
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

class DeleteWalletPage extends StatelessWidget {
  const DeleteWalletPage({super.key});

  @override
  Widget build(BuildContext context) {
    final walletCtx = WalletContext.of(context);
    final keyId = KeyContext.of(context)!.keyId;
    final frostKey = coord.getFrostKey(keyId: keyId)!;
    final walletName = frostKey.keyName();

    return Padding(
      padding: EdgeInsets.all(16.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.center,
        children: [
          // Wallet Name
          Text(
            "DELETE ‘$walletName’?",
            style: Theme.of(context).textTheme.titleLarge,
          ),
          SizedBox(height: 8),

          if (walletCtx != null)
            DefaultTextStyle(
              style: const TextStyle(fontSize: 24),
              child: Row(
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  const Text('Balance: '),
                  StreamBuilder(
                    stream: walletCtx.txStream,
                    builder: (context, snapshot) =>
                        SatoshiText(value: snapshot.data?.balance ?? 0),
                  ),
                  //UpdatingBalance(txStream: walletCtx.txStream),
                ],
              ),
            ),
          SizedBox(height: 16),
          DefaultTextStyle(
            textAlign: TextAlign.left,
            style: Theme.of(context).textTheme.bodyLarge!,
            child: BulletList(const [
              Text(
                'This only deletes the wallet from this app.',
                softWrap: true,
              ),
              Text(
                'No secret keys will be deleted from devices',
                softWrap: true,
              ),
              Text(
                'The wallet will still can still be restored from Frostsnap devices and/or backups',
                softWrap: true,
              ),
            ]),
          ),
          SizedBox(height: 24),
          // Hold-to-Delete Button
          Center(
            child: HoldToDeleteButton(
              buttonText: Text(
                style: TextStyle(fontWeight: FontWeight.bold),
                softWrap: true,
                textAlign: TextAlign.center,
                "Hold to Delete",
              ),
              onComplete: () async {
                await coord.deleteKey(keyId: keyId);
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
                        ElevatedButton(
                          onPressed: () {
                            Navigator.pop(context); // close popup
                            Navigator.pop(context); // close delete page
                          },
                          child: Text('OK'),
                        ),
                      ],
                    ),
                  );
                }
              },
            ),
          ),
        ],
      ),
    );
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
          color: _isPressed
              ? theme.colorScheme.secondary
              : theme.colorScheme.tertiary,
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
                backgroundColor: theme.colorScheme.surfaceContainer,
                valueColor: AlwaysStoppedAnimation<Color>(Colors.red),
              ),
            ),
            widget.buttonText,
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

    return Container(
      padding: EdgeInsets.all(16.0),
      child: Column(
        children: [
          Text(
            "The ‘$keyName’ wallet can be unlocked with:",
            style: Theme.of(context).textTheme.headlineMedium,
          ),
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
