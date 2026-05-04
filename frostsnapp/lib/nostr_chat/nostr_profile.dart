import 'package:cached_network_image/cached_network_image.dart';
import 'package:flutter/material.dart';
import 'package:frostsnap/src/rust/api/nostr.dart';

/// Avatar widget for displaying Nostr profile pictures.
/// Falls back to first letter of name, then to generic person icon.
class NostrAvatar extends StatelessWidget {
  final NostrProfile? profile;
  final PublicKey? pubkey;
  final double size;
  final VoidCallback? onTap;

  const NostrAvatar({
    super.key,
    this.profile,
    this.pubkey,
    this.size = 36,
    this.onTap,
  });

  /// Small avatar for chat messages (36dp)
  const NostrAvatar.small({super.key, this.profile, this.pubkey, this.onTap})
    : size = 36;

  /// Medium avatar for list items (48dp)
  const NostrAvatar.medium({super.key, this.profile, this.pubkey, this.onTap})
    : size = 48;

  /// Large avatar for profile pages (64dp)
  const NostrAvatar.large({super.key, this.profile, this.pubkey, this.onTap})
    : size = 64;

  String? get _displayName => profile?.displayName ?? profile?.name;

  String? get _pictureUrl => profile?.picture;

  String _getInitial() {
    final name = _displayName;
    if (name != null && name.isNotEmpty) {
      return name[0].toUpperCase();
    }
    return '?';
  }

  Widget _buildFallbackAvatar(ThemeData theme) {
    if (_displayName != null) {
      return CircleAvatar(
        radius: size / 2,
        backgroundColor: theme.colorScheme.primaryContainer,
        child: Text(
          _getInitial(),
          style: TextStyle(
            fontSize: size * 0.4,
            fontWeight: FontWeight.w600,
            color: theme.colorScheme.onPrimaryContainer,
          ),
        ),
      );
    }
    return CircleAvatar(
      radius: size / 2,
      backgroundColor: theme.colorScheme.surfaceContainerHighest,
      child: Icon(
        Icons.person,
        size: size * 0.6,
        color: theme.colorScheme.onSurfaceVariant,
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final pictureUrl = _pictureUrl;

    Widget avatar;
    if (pictureUrl != null && pictureUrl.isNotEmpty) {
      avatar = CachedNetworkImage(
        imageUrl: pictureUrl,
        imageBuilder: (context, imageProvider) => CircleAvatar(
          radius: size / 2,
          backgroundImage: imageProvider,
          backgroundColor: theme.colorScheme.surfaceContainerHighest,
        ),
        placeholder: (context, url) => CircleAvatar(
          radius: size / 2,
          backgroundColor: theme.colorScheme.surfaceContainerHighest,
        ),
        errorWidget: (context, url, error) => _buildFallbackAvatar(theme),
      );
    } else {
      avatar = _buildFallbackAvatar(theme);
    }

    if (onTap != null) {
      return GestureDetector(onTap: onTap, child: avatar);
    }

    return avatar;
  }
}

/// Helper to get a display name from a profile or pubkey.
String getDisplayName(NostrProfile? profile, PublicKey? pubkey) {
  if (profile?.displayName != null && profile!.displayName!.isNotEmpty) {
    return profile.displayName!;
  }
  if (profile?.name != null && profile!.name!.isNotEmpty) {
    return profile.name!;
  }
  if (pubkey != null) {
    return shortenNpub(pubkey.toNpub());
  }
  return 'Unknown';
}

/// Shorten an npub for display (first 8 + last 4 chars).
String shortenNpub(String npub) {
  if (npub.length <= 16) return npub;
  return '${npub.substring(0, 12)}...${npub.substring(npub.length - 4)}';
}

/// Shorten a hex pubkey for display.
String shortenPubkeyHex(String hex) {
  if (hex.length <= 12) return hex;
  return '${hex.substring(0, 6)}...${hex.substring(hex.length - 4)}';
}
