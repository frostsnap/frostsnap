import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnap/address.dart';
import 'package:frostsnap/backup_workflow.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/maybe_fullscreen_dialog.dart';
import 'package:frostsnap/psbt.dart';
import 'package:frostsnap/settings.dart';
import 'package:frostsnap/sign_message.dart';
import 'package:frostsnap/snackbar.dart';
import 'package:frostsnap/theme.dart';
import 'package:pretty_qr_code/pretty_qr_code.dart';

class WalletMore extends StatefulWidget {
  final ScrollController? scrollController;

  const WalletMore({super.key, this.scrollController});

  @override
  State<WalletMore> createState() => _WalletMoreState();
}

class _WalletMoreState extends State<WalletMore> {
  static const contentPadding = EdgeInsets.symmetric(horizontal: 16);
  static const tileShape = RoundedRectangleBorder(
    borderRadius: BorderRadius.vertical(
      top: Radius.circular(4),
      bottom: Radius.circular(4),
    ),
  );
  static const tileShapeTop = RoundedRectangleBorder(
    borderRadius: BorderRadius.vertical(
      top: Radius.circular(24),
      bottom: Radius.circular(4),
    ),
  );
  static const tileShapeEnd = RoundedRectangleBorder(
    borderRadius: BorderRadius.vertical(
      top: Radius.circular(4),
      bottom: Radius.circular(24),
    ),
  );

  bool expandManage = true;

  @override
  Widget build(BuildContext context) {
    final walletCtx = WalletContext.of(context);
    return CustomScrollView(
      controller: widget.scrollController,
      shrinkWrap: true,
      physics: ClampingScrollPhysics(),
      slivers: [
        if (walletCtx != null)
          SliverToBoxAdapter(child: buildColumn(context, walletCtx)),
        SliverSafeArea(sliver: SliverToBoxAdapter(child: SizedBox(height: 12))),
      ],
    );
  }

  Widget buildColumn(BuildContext context, WalletContext walletCtx) {
    final theme = Theme.of(context);
    final tileColor = theme.colorScheme.surfaceContainer;

    final superCtx = SuperWalletContext.of(context)!;
    final walletCtx = WalletContext.of(context)!;
    final frostKey = coord.getFrostKey(keyId: walletCtx.keyId);

    final signColumn = Padding(
      padding: const EdgeInsets.symmetric(horizontal: 16.0),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        spacing: 2,
        children: [
          ListTile(
            contentPadding: contentPadding,
            tileColor: tileColor,
            shape: tileShapeTop,
            title: Text('PSBT'),
            subtitle: Text('Sign a partially signed bitcoin transaction'),
            leading: Icon(Icons.edit_document),
            onTap: () async {
              await MaybeFullscreenDialog.show(
                context: context,
                child: walletCtx.wrap(LoadPsbtPage(wallet: walletCtx.wallet)),
              );
            },
          ),
          ListTile(
            contentPadding: contentPadding,
            tileColor: tileColor,
            shape: tileShapeEnd,
            title: Text('Message'),
            subtitle: Text('Sign an arbitrary message'),
            leading: Icon(Icons.edit_note),
            onTap: frostKey == null
                ? null
                : () async {
                    await MaybeFullscreenDialog.show(
                      context: context,
                      child: SignMessagePage(frostKey: frostKey),
                    );
                  },
          ),
        ],
      ),
    );

    final manageColumn = Padding(
      padding: const EdgeInsets.symmetric(horizontal: 16),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        spacing: 2,
        children: [
          ListTile(
            contentPadding: contentPadding,
            tileColor: tileColor,
            shape: tileShapeTop,
            title: Text('Keys'),
            subtitle: Text('View wallet access structure and add devices'),
            leading: Icon(Icons.key_rounded),
            onTap: () async {
              await MaybeFullscreenDialog.show(
                context: context,
                child: KeyContext(
                  keyId: walletCtx.keyId,
                  child: KeysSettings(),
                ),
              );
            },
          ),
          ListTile(
            contentPadding: contentPadding,
            tileColor: tileColor,
            shape: tileShape,
            title: Text('Backup'),
            subtitle: Text('Physically backup wallet keys'),
            leading: Icon(Icons.shield),
            onTap: frostKey == null
                ? null
                : () async {
                    await MaybeFullscreenDialog.show(
                      context: context,
                      child: superCtx.tryWrapInWalletContext(
                        keyId: walletCtx.keyId,
                        child: BackupChecklist(
                          accessStructure: frostKey.accessStructures()[0],
                          showAppBar: true,
                        ),
                      ),
                    );
                  },
          ),
          ListTile(
            contentPadding: contentPadding,
            tileColor: tileColor,
            shape: tileShape,
            title: Text('Check address'),
            subtitle: Text('Check if an address is part of this wallet'),
            leading: Icon(Icons.pin_drop),
            onTap: () async {
              await MaybeFullscreenDialog.show(
                context: context,
                child: walletCtx.wrap(CheckAddressPage()),
              );
            },
          ),
          ListTile(
            contentPadding: contentPadding,
            tileColor: tileColor,
            shape: tileShape,
            title: Text('Descriptor'),
            subtitle: Text('Show the wallet\'s miniscript descriptor'),
            leading: Icon(Icons.code),
            onTap: () => showExportWalletDialog(
              context,
              walletCtx.network.descriptorForKey(
                masterAppkey: walletCtx.wallet.masterAppkey,
              ),
            ),
          ),
          ListTile(
            contentPadding: contentPadding,
            tileColor: tileColor,
            shape: tileShapeEnd,
            title: Text('Delete wallet'),
            subtitle: Text('Delete this wallet from the app'),
            leading: Icon(Icons.delete),
            textColor: theme.colorScheme.error,
            iconColor: theme.colorScheme.error,
            onTap: () async {
              await MaybeFullscreenDialog.show(
                context: context,
                child: walletCtx.wrap(DeleteWalletPage()),
              );
            },
          ),
        ],
      ),
    );

    final column = Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        makeTitle(context, title: Text('Sign data')),
        signColumn,
        makeTitle(context, title: Text('Manage wallet')),
        manageColumn,
        SizedBox(height: 8),
      ],
    );
    return AnimatedSize(
      duration: Durations.short4,
      curve: Curves.easeInOutCubicEmphasized,
      alignment: AlignmentGeometry.topCenter,
      child: column,
    );
  }

  Widget makeTitle(
    BuildContext context, {
    Widget? title,
    Widget? trailing,
    Function()? onTap,
  }) {
    final theme = Theme.of(context);
    return ListTile(
      contentPadding: contentPadding,
      title: title,
      titleTextStyle: TextStyle(
        color: theme.colorScheme.secondary,
        fontWeight: FontWeight.w600,
        inherit: false,
      ),
      trailing: trailing,
      onTap: onTap,
    );
  }
}

void showExportWalletDialog(BuildContext context, String descriptor) async {
  final theme = Theme.of(context).copyWith(
    colorScheme: ColorScheme.fromSeed(
      brightness: Brightness.light,
      seedColor: seedColor,
    ),
  );

  final qrCode = QrCode(8, QrErrorCorrectLevel.L);
  qrCode.addData(descriptor);
  final qr = PrettyQrView(qrImage: QrImage(qrCode));

  final descriptorButton = TextButton.icon(
    onPressed: () async {
      await Clipboard.setData(ClipboardData(text: descriptor));
      showMessageSnackbar(context, "Descriptor copied");
    },
    icon: Icon(Icons.copy),
    label: Text(
      descriptor,
      style: TextStyle(
        fontFamily: monospaceTextStyle.fontFamily,
        color: theme.colorScheme.onSurface,
      ),
    ),
    style: TextButton.styleFrom(alignment: Alignment.centerLeft),
  );

  final doneButton = FilledButton(
    onPressed: () => Navigator.popUntil(context, (r) => r.isFirst),
    child: Text('Done'),
  );

  await showDialog(
    context: context,
    barrierDismissible: true,
    builder: (BuildContext context) {
      return Theme(
        data: theme,
        child: Dialog(
          child: ConstrainedBox(
            constraints: BoxConstraints(maxWidth: 600),
            child: Padding(
              padding: EdgeInsets.all(16),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.stretch,
                spacing: 16,
                children: [
                  Flexible(
                    child: Center(
                      child: AspectRatio(aspectRatio: 1, child: qr),
                    ),
                  ),
                  Flexible(
                    child: SingleChildScrollView(child: descriptorButton),
                  ),
                  doneButton,
                ],
              ),
            ),
          ),
        ),
      );
    },
  );
}
