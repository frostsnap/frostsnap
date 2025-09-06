import 'package:flutter/material.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/maybe_fullscreen_dialog.dart';
import 'package:frostsnap/psbt.dart';

class WalletMore extends StatefulWidget {
  final ScrollController? scrollController;

  const WalletMore({super.key, this.scrollController});

  @override
  State<WalletMore> createState() => _WalletMoreState();
}

class _WalletMoreState extends State<WalletMore> {
  static const tilePadding = EdgeInsets.symmetric(horizontal: 16);

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

    final column = Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        makeTitle(context, title: Text('Sign data')),
        ListTile(
          contentPadding: tilePadding,
          title: Text('PSBT'),
          subtitle: Text('Sign a partially signed bitcoin transaction'),
          leading: Icon(Icons.edit_document),
          onTap: () async {
            Navigator.popUntil(context, (r) => r.isFirst);
            await MaybeFullscreenDialog.show(
              context: context,
              child: walletCtx.wrap(LoadPsbtPage(wallet: walletCtx.wallet)),
            );
          },
        ),
        ListTile(
          contentPadding: tilePadding,
          title: Text('Message'),
          subtitle: Text('Sign an arbitary message'),
          leading: Icon(Icons.edit_note),
          onTap: () {},
        ),
        makeTitle(context, title: Text('Manage wallet')),
        ListTile(
          contentPadding: tilePadding,
          title: Text('Keys'),
          subtitle: Text('View wallet access structure and add devices'),
          leading: Icon(Icons.key_rounded),
          onTap: () {},
        ),
        ListTile(
          contentPadding: tilePadding,
          title: Text('Backup'),
          subtitle: Text('Physically backup wallet keys'),
          leading: Icon(Icons.shield),
          onTap: () {},
        ),
        ListTile(
          contentPadding: tilePadding,
          title: Text('Check address'),
          subtitle: Text('Check if an address is part of this wallet'),
          leading: Icon(Icons.pin_drop),
        ),
        ListTile(
          contentPadding: tilePadding,
          title: Text('Descriptor'),
          subtitle: Text('Show the wallet\'s miniscript descriptor'),
          leading: Icon(Icons.code),
        ),
        ListTile(
          contentPadding: tilePadding,
          title: Text('Delete wallet'),
          subtitle: Text('Delete this wallet from the app'),
          leading: Icon(Icons.delete),
          textColor: theme.colorScheme.error,
          iconColor: theme.colorScheme.error,
        ),
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
      contentPadding: tilePadding,
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
