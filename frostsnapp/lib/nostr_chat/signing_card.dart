import 'package:flutter/material.dart';
import 'package:frostsnap/nostr_chat/nostr_profile.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/nostr.dart';
import 'package:frostsnap/src/rust/api/signing.dart';

class SigningRequestState {
  final FfiSigningEvent_Request request;
  final Map<String, FfiSigningEvent_Offer> offers = {};
  final Map<String, FfiSigningEvent_Partial> partials = {};

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

String signingDetailsText(SigningDetails details) => switch (details) {
      SigningDetails_Message(:final message) => message,
      SigningDetails_Nostr(:final content) => content,
      SigningDetails_Transaction() => 'Bitcoin Transaction',
    };

String _signersNeededText(int offerCount, int threshold) {
  final remaining = threshold - offerCount;
  if (remaining <= 0) return 'All signers ready';
  return '$remaining more ${remaining == 1 ? 'signer' : 'signers'} needed';
}

Widget _buildSigningDetails(ThemeData theme, SigningDetails details) {
  final (IconData icon, String text) = switch (details) {
    SigningDetails_Message(:final message) => (Icons.message, message),
    SigningDetails_Nostr(:final content) => (Icons.tag, content),
    SigningDetails_Transaction() =>
      (Icons.currency_bitcoin, 'Bitcoin Transaction'),
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
Widget _signingHeader(ThemeData theme, {
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

  const _CardWrapper({
    required this.bubble,
    required this.isMe,
    this.author,
    this.profile,
    this.onCopy,
    this.onReply,
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
                icon: Icon(Icons.copy, size: 18,
                    color: theme.colorScheme.onSurfaceVariant),
                onPressed: widget.onCopy,
                padding: EdgeInsets.zero,
                constraints: const BoxConstraints(),
                splashRadius: 14,
                tooltip: 'Copy',
              ),
            if (widget.onReply != null)
              IconButton(
                icon: Icon(Icons.reply, size: 18,
                    color: theme.colorScheme.onSurfaceVariant),
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
                        NostrAvatar.small(
                            profile: widget.profile, pubkey: widget.author!),
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

/// Bubble for offer/partial/error signing events.
class SigningEventCard extends StatelessWidget {
  final IconData icon;
  final Color? iconColor;
  final String title;
  final String? text;
  final String? subtitle;
  final bool isMe;
  final bool isOrphaned;
  final Widget? quotedContent;
  final PublicKey? author;
  final FfiNostrProfile? profile;
  final VoidCallback? onSign;
  final VoidCallback? onCopy;
  final VoidCallback? onReply;

  const SigningEventCard._({
    super.key,
    required this.icon,
    this.iconColor,
    required this.title,
    this.text,
    this.subtitle,
    this.isMe = false,
    this.isOrphaned = false,
    this.quotedContent,
    this.author,
    this.profile,
    this.onSign,
    this.onCopy,
    this.onReply,
  });

  factory SigningEventCard.offer({
    Key? key,
    required PublicKey author,
    required FfiNostrProfile? profile,
    required bool isMe,
    required int shareIndex,
    bool isOrphaned = false,
    SigningRequestState? requestState,
    String? requestAuthorName,
    int? threshold,
    VoidCallback? onSign,
    VoidCallback? onCopy,
    VoidCallback? onReply,
  }) {
    final name = isMe ? 'You' : getDisplayName(profile, author);

    String? subtitle;
    if (requestState != null && threshold != null) {
      subtitle = _signersNeededText(requestState.offers.length, threshold);
    }

    return SigningEventCard._(
      key: key,
      icon: Icons.draw,
      title: 'Sign Offer',
      text: '$name offered to sign with key #$shareIndex',
      subtitle: subtitle,
      isMe: isMe,
      isOrphaned: isOrphaned,
      author: author,
      profile: profile,
      onSign: onSign,
      onCopy: onCopy,
      onReply: onReply,
      quotedContent: requestState != null
          ? Builder(
              builder: (context) {
                final theme = Theme.of(context);
                return Container(
                  padding:
                      const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
                  decoration: BoxDecoration(
                    border: Border(
                      left: BorderSide(
                          color: theme.colorScheme.primary, width: 2),
                    ),
                    color: theme.colorScheme.surface.withValues(alpha: 0.5),
                  ),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      if (requestAuthorName != null)
                        Text(
                          requestAuthorName,
                          style: theme.textTheme.labelSmall?.copyWith(
                            color: theme.colorScheme.primary,
                            fontWeight: FontWeight.w600,
                          ),
                        ),
                      Text(
                        signingDetailsText(
                            requestState.request.signingDetails),
                        maxLines: 2,
                        overflow: TextOverflow.ellipsis,
                        style: theme.textTheme.bodySmall?.copyWith(
                          color: theme.colorScheme.onSurfaceVariant,
                        ),
                      ),
                      if (requestState.request.message != null)
                        Text(
                          requestState.request.message!,
                          maxLines: 2,
                          overflow: TextOverflow.ellipsis,
                          style: theme.textTheme.bodySmall?.copyWith(
                            color: theme.colorScheme.onSurfaceVariant,
                          ),
                        ),
                    ],
                  ),
                );
              },
            )
          : null,
    );
  }

  factory SigningEventCard.partial({
    Key? key,
    required PublicKey author,
    required FfiNostrProfile? profile,
    required bool isMe,
    bool isOrphaned = false,
    SigningRequestState? requestState,
    int? threshold,
    VoidCallback? onCopy,
    VoidCallback? onReply,
  }) {
    final name = isMe ? 'You' : getDisplayName(profile, author);

    String? subtitle;
    if (requestState != null && threshold != null) {
      final signed = requestState.partials.length;
      subtitle = '$signed/$threshold signed';
    }

    Widget? quotedContent;
    if (requestState != null) {
      final offer = requestState.offers[author.toHex()];
      if (offer != null) {
        quotedContent = Builder(builder: (context) {
          final theme = Theme.of(context);
          return Container(
            padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
            decoration: BoxDecoration(
              border: Border(
                left: BorderSide(color: theme.colorScheme.primary, width: 2),
              ),
              color: theme.colorScheme.surface.withValues(alpha: 0.5),
            ),
            child: Text(
              '${isMe ? 'Your' : '${getDisplayName(profile, author)}\'s'} offer — key #${offer.shareIndex}',
              style: theme.textTheme.bodySmall?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
          );
        });
      }
    }

    return SigningEventCard._(
      key: key,
      icon: Icons.check_circle,
      iconColor: Colors.green,
      title: 'Signed',
      text: '$name signed',
      subtitle: subtitle,
      isMe: isMe,
      isOrphaned: isOrphaned,
      quotedContent: quotedContent,
      author: author,
      profile: profile,
      onCopy: onCopy,
      onReply: onReply,
    );
  }

  factory SigningEventCard.error({
    Key? key,
    required String text,
    PublicKey? author,
    FfiNostrProfile? profile,
    bool isMe = false,
    VoidCallback? onCopy,
    VoidCallback? onReply,
  }) {
    return SigningEventCard._(
      key: key,
      icon: Icons.error_outline,
      iconColor: Colors.red,
      title: 'Error',
      text: text,
      isMe: isMe,
      author: author,
      profile: profile,
      onCopy: onCopy,
      onReply: onReply,
    );
  }

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
        border: isOrphaned ? Border.all(color: Colors.red, width: 1) : null,
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          _signingHeader(theme, icon: icon, iconColor: iconColor, title: title),
          if (quotedContent != null) ...[
            const SizedBox(height: 6),
            quotedContent!,
          ],
          if (text != null) ...[
            const SizedBox(height: 4),
            Text(
              text!,
              style: theme.textTheme.bodyMedium?.copyWith(
                fontStyle: FontStyle.italic,
              ),
            ),
          ],
          if (subtitle != null) ...[
            const SizedBox(height: 4),
            Text(
              subtitle!,
              style: theme.textTheme.bodySmall?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
          ],
          if (onSign != null) ...[
            const SizedBox(height: 8),
            FilledButton(
              onPressed: onSign,
              child: const Text('Sign'),
            ),
          ],
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
    );
  }
}

class SigningRequestCard extends StatelessWidget {
  final SigningRequestState state;
  final int threshold;
  final bool isMe;
  final VoidCallback? onOfferToSign;
  final String Function(PublicKey) getDisplayName;
  final FfiNostrProfile? profile;
  final VoidCallback? onCopy;
  final VoidCallback? onReply;

  const SigningRequestCard({
    super.key,
    required this.state,
    required this.threshold,
    required this.isMe,
    required this.getDisplayName,
    this.profile,
    this.onOfferToSign,
    this.onCopy,
    this.onReply,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final request = state.request;
    final offerCount = state.offers.length;
    final partialCount = state.partials.length;
    final isComplete = partialCount >= threshold;

    final bubble = Container(
      padding: const EdgeInsets.all(12),
      constraints: BoxConstraints(
        maxWidth: MediaQuery.of(context).size.width * 0.7,
      ),
      decoration: BoxDecoration(
        color: isMe
            ? theme.colorScheme.primaryContainer
            : theme.colorScheme.surfaceContainerHighest,
        borderRadius: BorderRadius.circular(16),
      ),
      child: IntrinsicWidth(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          mainAxisSize: MainAxisSize.min,
          children: [
            Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                _signingHeader(theme,
                  icon: isComplete ? Icons.check_circle : Icons.draw,
                  iconColor: isComplete ? Colors.green : null,
                  title: isComplete ? 'Signed' : 'Signing Request',
                ),
                const SizedBox(width: 12),
                Text(
                  _signersNeededText(offerCount, threshold),
                  style: theme.textTheme.bodySmall?.copyWith(
                    color: theme.colorScheme.onSurfaceVariant,
                  ),
                ),
              ],
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
            Container(
              padding: const EdgeInsets.all(10),
              decoration: BoxDecoration(
                borderRadius: BorderRadius.circular(12),
                border: Border.all(
                  color: theme.colorScheme.outline.withValues(alpha: 0.3),
                ),
              ),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  _buildSigningDetails(theme, request.signingDetails),
                  if (!isComplete && onOfferToSign != null) ...[
                    const SizedBox(height: 10),
                    FilledButton(
                      onPressed: onOfferToSign,
                      child: const Text('Offer to Sign'),
                    ),
                  ],
                ],
              ),
            ),
            if (state.offers.isNotEmpty) ...[
              const SizedBox(height: 8),
              ...state.offers.values.map((offer) {
                final name = this.getDisplayName(offer.author);
                return Padding(
                  padding: const EdgeInsets.only(top: 2),
                  child: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Icon(Icons.person,
                          size: 14,
                          color: theme.colorScheme.onSurfaceVariant),
                      const SizedBox(width: 4),
                      Text(
                        '$name — key #${offer.shareIndex}',
                        style: theme.textTheme.bodySmall?.copyWith(
                          color: theme.colorScheme.onSurfaceVariant,
                        ),
                      ),
                    ],
                  ),
                );
              }),
            ],
            if (request.message != null) ...[
              const SizedBox(height: 8),
              Text(
                request.message!,
                style: theme.textTheme.bodyMedium,
              ),
            ],
          ],
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
    );
  }
}
