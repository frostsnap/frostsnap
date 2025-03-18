import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnapp/bridge_definitions.dart';
import 'package:frostsnapp/contexts.dart';
import 'package:frostsnapp/theme.dart';
import 'package:frostsnapp/wallet.dart';
import 'package:url_launcher/url_launcher.dart';

class TxDetailsModel {
  /// The raw transaction.
  final Transaction tx;
  final int chainTipHeight;
  final DateTime now;

  TxDetailsModel({
    required this.tx,
    required this.chainTipHeight,
    required this.now,
  });

  int get netValue => tx.netValue;

  /// Number of blocks in our view of the best chain.
  int get chainLength => chainTipHeight + 1;

  /// Number of tx confirmations.
  int get confirmations =>
      chainLength - (tx.confirmationTime?.height ?? chainLength);
  bool get isConfirmed => confirmations > 0;
  bool get isSend => tx.netValue < 0;

  /// Human-readable string of the last update. This is either the confirmation time or when we last
  /// saw the tx in the mempool.
  String get lastUpdateString {
    final txTimeRaw = tx.timestamp();
    final txTime =
        (txTimeRaw != null)
            ? DateTime.fromMillisecondsSinceEpoch(txTimeRaw * 1000)
            : now;
    return humanReadableTimeDifference(now, txTime);
  }
}

String humanReadableTimeDifference(DateTime currentTime, DateTime itemTime) {
  final Duration difference = currentTime.difference(itemTime);

  if (difference.inSeconds < 60) {
    return 'Just now';
  } else if (difference.inMinutes < 60) {
    return '${difference.inMinutes} minute${difference.inMinutes > 1 ? 's' : ''} ago';
  } else if (difference.inHours < 24) {
    return '${difference.inHours} hour${difference.inHours > 1 ? 's' : ''} ago';
  } else if (difference.inDays == 1) {
    return 'Yesterday';
  } else if (difference.inDays < 7) {
    return '${difference.inDays} day${difference.inDays > 1 ? 's' : ''} ago';
  } else if (difference.inDays < 30) {
    final int weeks = (difference.inDays / 7).floor();
    return '$weeks week${weeks > 1 ? 's' : ''} ago';
  } else if (difference.inDays < 365) {
    final int months = (difference.inDays / 30).floor();
    return '$months month${months > 1 ? 's' : ''} ago';
  } else {
    final int years = (difference.inDays / 365).floor();
    return '$years year${years > 1 ? 's' : ''} ago';
  }
}

class TxSentOrReceivedTile extends StatelessWidget {
  final TxDetailsModel txDetails;
  final void Function()? onTap;

  const TxSentOrReceivedTile({super.key, required this.txDetails, this.onTap});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final Widget icon = Badge(
      alignment: AlignmentDirectional.bottomEnd,
      label: Icon(Icons.hourglass_top_rounded, size: 12.0),
      isLabelVisible: !txDetails.isConfirmed,
      backgroundColor: Colors.transparent,
      child: Icon(
        txDetails.isSend ? Icons.north_east : Icons.south_east,
        color:
            txDetails.isConfirmed
                ? (txDetails.isSend
                    ? theme.colorScheme.error
                    : theme.colorScheme.primary)
                : theme.disabledColor,
      ),
    );
    return ListTile(
      onTap: onTap,
      leading: icon,
      title: Text(
        txDetails.isSend
            ? (txDetails.isConfirmed ? 'Sent' : 'Sending...')
            : (txDetails.isConfirmed ? 'Received' : 'Receiving...'),
      ),
      subtitle: Text(txDetails.lastUpdateString),
      trailing: SatoshiText(
        value: txDetails.netValue,
        showSign: true,
        style: theme.textTheme.bodyLarge,
      ),
    );
  }
}

class TxDetailsPage extends StatelessWidget {
  final TxDetailsModel txDetails;

  const TxDetailsPage({super.key, required this.txDetails});

  Widget buildDetailsColumn(BuildContext context) {
    final walletCtx = WalletContext.of(context)!;
    return Column(
      children: [
        if (txDetails.isSend)
          ...txDetails.tx.recipients.where((info) => !info.isMine).map((info) {
            final address = info.address(network: walletCtx.network);
            return Column(
              children: [
                ListTile(
                  dense: true,
                  leading: Text('Recipient #${info.vout}'),
                  title: Text(
                    spacedHex(address ?? '<unknown>'),
                    style: monospaceTextStyle,
                    textAlign: TextAlign.end,
                  ),
                  onTap:
                      address == null
                          ? null
                          : () =>
                              copyAction(context, 'Recipient address', address),
                ),
                ListTile(
                  dense: true,
                  leading: Text('\u2570 Amount'),
                  title: SatoshiText(value: info.amount, showSign: false),
                  onTap:
                      () => copyAction(
                        context,
                        'Recipient amount',
                        '${info.amount}',
                      ),
                ),
              ],
            );
          }),
        ListTile(
          dense: true,
          leading: Text('Fee'),
          title:
              txDetails.tx.fee == null
                  ? Text('Unknown')
                  : SatoshiText(value: txDetails.tx.fee),
          onTap: () => copyAction(context, 'Fee amount', '${txDetails.tx.fee}'),
        ),
        ListTile(
          dense: true,
          leading: Text('Confirmations'),
          title: Text(
            txDetails.isConfirmed
                ? '${txDetails.confirmations} Block(s)'
                : 'None',
            textAlign: TextAlign.end,
          ),
          onTap:
              () => copyAction(
                context,
                'Confirmation count',
                '${txDetails.confirmations}',
              ),
        ),
        ListTile(
          dense: true,
          leading: Text('Txid'),
          title: Text(
            txDetails.tx.txid,
            style: monospaceTextStyle,
            textAlign: TextAlign.end,
          ),
          onTap: () => copyAction(context, 'Txid', txDetails.tx.txid),
        ),
      ],
    );
  }

  @override
  Widget build(BuildContext context) {
    const margin = EdgeInsets.only(left: 16.0, right: 16.0, bottom: 16.0);
    final theme = Theme.of(context);
    return CustomScrollView(
      shrinkWrap: true,
      physics: ClampingScrollPhysics(),
      slivers: [
        SliverAppBar(
          title: Text('Transaction'),
          titleTextStyle: theme.textTheme.titleMedium,
          centerTitle: true,
          forceMaterialTransparency: true,
          leading: IconButton(
            onPressed: () => Navigator.pop(context),
            icon: Icon(Icons.close),
          ),
          actionsPadding: EdgeInsets.symmetric(horizontal: 10.0),
          automaticallyImplyLeading: false,
        ),
        SliverSafeArea(
          sliver: SliverList(
            delegate: SliverChildListDelegate.fixed([
              Card.filled(
                margin: margin,
                child: TxSentOrReceivedTile(txDetails: txDetails),
              ),
              buildDetailsColumn(context),
              SingleChildScrollView(
                scrollDirection: Axis.horizontal,
                reverse: true,
                padding: EdgeInsets.all(16.0),
                child: Row(
                  spacing: 8.0,
                  children: [
                    if (!txDetails.isConfirmed)
                      ActionChip(
                        avatar: Icon(Icons.publish),
                        label: Text('Rebroadcast'),
                        onPressed: () => rebroadcastAction(context),
                      ),
                    ActionChip(
                      avatar: Icon(Icons.open_in_new),
                      label: Text('View in Explorer'),
                      onPressed: () async => await explorerAction(context),
                    ),
                  ],
                ),
              ),
            ]),
          ),
        ),
      ],
    );
  }

  rebroadcastAction(BuildContext context) {
    final walletCtx = WalletContext.of(context)!;
    walletCtx.superWallet.rebroadcast(txid: txDetails.tx.txid);
    ScaffoldMessenger.of(
      context,
    ).showSnackBar(SnackBar(content: Text('Transaction rebroadcasted')));
  }

  copyAction(BuildContext context, String what, String data) {
    Clipboard.setData(ClipboardData(text: data));
    ScaffoldMessenger.of(
      context,
    ).showSnackBar(SnackBar(content: Text('$what copied to clipboard')));
  }

  explorerAction(BuildContext context) async {
    final walletCtx = WalletContext.of(context)!;
    final explorer = getBlockExplorer(walletCtx.superWallet.network);
    await launchUrl(
      explorer.replace(path: '${explorer.path}tx/${txDetails.tx.txid}'),
    );
  }
}
