import 'package:flutter/material.dart';
import 'package:frostsnap/copy_feedback.dart';
import 'package:frostsnap/nostr_chat/nostr_profile.dart';
import 'package:frostsnap/src/rust/api/nostr.dart';
import 'package:url_launcher/url_launcher.dart';

/// Body of the member detail sheet/dialog. Designed to be the
/// `builder` return of `showBottomSheetOrDialog` — the host wraps
/// it; don't wrap in another modal.
class MemberDetailSheet extends StatelessWidget {
  final PublicKey pubkey;
  final NostrProfile? profile;
  final bool isSelf;
  final List<int> keyIndices;
  final List<DeviceKeyEntry> deviceKeys;
  final ScrollController? scrollController;
  final VoidCallback? onRestoreFromBackup;

  const MemberDetailSheet({
    super.key,
    required this.pubkey,
    required this.profile,
    this.isSelf = false,
    this.keyIndices = const [],
    this.deviceKeys = const [],
    this.scrollController,
    this.onRestoreFromBackup,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final npub = pubkey.toNpub();

    // Different surface from the host TopBar (which uses
    // `colorScheme.surface`), so the bar + body have a visible seam
    // and the scroll-divider becomes meaningful.
    return ColoredBox(
      color: theme.colorScheme.surfaceContainerLow,
      child: ListView(
        controller: scrollController,
        padding: const EdgeInsets.fromLTRB(24, 16, 24, 24),
        children: [
          Center(
            child: NostrAvatar.large(profile: profile, pubkey: pubkey),
          ),
          const SizedBox(height: 12),
          Center(
            child: Text(
              getDisplayName(profile, pubkey),
              style: theme.textTheme.titleLarge,
              textAlign: TextAlign.center,
            ),
          ),
          if (profile?.about != null && profile!.about!.isNotEmpty) ...[
            const SizedBox(height: 8),
            Center(
              child: Text(
                profile!.about!,
                style: theme.textTheme.bodyMedium?.copyWith(
                  color: theme.colorScheme.onSurfaceVariant,
                ),
                textAlign: TextAlign.center,
              ),
            ),
          ],
          const SizedBox(height: 24),

          // KEYS section — uniform across self and others. For self
          // we know device names; for others we only know the key
          // index.
          _sectionLabel(theme, isSelf ? 'YOUR KEYS' : 'KEYS'),
          const SizedBox(height: 8),
          _KeysGroup(
            isSelf: isSelf,
            keyIndices: keyIndices,
            deviceKeys: deviceKeys,
            onRestoreFromBackup: onRestoreFromBackup,
          ),

          const SizedBox(height: 24),
          _sectionLabel(theme, 'IDENTITY'),
          const SizedBox(height: 8),
          _LabeledCopyField(
            label: 'npub',
            value: npub,
            onCopy: () => copyToClipboard(npub),
          ),
          if (profile?.nip05 != null && profile!.nip05!.isNotEmpty) ...[
            const SizedBox(height: 12),
            _LabeledCopyField(
              label: 'Nostr address (NIP-05)',
              value: profile!.nip05!,
              onCopy: () => copyToClipboard(profile!.nip05!),
            ),
          ],

          const SizedBox(height: 24),
          OutlinedButton.icon(
            onPressed: () => _openInNostrClient(context, npub),
            icon: const Icon(Icons.open_in_new_rounded),
            label: const Text('Open in Nostr client'),
          ),
        ],
      ),
    );
  }

  Widget _sectionLabel(ThemeData theme, String text) => Text(
    text,
    style: theme.textTheme.labelSmall?.copyWith(
      color: theme.colorScheme.onSurfaceVariant,
      fontWeight: FontWeight.w600,
      letterSpacing: 0.8,
    ),
  );

  void _openInNostrClient(BuildContext context, String npub) {
    final uri = Uri.parse('nostr:$npub');
    launchUrl(uri).catchError((e) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(
          content: Text('No Nostr client installed'),
          duration: Duration(seconds: 2),
        ),
      );
      return false;
    });
  }
}

class DeviceKeyEntry {
  final String deviceName;
  final int keyIndex;
  const DeviceKeyEntry({required this.deviceName, required this.keyIndex});
}

/// Rounded grouped tile list for the keys section.
class _KeysGroup extends StatelessWidget {
  final bool isSelf;
  final List<int> keyIndices;
  final List<DeviceKeyEntry> deviceKeys;
  final VoidCallback? onRestoreFromBackup;

  const _KeysGroup({
    required this.isSelf,
    required this.keyIndices,
    required this.deviceKeys,
    this.onRestoreFromBackup,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final tileColor = theme.colorScheme.surfaceContainerHigh;
    final tiles = <Widget>[];

    if (isSelf) {
      for (final d in deviceKeys) {
        tiles.add(
          _KeyTile(
            tileColor: tileColor,
            leading: const Icon(Icons.devices_rounded),
            title: d.deviceName,
            subtitle: 'Key #${d.keyIndex}',
          ),
        );
      }
      if (onRestoreFromBackup != null) {
        tiles.add(
          _KeyTile(
            tileColor: tileColor,
            leading: Icon(Icons.add_rounded, color: theme.colorScheme.primary),
            title: 'Restore a key from backup',
            titleColor: theme.colorScheme.primary,
            onTap: onRestoreFromBackup,
          ),
        );
      }
    } else {
      // Other member: we only know key index, not device name.
      for (final i in keyIndices) {
        tiles.add(
          _KeyTile(
            tileColor: tileColor,
            leading: const Icon(Icons.key_rounded),
            title: 'Key #$i',
          ),
        );
      }
      if (tiles.isEmpty) {
        tiles.add(
          _KeyTile(
            tileColor: tileColor,
            leading: const Icon(Icons.key_off_rounded),
            title: 'Holds no keys',
            titleStyle: FontStyle.italic,
          ),
        );
      }
    }

    // Apply grouped rounded shape — top/mid/end/single.
    final shaped = <Widget>[];
    for (var i = 0; i < tiles.length; i++) {
      shaped.add(_shapeWrap(tiles[i], i, tiles.length));
      if (i < tiles.length - 1) shaped.add(const SizedBox(height: 2));
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: shaped,
    );
  }

  Widget _shapeWrap(Widget tile, int i, int total) {
    final shape = _shapeFor(i, total);
    return ClipRRect(borderRadius: shape, child: tile);
  }

  BorderRadius _shapeFor(int i, int total) {
    const r = Radius.circular(20);
    const r4 = Radius.circular(4);
    if (total == 1) return const BorderRadius.all(r);
    if (i == 0)
      return const BorderRadius.only(
        topLeft: r,
        topRight: r,
        bottomLeft: r4,
        bottomRight: r4,
      );
    if (i == total - 1)
      return const BorderRadius.only(
        topLeft: r4,
        topRight: r4,
        bottomLeft: r,
        bottomRight: r,
      );
    return const BorderRadius.all(r4);
  }
}

class _KeyTile extends StatelessWidget {
  final Color tileColor;
  final Widget leading;
  final String title;
  final String? subtitle;
  final Color? titleColor;
  final FontStyle? titleStyle;
  final VoidCallback? onTap;

  const _KeyTile({
    required this.tileColor,
    required this.leading,
    required this.title,
    this.subtitle,
    this.titleColor,
    this.titleStyle,
    this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Material(
      color: tileColor,
      child: ListTile(
        leading: leading,
        title: Text(
          title,
          style: theme.textTheme.bodyLarge?.copyWith(
            color: titleColor,
            fontStyle: titleStyle,
          ),
        ),
        subtitle: subtitle == null
            ? null
            : Text(
                subtitle!,
                style: theme.textTheme.bodySmall?.copyWith(
                  color: theme.colorScheme.onSurfaceVariant,
                ),
              ),
        onTap: onTap,
      ),
    );
  }
}

class _LabeledCopyField extends StatelessWidget {
  final String label;
  final String value;
  final VoidCallback onCopy;

  const _LabeledCopyField({
    required this.label,
    required this.value,
    required this.onCopy,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          label,
          style: theme.textTheme.bodySmall?.copyWith(
            color: theme.colorScheme.onSurfaceVariant,
          ),
        ),
        const SizedBox(height: 4),
        Material(
          color: theme.colorScheme.surfaceContainerHigh,
          borderRadius: BorderRadius.circular(12),
          child: InkWell(
            onTap: onCopy,
            borderRadius: BorderRadius.circular(12),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 12),
              child: Row(
                children: [
                  Expanded(
                    child: Text(
                      value,
                      style: theme.textTheme.bodyMedium?.copyWith(
                        fontFamily: 'monospace',
                      ),
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
                  const SizedBox(width: 8),
                  Icon(
                    Icons.copy_rounded,
                    size: 18,
                    color: theme.colorScheme.onSurfaceVariant,
                  ),
                ],
              ),
            ),
          ),
        ),
      ],
    );
  }
}
