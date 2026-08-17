import 'package:flutter/material.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/src/rust/api/super_wallet.dart';
import 'package:frostsnap/theme.dart';
import 'package:frostsnap/wallet.dart';
import 'package:frostsnap/wallet_send.dart';

/// Nudges the user to consolidate coins sitting past the recovery gap.
///
/// A standard restore crawls addresses and stalls at the first unused stretch wider than its
/// gap limit, so a coin past such a stretch would be missed. Any outgoing transaction
/// consolidates those coins automatically (`plan_send` force-selects them); this banner exists
/// so the user knows one is worth making. Mounting it also kicks the recovery scan, so change
/// the wallet can't yet see is attributed first and then counted here.
class StrandedCoinsBanner extends StatefulWidget {
  const StrandedCoinsBanner({super.key});

  @override
  State<StrandedCoinsBanner> createState() => _StrandedCoinsBannerState();
}

class _StrandedCoinsBannerState extends State<StrandedCoinsBanner> {
  bool scanKicked = false;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (scanKicked) return;
    scanKicked = true;
    final walletCtx = WalletContext.of(context)!;
    walletCtx.superWallet
        .recoveryScan(masterAppkey: walletCtx.masterAppkey)
        .catchError((Object err) {
          debugPrint('recovery scan failed: $err');
        });
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final walletCtx = WalletContext.of(context)!;

    return StreamBuilder<TxState>(
      stream: walletCtx.txStream,
      builder: (context, snapshot) {
        var count = 0;
        var sats = 0;
        if (snapshot.hasData) {
          final (strandedCount, strandedSats) = walletCtx.superWallet
              .gapStrandedValue(masterAppkey: walletCtx.masterAppkey);
          count = strandedCount;
          sats = strandedSats;
        }

        final card = count == 0
            ? SizedBox(width: double.infinity)
            : Card(
                margin: EdgeInsets.symmetric(horizontal: 16, vertical: 4),
                shape: cardShape(context),
                color: tintSurfaceContainer(context, tint: cautionColor),
                clipBehavior: Clip.hardEdge,
                child: ListTile(
                  onTap: () => showBottomSheetOrDialog(
                    context,
                    title: Text('Send'),
                    builder: (context, scrollController) => walletCtx.wrap(
                      WalletSendPage(
                        scrollController: scrollController,
                        superWallet: walletCtx.superWallet,
                        masterAppkey: walletCtx.masterAppkey,
                      ),
                    ),
                  ),
                  leading: Icon(Icons.merge_rounded, color: cautionColor),
                  trailing: Icon(Icons.chevron_right),
                  title: Text(
                    count == 1
                        ? 'A coin could be missed by a future restore'
                        : '$count coins could be missed by a future restore',
                  ),
                  subtitle: Row(
                    children: [
                      Expanded(
                        child: Text(
                          count == 1
                              ? 'Your next payment consolidates it automatically'
                              : 'Your next payment consolidates them automatically',
                          style: theme.textTheme.bodySmall?.copyWith(
                            color: theme.colorScheme.onSurfaceVariant,
                          ),
                        ),
                      ),
                      SatoshiText(
                        value: sats,
                        style: theme.textTheme.bodySmall?.copyWith(
                          color: theme.colorScheme.onSurfaceVariant,
                        ),
                      ),
                    ],
                  ),
                ),
              );

        return AnimatedSize(
          duration: Durations.medium4,
          curve: Curves.easeInOutCubicEmphasized,
          alignment: Alignment.topCenter,
          child: card,
        );
      },
    );
  }
}
