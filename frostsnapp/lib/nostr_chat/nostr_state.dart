import 'package:flutter/material.dart';
import 'package:frostsnap/src/rust/api/nostr.dart';
import 'package:frostsnap/stream_ext.dart';
import 'package:rxdart/rxdart.dart';

/// Provides NostrSettings to widget tree, following same pattern as SettingsContext.
// ignore: must_be_immutable
class NostrContext extends InheritedWidget {
  final NostrSettings nostrSettings;
  late final BehaviorSubject<FfiNostrIdentity> identityStream;

  // Profile cache for other users
  final Map<String, BehaviorSubject<NostrProfile?>> _profileCache = {};
  final Set<String> _fetchingProfiles = {};
  NostrClient? _client;

  NostrContext({super.key, required this.nostrSettings, required super.child}) {
    identityStream = nostrSettings.subIdentity().toBehaviorSubject();
  }

  static NostrContext of(BuildContext context) {
    final widget = context.dependOnInheritedWidgetOfExactType<NostrContext>();
    assert(widget != null, 'No NostrContext found in widget tree');
    return widget!;
  }

  static NostrContext? maybeOf(BuildContext context) {
    return context.dependOnInheritedWidgetOfExactType<NostrContext>();
  }

  // Convenience accessors
  FfiNostrIdentity get identity => identityStream.value;
  PublicKey? get myPubkey => identity.pubkey;

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

  @override
  bool updateShouldNotify(NostrContext oldWidget) => false;
}

/// Builder widget for reactive identity updates.
class NostrIdentityBuilder extends StatelessWidget {
  final Widget Function(BuildContext, FfiNostrIdentity) builder;

  const NostrIdentityBuilder({super.key, required this.builder});

  @override
  Widget build(BuildContext context) {
    final nostr = NostrContext.of(context);
    return StreamBuilder<FfiNostrIdentity>(
      stream: nostr.identityStream,
      initialData: nostr.identity,
      builder: (context, snap) => builder(context, snap.data!),
    );
  }
}

/// StreamBuilder wrapper for the current user's profile based on identity.
class MyProfileBuilder extends StatelessWidget {
  final Widget Function(
    BuildContext context,
    PublicKey? pubkey,
    NostrProfile? profile,
  )
  builder;

  const MyProfileBuilder({super.key, required this.builder});

  @override
  Widget build(BuildContext context) {
    final nostr = NostrContext.of(context);
    return StreamBuilder<FfiNostrIdentity>(
      stream: nostr.identityStream,
      initialData: nostr.identity,
      builder: (context, identitySnap) {
        final pubkey = identitySnap.data?.pubkey;
        if (pubkey == null) {
          return builder(context, null, null);
        }
        return StreamBuilder<NostrProfile?>(
          stream: nostr.profileStream(pubkey),
          initialData: nostr.getProfile(pubkey),
          builder: (context, profileSnap) {
            return builder(context, pubkey, profileSnap.data);
          },
        );
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
