pub mod backup_run;
pub mod bitcoin;
pub mod broadcast;
pub mod camera;
pub mod coordinator;
pub mod device_list;
pub mod firmware;
pub mod init;
pub mod keygen;
pub mod log;
pub mod name;
pub mod nonce_replenish;
pub mod nostr;
pub mod port;
pub mod psbt_manager;
pub mod qr;
pub mod recovery;
pub mod settings;
pub mod signing;
pub mod super_wallet;
pub mod transaction;

use flutter_rust_bridge::frb;

use frostsnap_coordinator::frostsnap_core;

pub use frostsnap_core::{
    device::KeyPurpose, message::EncodedSignature, AccessStructureId, AccessStructureRef, DeviceId,
    KeyId, KeygenId, MasterAppkey, RestorationId, SessionHash, SignSessionId, SymmetricKey,
};

// Wrapped-bytes IDs each need byte-content `==` / `hashCode` on the Dart
// side; the FRB defaults delegate to `field0`, which is a
// `NonGrowableListView<int>` using reference identity. Each mirror below
// injects the same content-equality override, matching the pattern on
// `_PublicKey` / `EventId` in `api/nostr/mod.rs`.

#[frb(
    mirror(KeygenId),
    non_hash,
    non_eq,
    dart_code = "
  @override
  int get hashCode => Object.hashAll(field0);

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is KeygenId && _listEquals(field0, other.field0));

  static bool _listEquals(List<int> a, List<int> b) {
    if (a.length != b.length) return false;
    for (int i = 0; i < a.length; i++) {
      if (a[i] != b[i]) return false;
    }
    return true;
  }
"
)]
pub struct _KeygenId(pub [u8; 32]);

#[frb(
    mirror(AccessStructureId),
    non_hash,
    non_eq,
    dart_code = "
  @override
  int get hashCode => Object.hashAll(field0);

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is AccessStructureId && _listEquals(field0, other.field0));

  static bool _listEquals(List<int> a, List<int> b) {
    if (a.length != b.length) return false;
    for (int i = 0; i < a.length; i++) {
      if (a[i] != b[i]) return false;
    }
    return true;
  }
"
)]
pub struct _AccessStructureId(pub [u8; 32]);

#[frb(
    mirror(DeviceId),
    non_hash,
    non_eq,
    dart_code = "
  @override
  int get hashCode => Object.hashAll(field0);

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is DeviceId && _listEquals(field0, other.field0));

  static bool _listEquals(List<int> a, List<int> b) {
    if (a.length != b.length) return false;
    for (int i = 0; i < a.length; i++) {
      if (a[i] != b[i]) return false;
    }
    return true;
  }
"
)]
pub struct _DeviceId(pub [u8; 33]);

#[frb(
    mirror(MasterAppkey),
    non_hash,
    non_eq,
    dart_code = "
  @override
  int get hashCode => Object.hashAll(field0);

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is MasterAppkey && _listEquals(field0, other.field0));

  static bool _listEquals(List<int> a, List<int> b) {
    if (a.length != b.length) return false;
    for (int i = 0; i < a.length; i++) {
      if (a[i] != b[i]) return false;
    }
    return true;
  }
"
)]
pub struct _MasterAppkey(pub [u8; 65]);

#[frb(
    mirror(KeyId),
    non_hash,
    non_eq,
    dart_code = "
  @override
  int get hashCode => Object.hashAll(field0);

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is KeyId && _listEquals(field0, other.field0));

  static bool _listEquals(List<int> a, List<int> b) {
    if (a.length != b.length) return false;
    for (int i = 0; i < a.length; i++) {
      if (a[i] != b[i]) return false;
    }
    return true;
  }
"
)]
pub struct _KeyId(pub [u8; 32]);

#[frb(
    mirror(SessionHash),
    non_hash,
    non_eq,
    dart_code = "
  @override
  int get hashCode => Object.hashAll(field0);

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is SessionHash && _listEquals(field0, other.field0));

  static bool _listEquals(List<int> a, List<int> b) {
    if (a.length != b.length) return false;
    for (int i = 0; i < a.length; i++) {
      if (a[i] != b[i]) return false;
    }
    return true;
  }
"
)]
pub struct _SessionHash(pub [u8; 32]);

#[frb(
    mirror(EncodedSignature),
    non_hash,
    non_eq,
    dart_code = "
  @override
  int get hashCode => Object.hashAll(field0);

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is EncodedSignature && _listEquals(field0, other.field0));

  static bool _listEquals(List<int> a, List<int> b) {
    if (a.length != b.length) return false;
    for (int i = 0; i < a.length; i++) {
      if (a[i] != b[i]) return false;
    }
    return true;
  }
"
)]
pub struct _EncodedSignature(pub [u8; 64]);

#[frb(
    mirror(SignSessionId),
    non_hash,
    non_eq,
    dart_code = "
  @override
  int get hashCode => Object.hashAll(field0);

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is SignSessionId && _listEquals(field0, other.field0));

  static bool _listEquals(List<int> a, List<int> b) {
    if (a.length != b.length) return false;
    for (int i = 0; i < a.length; i++) {
      if (a[i] != b[i]) return false;
    }
    return true;
  }
"
)]
pub struct _SignSessionId(pub [u8; 32]);

#[frb(mirror(AccessStructureRef))]
pub struct _AccessStructureRef {
    pub key_id: KeyId,
    pub access_structure_id: AccessStructureId,
}

#[frb(
    mirror(RestorationId),
    non_hash,
    non_eq,
    dart_code = "
  @override
  int get hashCode => Object.hashAll(field0);

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is RestorationId && _listEquals(field0, other.field0));

  static bool _listEquals(List<int> a, List<int> b) {
    if (a.length != b.length) return false;
    for (int i = 0; i < a.length; i++) {
      if (a[i] != b[i]) return false;
    }
    return true;
  }
"
)]
pub struct _RestorattionId([u8; 16]);

#[frb(
    mirror(SymmetricKey),
    non_hash,
    non_eq,
    dart_code = "
  @override
  int get hashCode => Object.hashAll(field0);

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is SymmetricKey && _listEquals(field0, other.field0));

  static bool _listEquals(List<int> a, List<int> b) {
    if (a.length != b.length) return false;
    for (int i = 0; i < a.length; i++) {
      if (a[i] != b[i]) return false;
    }
    return true;
  }
"
)]
pub struct _SymmetricKey(pub [u8; 32]);

pub struct Api {}

impl Api {}

#[frb(external)]
impl MasterAppkey {
    #[frb(sync)]
    pub fn key_id(&self) -> KeyId {}
}

use bitcoin::BitcoinNetwork;
#[frb(external)]
impl KeyPurpose {
    #[frb(sync)]
    pub fn bitcoin_network(&self) -> Option<BitcoinNetwork> {}
}

/// Build a `KeyPurpose::Test` from Dart (the variant is opaque in FRB,
/// so Dart can't construct it directly). Used by the remote-keygen
/// lobby while the production purpose story settles.
#[frb(sync)]
pub fn key_purpose_test() -> KeyPurpose {
    KeyPurpose::Test
}

/// Build a `KeyPurpose::Bitcoin(network)` from Dart.
#[frb(sync)]
pub fn key_purpose_bitcoin(network: BitcoinNetwork) -> KeyPurpose {
    KeyPurpose::Bitcoin(network)
}
