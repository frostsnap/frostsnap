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
  final SigningEvent_Request request;
  final Map<String, SigningEvent_Offer> offers = {};
  final Map<String, SigningEvent_Partial> partials = {};
  bool _cancelled = false;

  /// Set once when the settling timer expires with >= threshold offers.
  /// Drives the transition from "collecting offers" to "signing".
  SealedSigningData? _sealedData;

  /// Offer event ids that made the cut, in the canonical order chosen by
  /// the Rust tree. Used when constructing `sendSignPartial`.
  List<EventId>? _sealedOfferSubset;

  /// The most recent "round still collecting" snapshot from the Rust tree.
  /// Set by `RoundPending` events fired when the settling timer expires
  /// below threshold. UI can use this to show "your offer is likely
  /// accepted" to authors whose offer is in `pendingObserved`.
  ({List<EventId> observed, int threshold})? _pending;

  bool get cancelled => _cancelled;
  set cancelled(bool value) {
    _cancelled = value;
    notifyListeners();
  }

  SealedSigningData? get sealedData => _sealedData;
  List<EventId>? get sealedOfferSubset => _sealedOfferSubset;
  ({List<EventId> observed, int threshold})? get pending => _pending;

  void addOffer(String pubkeyHex, SigningEvent_Offer offer) {
    offers[pubkeyHex] = offer;
    notifyListeners();
  }

  void addPartial(String pubkeyHex, SigningEvent_Partial partial) {
    partials[pubkeyHex] = partial;
    notifyListeners();
  }

  void setRoundConfirmed(
    SealedSigningData sealed,
    List<EventId> subsetEventIds,
  ) {
    _sealedData = sealed;
    _sealedOfferSubset = subsetEventIds;
    notifyListeners();
  }

  void setRoundPending(List<EventId> observed, int threshold) {
    _pending = (observed: observed, threshold: threshold);
    notifyListeners();
  }

  SigningRequestState(this.request);

  DateTime get timestamp =>
      DateTime.fromMillisecondsSinceEpoch(request.timestamp * 1000);
}

String signingDetailsText(SigningDetails details, {WalletContext? walletCtx}) =>
    switch (details) {
      SigningDetails_Message(:final message) => message,
      SigningDetails_Nostr(:final content) => content,
      SigningDetails_Transaction(:final transaction) => () {
        if (walletCtx == null) return 'Bitcoin Transaction';
        final recipients = transaction
            .recipients()
            .where((r) => !r.isMine)
            .toList();
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
          onTap:
              onTap ??
              () {
                showBottomSheetOrDialog(
                  context,
                  title: const Text('Transaction Details'),
                  builder: (_, scrollController) => walletCtx.wrap(
                    Builder(
                      builder: (ctx) => SingleChildScrollView(
                        controller: scrollController,
                        child: buildDetailsColumn(ctx, txDetails: txDetails),
                      ),
                    ),
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
    SigningDetails_Transaction() => (
      Icons.currency_bitcoin,
      'Bitcoin Transaction',
    ),
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
  final NostrProfile? profile;
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
  final NostrProfile? profile;
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
  final bool iOffered;
  final bool isHighlighted;
  final MessageStatus? sendStatus;
  final VoidCallback? onOfferToSign;
  final VoidCallback? onCancel;
  final String Function(PublicKey) getDisplayName;
  final NostrProfile? profile;
  final VoidCallback? onTap;
  final VoidCallback? onCopy;
  final VoidCallback? onReply;
  final VoidCallback? onTapAvatar;

  const SigningRequestCard({
    super.key,
    required this.state,
    required this.threshold,
    required this.isMe,
    required this.iOffered,
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
    return AnimatedBuilder(
      animation: state,
      builder: (context, _) => _buildCard(context),
    );
  }

  Widget _buildCard(BuildContext context) {
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
                _buildSigningDetails(
                  context,
                  theme,
                  signingDetails(signTask: request.signTask),
                  onTap: onTap,
                ),
                if (request.message.isNotEmpty) ...[
                  const SizedBox(height: 6),
                  Text(request.message, style: theme.textTheme.bodyMedium),
                ],
                if (!isCancelled && !isComplete) ...[
                  const SizedBox(height: 10),
                  _SigningStatusStrip(
                    state: state,
                    threshold: threshold,
                    iOffered: iOffered,
                    onOfferToSign: onOfferToSign,
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
      ),
    );

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

/// Rendered inside the SigningRequestCard to show "what's happening right
/// now" with the signing round — from "no offers yet" through "finalizing"
/// to "signing". Reads from SigningRequestState to derive the current
/// phase.
///
/// Phases:
/// - **Awaiting**    : no offers in yet, this user hasn't offered.
///                     Shows "Offer to Sign" button.
/// - **Offering**    : this user offered, round collecting, timer still
///                     counting down from their offer. No `pending`
///                     snapshot yet.
/// - **Parked**      : `RoundPending` fired (settling timer expired below
///                     threshold). Round is still collecting but paused
///                     for now; this user's offer is likely in when more
///                     arrive. Indeterminate progress.
/// - **Finalizing**  : offer count >= threshold, `sealedData == null`.
///                     The settling timer is running its final quiet
///                     window before confirming.
/// - **Signing**     : `sealedData != null`, waiting for partials.
class _SigningStatusStrip extends StatelessWidget {
  final SigningRequestState state;
  final int threshold;
  final bool iOffered;
  final VoidCallback? onOfferToSign;

  const _SigningStatusStrip({
    required this.state,
    required this.threshold,
    required this.iOffered,
    required this.onOfferToSign,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    // User hasn't offered and can offer → show the button.
    if (!iOffered && onOfferToSign != null) {
      return FilledButton(
        onPressed: onOfferToSign,
        child: Text(
          state.offers.length + 1 >= threshold ? 'Sign' : 'Offer to Sign',
        ),
      );
    }

    // From here the user is an observer or has already offered.
    // Derive the current phase.

    final offerCount = state.offers.length;
    final sealed = state.sealedData;
    final pending = state.pending;
    final partialCount = state.partials.length;

    // Signing phase: subset is locked, waiting for shares.
    if (sealed != null) {
      final signedCount = partialCount;
      return _StatusRow(
        color: theme.colorScheme.primary,
        icon: const SizedBox(
          width: 14,
          height: 14,
          child: CircularProgressIndicator(strokeWidth: 2),
        ),
        label: signedCount == 0
            ? 'Signing — waiting for devices'
            : 'Signing — $signedCount of $threshold signed',
        progress: threshold > 0 ? signedCount / threshold : null,
      );
    }

    // Threshold met but settling timer still running — "finalizing".
    if (offerCount >= threshold) {
      return _StatusRow(
        color: theme.colorScheme.primary,
        icon: const SizedBox(
          width: 14,
          height: 14,
          child: CircularProgressIndicator(strokeWidth: 2),
        ),
        label: 'Finalizing round…',
        progress: null, // indeterminate
      );
    }

    // Below threshold, timer fired at least once → parked, waiting for
    // more offers to arrive. Friendlier message if this user is already
    // in the observed set.
    if (pending != null) {
      final myInPending =
          iOffered; // if I offered, I'm in pending by construction
      final remaining = threshold - offerCount;
      return _StatusRow(
        color: theme.colorScheme.tertiary,
        icon: Icon(
          myInPending ? Icons.check_circle_outline : Icons.hourglass_top,
          size: 16,
          color: theme.colorScheme.tertiary,
        ),
        label: myInPending
            ? 'Your offer is held — waiting for ${_devicesWord(remaining)}'
            : '$offerCount of $threshold offered — waiting for ${_devicesWord(remaining)}',
        progress: threshold > 0 ? offerCount / threshold : null,
      );
    }

    // Below threshold, timer hasn't fired yet → quiet period still
    // running from the last offer. Ephemeral state; transitions to
    // either Parked (timer fires) or Finalizing (more offers push
    // count to threshold).
    if (iOffered) {
      return _StatusRow(
        color: theme.colorScheme.primary,
        icon: const SizedBox(
          width: 14,
          height: 14,
          child: CircularProgressIndicator(strokeWidth: 2),
        ),
        label: offerCount >= threshold
            ? 'Finalizing round…'
            : 'You offered — waiting for ${_devicesWord(threshold - offerCount)}',
        progress: threshold > 0 ? offerCount / threshold : null,
      );
    }

    // Observer watching offers accumulate.
    return _StatusRow(
      color: theme.colorScheme.onSurfaceVariant,
      icon: Icon(
        Icons.hourglass_empty,
        size: 16,
        color: theme.colorScheme.onSurfaceVariant,
      ),
      label: '$offerCount of $threshold offered',
      progress: threshold > 0 ? offerCount / threshold : null,
    );
  }

  static String _devicesWord(int n) =>
      n == 1 ? '1 more device' : '$n more devices';
}

/// Compact status row — icon + label on one line, optional progress bar
/// underneath. Used by [_SigningStatusStrip] for every phase.
class _StatusRow extends StatelessWidget {
  final Color color;
  final Widget icon;
  final String label;

  /// `null` renders an indeterminate progress bar; a value renders a
  /// determinate bar at `value` (0..1).
  final double? progress;

  const _StatusRow({
    required this.color,
    required this.icon,
    required this.label,
    required this.progress,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: [
        Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            SizedBox(width: 16, height: 16, child: Center(child: icon)),
            const SizedBox(width: 8),
            Flexible(
              child: Text(
                label,
                style: theme.textTheme.labelMedium?.copyWith(color: color),
              ),
            ),
          ],
        ),
        const SizedBox(height: 6),
        ClipRRect(
          borderRadius: BorderRadius.circular(2),
          child: LinearProgressIndicator(
            value: progress,
            minHeight: 3,
            backgroundColor: color.withValues(alpha: 0.15),
            valueColor: AlwaysStoppedAnimation<Color>(color),
          ),
        ),
      ],
    );
  }
}
