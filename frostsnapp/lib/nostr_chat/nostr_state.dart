import 'package:flutter/material.dart';
import 'package:frostsnap/nostr_chat/setup_dialog.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/nostr.dart';
import 'package:frostsnap/stream_ext.dart';
import 'package:rxdart/rxdart.dart';

/// Provides NostrSettings to widget tree, following same pattern as SettingsContext.
// ignore: must_be_immutable
class NostrContext extends InheritedWidget {
  final NostrSettings nostrSettings;
  late final BehaviorSubject<PublicKey?> identityStream;

  // Profile cache for other users
  final Map<String, BehaviorSubject<NostrProfile?>> _profileCache = {};
  final Set<String> _fetchingProfiles = {};
  NostrClient? _client;

  NostrContext({super.key, required this.nostrSettings, required super.child}) {
    identityStream = nostrSettings.subIdentity().toBehaviorSubject();
  }

  /// Convenience: `Stream<bool>` over a wallet's coordination-UI flag.
  /// Each call returns a fresh stream; `StreamBuilder` callers may
  /// reattach a Rust sink on every rebuild — that's an explicit,
  /// deterministic register/unregister cycle, not a leak.
  Stream<bool> watchCoordinationUi(AccessStructureRef asRef) => nostrSettings
      .accessStructure(accessStructureRef: asRef)
      .watch()
      .map((s) => s.coordinationUiEnabled);

  /// Sync read of the current coordination-UI flag via the Rust getter.
  bool isCoordinationUiEnabled(AccessStructureRef asRef) => nostrSettings
      .isCoordinationUiEnabled(accessStructureId: asRef.accessStructureId);

  static NostrContext of(BuildContext context) {
    final widget = context.dependOnInheritedWidgetOfExactType<NostrContext>();
    assert(widget != null, 'No NostrContext found in widget tree');
    return widget!;
  }

  static NostrContext? maybeOf(BuildContext context) {
    return context.dependOnInheritedWidgetOfExactType<NostrContext>();
  }

  /// Current local nostr pubkey. Throws if no identity is configured —
  /// callers downstream of `ensureIdentity` can rely on this.
  PublicKey get myPubkey {
    final pk = identityStream.value;
    if (pk == null) {
      throw StateError(
        'nostr identity not configured — '
        'call ensureIdentity() before accessing myPubkey',
      );
    }
    return pk;
  }

  /// Whether a nostr identity has been configured.
  bool get hasIdentity => identityStream.value != null;

  /// The single way to obtain the nostr signing identity. Shows the
  /// setup dialog if no identity exists. Returns the nsec string,
  /// or null if the user cancels.
  Future<String?> ensureIdentity(BuildContext context) async {
    try {
      return nostrSettings.getNsec();
    } catch (_) {}
    final result = await showNostrSetupDialog(context);
    if (result == NostrSetupResult.cancelled || !context.mounted) return null;
    try {
      return nostrSettings.getNsec();
    } catch (_) {
      return null;
    }
  }

  /// Get a stream of profile updates for a pubkey.
  Stream<NostrProfile?> profileStream(PublicKey pubkey) {
    final hex = pubkey.toHex();
    _ensureProfileSubject(hex, pubkey);
    return _profileCache[hex]!.stream;
  }

  /// Get cached profile for a pubkey (may be null if not yet fetched).
  NostrProfile? getProfile(PublicKey pubkey) {
    final hex = pubkey.toHex();
    _ensureProfileSubject(hex, pubkey);
    return _profileCache[hex]!.value;
  }

  void _ensureProfileSubject(String hex, PublicKey pubkey) {
    if (!_profileCache.containsKey(hex)) {
      _profileCache[hex] = BehaviorSubject.seeded(null);
      _fetchProfile(pubkey);
    }
  }

  Future<void> _fetchProfile(PublicKey pubkey) async {
    final hex = pubkey.toHex();

    if (_fetchingProfiles.contains(hex)) return;
    _fetchingProfiles.add(hex);

    try {
      _client ??= await NostrClient.connect();
      final profile = await _client!.fetchProfile(pubkey: pubkey);
      _profileCache[hex]?.add(profile);
    } catch (e) {
      debugPrint('Failed to fetch profile for $hex: $e');
    } finally {
      _fetchingProfiles.remove(hex);
    }
  }

  /// Update the profile cache from channel events.
  void updateProfilesFromChannel(List<GroupMember> members) {
    for (final member in members) {
      final hex = member.pubkey.toHex();
      if (!_profileCache.containsKey(hex)) {
        _profileCache[hex] = BehaviorSubject.seeded(member.profile);
      } else if (member.profile != null) {
        _profileCache[hex]!.add(member.profile);
      }
    }
  }

  /// Update the profile cache for a single member — used for
  /// per-author `ChannelEvent.memberProfileUpdated` events folded
  /// from in-channel kind 0 publications.
  void updateMemberProfile(PublicKey pubkey, NostrProfile profile) {
    final hex = pubkey.toHex();
    if (!_profileCache.containsKey(hex)) {
      _profileCache[hex] = BehaviorSubject.seeded(profile);
    } else {
      _profileCache[hex]!.add(profile);
    }
  }

  /// Lazily-initialized shared `NostrClient` for profile fetches and
  /// for callers that need to dispatch to channels (e.g. the profile
  /// editor's publish-in-all-channels save). On first access, pushes
  /// the current identity's publish credentials into the client so
  /// auto-publish on channel-connect works without an explicit save
  /// roundtrip after restart.
  Future<NostrClient> get nostrClient async {
    if (_client == null) {
      _client = await NostrClient.connect();
      refreshPublishCredentials(_client!);
    }
    return _client!;
  }

  /// Sync the client's auto-publish snapshot to whatever the current
  /// identity looks like. Called after every identity mutation
  /// (generate, import, clear) so the next channel-connect auto-publish
  /// uses fresh credentials. No-op in Mode A (Imported) — Mode A never
  /// publishes in-channel.
  void refreshPublishCredentials(NostrClient client) {
    final id = nostrSettings.currentIdentity();
    NostrProfile? profile;
    String? nsec;
    if (id is UserIdentity_Generated) {
      try {
        nsec = nostrSettings.getNsec();
        profile = NostrProfile(pubkey: id.pubkey, name: id.name);
      } catch (_) {
        // No nsec configured — nothing to publish.
      }
    }
    client.setLocalPublishCredentials(profile: profile, nsec: nsec);
  }

  @override
  bool updateShouldNotify(NostrContext oldWidget) => false;
}

/// Builder widget for reactive identity updates. The pubkey is non-nullable
/// because all widgets using this are downstream of the identity gate.
class NostrIdentityBuilder extends StatelessWidget {
  final Widget Function(BuildContext, PublicKey) builder;

  const NostrIdentityBuilder({super.key, required this.builder});

  @override
  Widget build(BuildContext context) {
    final nostr = NostrContext.of(context);
    return StreamBuilder<PublicKey?>(
      stream: nostr.identityStream,
      initialData: nostr.identityStream.value,
      builder: (context, snap) {
        final pk = snap.data;
        if (pk == null) return const SizedBox.shrink();
        return builder(context, pk);
      },
    );
  }
}

/// StreamBuilder wrapper for the current user's profile based on identity.
/// Requires a nostr identity to be configured (gate via
/// `ensureIdentity` before rendering).
class MyProfileBuilder extends StatelessWidget {
  final Widget Function(
    BuildContext context,
    PublicKey pubkey,
    NostrProfile? profile,
  ) builder;

  const MyProfileBuilder({super.key, required this.builder});

  @override
  Widget build(BuildContext context) {
    final nostr = NostrContext.of(context);
    final pubkey = nostr.myPubkey;
    return StreamBuilder<NostrProfile?>(
      stream: nostr.profileStream(pubkey),
      initialData: nostr.getProfile(pubkey),
      builder: (context, profileSnap) {
        return builder(context, pubkey, profileSnap.data);
      },
    );
  }
}

/// StreamBuilder wrapper for any user's profile.
class ProfileBuilder extends StatelessWidget {
  final PublicKey pubkey;
  final Widget Function(BuildContext context, NostrProfile? profile) builder;

  const ProfileBuilder({
    super.key,
    required this.pubkey,
    required this.builder,
  });

  @override
  Widget build(BuildContext context) {
    final nostr = NostrContext.of(context);
    return StreamBuilder<NostrProfile?>(
      stream: nostr.profileStream(pubkey),
      initialData: nostr.getProfile(pubkey),
      builder: (context, snapshot) {
        return builder(context, snapshot.data);
      },
    );
  }
}
