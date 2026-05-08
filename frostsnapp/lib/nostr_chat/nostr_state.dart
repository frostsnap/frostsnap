import 'package:flutter/material.dart';
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

  /// Per-access-structure settings cache. Each entry holds the Rust
  /// opaque subscription handle alongside the `BehaviorSubject` that
  /// fans its events out to Dart consumers.
  ///
  /// **Why the handle is retained**: `subAccessStructure(...).start()`
  /// returns only the underlying Stream — the opaque subscription
  /// object itself is dropped when the call expression goes out of
  /// scope. Rust's `Drop` on that object calls `_stop()`, which
  /// removes the sink from the broadcast's subscriber map and
  /// silently halts updates. Holding the handle in this cache keeps
  /// the Rust side alive for the lifetime of this `NostrContext`.
  final Map<AccessStructureId, _AccessStructureWatch> _accessStructureCache =
      {};

  NostrContext({super.key, required this.nostrSettings, required super.child}) {
    identityStream = nostrSettings.subIdentity().toBehaviorSubject();
  }

  /// Long-lived `BehaviorSubject` for an access structure's settings.
  /// Created on first call and reused for the lifetime of this
  /// `NostrContext`. Consumers should call `.stream` (or use it
  /// directly) — the underlying Rust subscription is held by this
  /// context, not by individual widgets.
  BehaviorSubject<AccessStructureSettings> watchAccessStructure(
    AccessStructureRef asRef,
  ) {
    final cached = _accessStructureCache[asRef.accessStructureId];
    if (cached != null) return cached.subject;
    final sub = nostrSettings.subAccessStructure(accessStructureRef: asRef);
    final subject = sub.start().toBehaviorSubject();
    _accessStructureCache[asRef.accessStructureId] = _AccessStructureWatch(
      sub: sub,
      subject: subject,
    );
    return subject;
  }

  /// Convenience: `Stream<bool>` over a wallet's coordination-UI flag.
  Stream<bool> watchCoordinationUi(AccessStructureRef asRef) =>
      watchAccessStructure(asRef).map((s) => s.coordinationUiEnabled);

  /// Sync read of the current coordination-UI flag (sources from the
  /// `BehaviorSubject` cache if present, otherwise the sync getter).
  bool isCoordinationUiEnabled(AccessStructureRef asRef) {
    final cached =
        _accessStructureCache[asRef.accessStructureId]?.subject.valueOrNull;
    if (cached != null) return cached.coordinationUiEnabled;
    return nostrSettings.isCoordinationUiEnabled(
      accessStructureId: asRef.accessStructureId,
    );
  }

  static NostrContext of(BuildContext context) {
    final widget = context.dependOnInheritedWidgetOfExactType<NostrContext>();
    assert(widget != null, 'No NostrContext found in widget tree');
    return widget!;
  }

  static NostrContext? maybeOf(BuildContext context) {
    return context.dependOnInheritedWidgetOfExactType<NostrContext>();
  }

  // Convenience accessor — current local nostr pubkey, or null if no
  // identity is configured. Dart computes the bech32 `npub` form on
  // demand via `myPubkey?.toNpub()`.
  PublicKey? get myPubkey => identityStream.value;

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
  final Widget Function(BuildContext, PublicKey?) builder;

  const NostrIdentityBuilder({super.key, required this.builder});

  @override
  Widget build(BuildContext context) {
    final nostr = NostrContext.of(context);
    return StreamBuilder<PublicKey?>(
      stream: nostr.identityStream,
      initialData: nostr.myPubkey,
      builder: (context, snap) => builder(context, snap.data),
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
    return StreamBuilder<PublicKey?>(
      stream: nostr.identityStream,
      initialData: nostr.myPubkey,
      builder: (context, identitySnap) {
        final pubkey = identitySnap.data;
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

/// Pairs a Rust opaque `AccessStructureSettingsBroadcastSubscription`
/// with the `BehaviorSubject` that re-broadcasts its stream to Dart
/// consumers. Both must be retained together: the subject keeps Dart
/// listeners hot, and the subscription handle keeps the Rust side
/// from `Drop`-ing the underlying broadcast registration.
class _AccessStructureWatch {
  _AccessStructureWatch({required this.sub, required this.subject});

  // ignore: unused_field
  final AccessStructureSettingsBroadcastSubscription sub;
  final BehaviorSubject<AccessStructureSettings> subject;
}
