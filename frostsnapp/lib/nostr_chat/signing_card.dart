import 'package:dynamic_color/dynamic_color.dart';
import 'package:flutter/material.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/nostr_chat/chat_page.dart' show MessageStatus;
import 'package:frostsnap/nostr_chat/nostr_profile.dart';
import 'package:frostsnap/src/rust/api/nostr.dart';
import 'package:frostsnap/src/rust/api/signing.dart';
import 'package:frostsnap/theme.dart';
import 'package:frostsnap/wallet.dart';
import 'package:frostsnap/wallet_tx_details.dart';

class SigningRequestState extends ChangeNotifier {
  final FfiSigningEvent_Request request;
  final Map<String, FfiSigningEvent_Offer> offers = {};
  final Map<String, FfiSigningEvent_Partial> partials = {};
  bool _cancelled = false;

  bool get cancelled => _cancelled;
  set cancelled(bool value) {
    _cancelled = value;
    notifyListeners();
  }

  void addOffer(String pubkeyHex, FfiSigningEvent_Offer offer) {
    offers[pubkeyHex] = offer;
    notifyListeners();
  }

  void addPartial(String pubkeyHex, FfiSigningEvent_Partial partial) {
    partials[pubkeyHex] = partial;
    notifyListeners();
  }

  SigningRequestState(this.request);

  SealedSigningData? get sealedData {
    for (final offer in offers.values) {
      if (offer.sealed != null) return offer.sealed;
    }
    return null;
  }

  NostrEventId get chainTip {
    if (offers.isEmpty) return request.eventId;
    return offers.values
        .reduce((a, b) => a.timestamp > b.timestamp ? a : b)
        .eventId;
  }

  DateTime get timestamp =>
      DateTime.fromMillisecondsSinceEpoch(request.timestamp * 1000);
}

String signingDetailsText(
  SigningDetails details, {
  WalletContext? walletCtx,
}) => switch (details) {
  SigningDetails_Message(:final message) => message,
  SigningDetails_Nostr(:final content) => content,
  SigningDetails_Transaction(:final transaction) => () {
    if (walletCtx == null) return 'Bitcoin Transaction';
    final recipients = transaction.recipients().where((r) => !r.isMine).toList();
    if (recipients.isEmpty) return 'Bitcoin Transaction';
    final r = recipients.first;
    final addr = r.address(network: walletCtx.network)?.toString() ?? '?';
    return 'Send ${r.amount} sats to $addr';
  }(),
};

Widget _buildSigningDetails(
  BuildContext context,
  ThemeData theme,
  SigningDetails details, {
  VoidCallback? onTap,
}) {
  if (details is SigningDetails_Transaction) {
    final walletCtx = WalletContext.of(context);
    final tx = details.transaction;
    if (walletCtx != null) {
      final chainTipHeight = walletCtx.superWallet.height();
      final txDetails = TxDetailsModel(
        tx: tx,
        chainTipHeight: chainTipHeight,
        now: DateTime.now(),
      );
      final accentColor = txDetails.isSend
          ? Colors.redAccent.harmonizeWith(theme.colorScheme.primary)
          : Colors.green.harmonizeWith(theme.colorScheme.primary);
      return Container(
        decoration: BoxDecoration(
          color: theme.colorScheme.surface.withValues(alpha: 0.5),
          borderRadius: BorderRadius.circular(8),
        ),
        child: ListTile(
          shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(8)),
          contentPadding: const EdgeInsets.symmetric(horizontal: 16),
          leading: Icon(
            txDetails.isSend ? Icons.north_east : Icons.south_east,
            color: accentColor,
          ),
          title: Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              Text(txDetails.isSend ? 'Send' : 'Receive'),
              SatoshiText(
                value: txDetails.netValue.abs(),
                style: theme.textTheme.bodyLarge,
              ),
            ],
          ),
          onTap: onTap ?? () {
            showBottomSheetOrDialog(
              context,
              title: const Text('Transaction Details'),
              builder: (_, scrollController) => walletCtx.wrap(
                Builder(builder: (ctx) => SingleChildScrollView(
                  controller: scrollController,
                  child: buildDetailsColumn(ctx, txDetails: txDetails),
                )),
              ),
            );
          },
        ),
      );
    }
  }

  final (IconData icon, String text) = switch (details) {
    SigningDetails_Message(:final message) => (Icons.message, message),
    SigningDetails_Nostr(:final content) => (Icons.tag, content),
    SigningDetails_Transaction() => (Icons.currency_bitcoin, 'Bitcoin Transaction'),
  };

  return Container(
    padding: const EdgeInsets.all(8),
    decoration: BoxDecoration(
      color: theme.colorScheme.surface.withValues(alpha: 0.5),
      borderRadius: BorderRadius.circular(8),
    ),
    child: Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Icon(icon, size: 16, color: theme.colorScheme.onSurfaceVariant),
        const SizedBox(width: 8),
        Flexible(child: Text(text, style: theme.textTheme.bodyMedium)),
      ],
    ),
  );
}

/// Common header row for all signing cards.
Widget _signingHeader(
  ThemeData theme, {
  required IconData icon,
  Color? iconColor,
  required String title,
}) {
  return Row(
    mainAxisSize: MainAxisSize.min,
    children: [
      Icon(icon, size: 18, color: iconColor ?? theme.colorScheme.primary),
      const SizedBox(width: 6),
      Text(
        title,
        style: theme.textTheme.titleSmall?.copyWith(
          fontWeight: FontWeight.w600,
        ),
      ),
    ],
  );
}

/// Wraps a signing card bubble with avatar, hover actions, and long-press menu.
class _CardWrapper extends StatefulWidget {
  final Widget bubble;
  final bool isMe;
  final PublicKey? author;
  final FfiNostrProfile? profile;
  final VoidCallback? onCopy;
  final VoidCallback? onReply;
  final VoidCallback? onTapAvatar;

  const _CardWrapper({
    required this.bubble,
    required this.isMe,
    this.author,
    this.profile,
    this.onCopy,
    this.onReply,
    this.onTapAvatar,
  });

  @override
  State<_CardWrapper> createState() => _CardWrapperState();
}

class _CardWrapperState extends State<_CardWrapper> {
  bool _isHovered = false;

  bool _isMobile(BuildContext context) =>
      MediaQuery.of(context).size.width < 600;

  void _showMobileActions(BuildContext context) {
    showModalBottomSheet(
      context: context,
      builder: (sheetContext) {
        return SafeArea(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              if (widget.onCopy != null)
                ListTile(
                  leading: const Icon(Icons.copy),
                  title: const Text('Copy'),
                  onTap: () {
                    Navigator.pop(sheetContext);
                    widget.onCopy!();
                  },
                ),
              if (widget.onReply != null)
                ListTile(
                  leading: const Icon(Icons.reply),
                  title: const Text('Reply'),
                  onTap: () {
                    Navigator.pop(sheetContext);
                    widget.onReply!();
                  },
                ),
            ],
          ),
        );
      },
    );
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final isMobile = _isMobile(context);
    final hasActions = widget.onCopy != null || widget.onReply != null;

    Widget? hoverActions;
    if (hasActions && !isMobile) {
      hoverActions = AnimatedOpacity(
        opacity: _isHovered ? 1.0 : 0.0,
        duration: const Duration(milliseconds: 150),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            if (widget.onCopy != null)
              IconButton(
                icon: Icon(
                  Icons.copy,
                  size: 18,
                  color: theme.colorScheme.onSurfaceVariant,
                ),
                onPressed: widget.onCopy,
                padding: EdgeInsets.zero,
                constraints: const BoxConstraints(),
                splashRadius: 14,
                tooltip: 'Copy',
              ),
            if (widget.onReply != null)
              IconButton(
                icon: Icon(
                  Icons.reply,
                  size: 18,
                  color: theme.colorScheme.onSurfaceVariant,
                ),
                onPressed: widget.onReply,
                padding: EdgeInsets.zero,
                constraints: const BoxConstraints(),
                splashRadius: 14,
                tooltip: 'Reply',
              ),
          ],
        ),
      );
    }

    return Align(
      alignment: widget.isMe ? Alignment.centerRight : Alignment.centerLeft,
      child: MouseRegion(
        onEnter: hasActions && !isMobile
            ? (_) => setState(() => _isHovered = true)
            : null,
        onExit: hasActions && !isMobile
            ? (_) => setState(() => _isHovered = false)
            : null,
        child: GestureDetector(
          onLongPress: hasActions && isMobile
              ? () => _showMobileActions(context)
              : null,
          child: Padding(
            padding: const EdgeInsets.symmetric(vertical: 4),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: widget.isMe
                  ? [
                      if (hoverActions != null) ...[
                        hoverActions,
                        const SizedBox(width: 4),
                      ],
                      Flexible(child: widget.bubble),
                    ]
                  : [
                      if (widget.author != null) ...[
                        GestureDetector(
                          onTap: widget.onTapAvatar,
                          child: NostrAvatar.small(
                            profile: widget.profile,
                            pubkey: widget.author!,
                          ),
                        ),
                        const SizedBox(width: 8),
                      ],
                      Flexible(child: widget.bubble),
                      if (hoverActions != null) ...[
                        const SizedBox(width: 4),
                        hoverActions,
                      ],
                    ],
            ),
          ),
        ),
      ),
    );
  }
}

class SigningErrorCard extends StatelessWidget {
  final String text;
  final bool isMe;
  final PublicKey? author;
  final FfiNostrProfile? profile;
  final VoidCallback? onCopy;
  final VoidCallback? onReply;
  final VoidCallback? onTapAvatar;

  const SigningErrorCard({
    super.key,
    required this.text,
    this.isMe = false,
    this.author,
    this.profile,
    this.onCopy,
    this.onReply,
    this.onTapAvatar,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final bubble = Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
      constraints: BoxConstraints(
        maxWidth: MediaQuery.of(context).size.width * 0.7,
      ),
      decoration: BoxDecoration(
        color: isMe
            ? theme.colorScheme.primaryContainer
            : theme.colorScheme.surfaceContainerHighest,
        borderRadius: BorderRadius.circular(16),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          _signingHeader(
            theme,
            icon: Icons.error_outline,
            iconColor: Colors.red,
            title: 'Error',
          ),
          const SizedBox(height: 4),
          Text(
            text,
            style: theme.textTheme.bodyMedium?.copyWith(
              fontStyle: FontStyle.italic,
            ),
          ),
        ],
      ),
    );

    return _CardWrapper(
      bubble: bubble,
      isMe: isMe,
      author: author,
      profile: profile,
      onCopy: onCopy,
      onReply: onReply,
      onTapAvatar: onTapAvatar,
    );
  }
}

class SigningRequestCard extends StatelessWidget {
  final SigningRequestState state;
  final int threshold;
  final bool isMe;
  final bool isHighlighted;
  final MessageStatus? sendStatus;
  final VoidCallback? onOfferToSign;
  final VoidCallback? onCancel;
  final String Function(PublicKey) getDisplayName;
  final FfiNostrProfile? profile;
  final VoidCallback? onTap;
  final VoidCallback? onCopy;
  final VoidCallback? onReply;
  final VoidCallback? onTapAvatar;

  const SigningRequestCard({
    super.key,
    required this.state,
    required this.threshold,
    required this.isMe,
    this.isHighlighted = false,
    this.sendStatus,
    required this.getDisplayName,
    this.profile,
    this.onOfferToSign,
    this.onCancel,
    this.onTap,
    this.onCopy,
    this.onReply,
    this.onTapAvatar,
  });

  String _formatTime(DateTime time) =>
      '${time.hour.toString().padLeft(2, '0')}:${time.minute.toString().padLeft(2, '0')}';

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final request = state.request;
    final isCancelled = state.cancelled;
    final isComplete = state.partials.length >= threshold;
    final time = DateTime.fromMillisecondsSinceEpoch(request.timestamp * 1000);

    final baseColor = isMe
        ? theme.colorScheme.primaryContainer
        : theme.colorScheme.surfaceContainerHighest;

    final bubble = GestureDetector(
      onTap: onTap,
      child: Opacity(
      opacity: isCancelled ? 0.5 : 1.0,
      child: Container(
        padding: const EdgeInsets.all(12),
        constraints: BoxConstraints(
          maxWidth: MediaQuery.of(context).size.width * 0.7,
        ),
        decoration: BoxDecoration(
          color: isHighlighted
              ? Color.lerp(baseColor, theme.colorScheme.primary, 0.2)
              : baseColor,
          borderRadius: BorderRadius.circular(16),
          boxShadow: isHighlighted
              ? [
                  BoxShadow(
                    color: theme.colorScheme.primary.withValues(alpha: 0.4),
                    blurRadius: 10,
                    spreadRadius: 1,
                  ),
                ]
              : [],
        ),
        child: IntrinsicWidth(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            mainAxisSize: MainAxisSize.min,
            children: [
              _signingHeader(
                theme,
                icon: isCancelled ? Icons.cancel : Icons.draw,
                iconColor: isCancelled ? theme.colorScheme.error : null,
                title: isCancelled ? 'Cancelled' : 'Signing Request',
              ),
              if (!isMe) ...[
                const SizedBox(height: 4),
                Text(
                  this.getDisplayName(request.author),
                  style: theme.textTheme.labelSmall?.copyWith(
                    color: theme.colorScheme.primary,
                  ),
                ),
              ],
              const SizedBox(height: 8),
              _buildSigningDetails(context, theme, request.signingDetails, onTap: onTap),
              if (request.message.isNotEmpty) ...[
                const SizedBox(height: 6),
                Text(request.message, style: theme.textTheme.bodyMedium),
              ],
              if (!isCancelled && !isComplete && onOfferToSign != null) ...[
                const SizedBox(height: 10),
                FilledButton(
                  onPressed: onOfferToSign,
                  child: Text(state.offers.length + 1 >= threshold ? 'Sign' : 'Offer to Sign'),
                ),
              ],
              if (!isCancelled && !isComplete && onCancel != null) ...[
                const SizedBox(height: 6),
                OutlinedButton(
                  onPressed: onCancel,
                  child: const Text('Cancel'),
                ),
              ],
              const SizedBox(height: 4),
              Row(
                mainAxisAlignment: MainAxisAlignment.end,
                mainAxisSize: MainAxisSize.min,
                children: [
                  if (isComplete) ...[
                    Icon(Icons.check_circle, size: 12, color: Colors.green),
                    const SizedBox(width: 4),
                  ],
                  Text(
                    _formatTime(time),
                    style: theme.textTheme.labelSmall?.copyWith(
                      fontSize: 10,
                      color: theme.colorScheme.onSurfaceVariant,
                    ),
                  ),
                  if (isMe && sendStatus != null) ...[
                    const SizedBox(width: 4),
                    Icon(
                      switch (sendStatus!) {
                        MessageStatus.pending => Icons.access_time,
                        MessageStatus.sent => Icons.check,
                        MessageStatus.failed => Icons.error_outline,
                      },
                      size: 12,
                      color: sendStatus == MessageStatus.failed
                          ? theme.colorScheme.error
                          : theme.colorScheme.outline,
                    ),
                  ],
                ],
              ),
            ],
          ),
        ),
      ),
    ));

    return _CardWrapper(
      bubble: bubble,
      isMe: isMe,
      author: request.author,
      profile: profile,
      onCopy: onCopy,
      onReply: onReply,
      onTapAvatar: onTapAvatar,
    );
  }
}
