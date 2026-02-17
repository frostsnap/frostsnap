import 'package:flutter/material.dart';
import 'package:frostsnap/nostr_chat/nostr_profile.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/nostr.dart';
import 'package:frostsnap/src/rust/api/signing.dart';

class SigningRequestState {
  final FfiSigningEvent_Request request;
  final Map<String, FfiSigningEvent_Offer> offers = {};
  final Map<String, FfiSigningEvent_Partial> partials = {};
  DeviceId? myOfferedDevice;
  SignSessionId? sessionId;
  bool signingInProgress = false;

  SigningRequestState(this.request);

  DateTime get timestamp =>
      DateTime.fromMillisecondsSinceEpoch(request.timestamp * 1000);
}

String _signingDetailsText(SigningDetails details) => switch (details) {
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
    // TODO: structured transaction display
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

/// Wraps a card widget with avatar + alignment matching chat bubble layout.
Widget _withAvatar({
  required Widget child,
  required bool isMe,
  required PublicKey? author,
  required FfiNostrProfile? profile,
}) {
  return Align(
    alignment: isMe ? Alignment.centerRight : Alignment.centerLeft,
    child: Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          if (!isMe && author != null) ...[
            NostrAvatar.small(profile: profile, pubkey: author),
            const SizedBox(width: 8),
          ],
          Flexible(child: child),
        ],
      ),
    ),
  );
}

/// A small bubble for offer/partial/error signing events.
class SigningEventCard extends StatelessWidget {
  final IconData icon;
  final Color? iconColor;
  final String text;
  final String? subtitle;
  final bool isMe;
  final bool isOrphaned;
  final Widget? quotedContent;
  final PublicKey? author;
  final FfiNostrProfile? profile;
  final VoidCallback? onSign;

  const SigningEventCard._({
    super.key,
    required this.icon,
    this.iconColor,
    required this.text,
    this.subtitle,
    this.isMe = false,
    this.isOrphaned = false,
    this.quotedContent,
    this.author,
    this.profile,
    this.onSign,
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
  }) {
    final name = isMe ? 'You' : getDisplayName(profile, author);

    String? subtitle;
    if (requestState != null && threshold != null) {
      subtitle = _signersNeededText(requestState.offers.length, threshold);
    }

    return SigningEventCard._(
      key: key,
      icon: Icons.draw,
      text: '$name offered to sign with key #$shareIndex',
      subtitle: subtitle,
      isMe: isMe,
      isOrphaned: isOrphaned,
      author: author,
      profile: profile,
      onSign: onSign,
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
                        _signingDetailsText(
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
  }) {
    final name = isMe ? 'You' : getDisplayName(profile, author);
    return SigningEventCard._(
      key: key,
      icon: Icons.check_circle,
      iconColor: Colors.green,
      text: '$name signed',
      isMe: isMe,
      isOrphaned: isOrphaned,
      author: author,
      profile: profile,
    );
  }

  factory SigningEventCard.error({Key? key, required String text}) {
    return SigningEventCard._(
      key: key,
      icon: Icons.error_outline,
      iconColor: Colors.red,
      text: text,
      isOrphaned: true,
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
          if (quotedContent != null) ...[
            quotedContent!,
            const SizedBox(height: 6),
          ],
          Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(icon,
                  size: 18, color: iconColor ?? theme.colorScheme.primary),
              const SizedBox(width: 8),
              Flexible(
                child: Text(
                  text,
                  style: theme.textTheme.bodyMedium?.copyWith(
                    fontStyle: FontStyle.italic,
                  ),
                ),
              ),
            ],
          ),
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

    return _withAvatar(
      child: bubble,
      isMe: isMe,
      author: author,
      profile: profile,
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

  const SigningRequestCard({
    super.key,
    required this.state,
    required this.threshold,
    required this.isMe,
    required this.getDisplayName,
    this.profile,
    this.onOfferToSign,
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
                Icon(
                  isComplete ? Icons.check_circle : Icons.draw,
                  color: isComplete ? Colors.green : theme.colorScheme.primary,
                  size: 20,
                ),
                const SizedBox(width: 8),
                Text(
                  isComplete ? 'Signed' : 'Signing Request',
                  style: theme.textTheme.titleSmall?.copyWith(
                    fontWeight: FontWeight.w600,
                  ),
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

    return _withAvatar(
      child: bubble,
      isMe: isMe,
      author: request.author,
      profile: profile,
    );
  }
}
