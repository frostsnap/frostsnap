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
pub mod port;
pub mod psbt_manager;
pub mod qr;
pub mod recovery;
pub mod send;
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

// FRB's default `==` compares `field0`, a list view with identity equality, so two decodes of the
// same ID would differ. `dart_code` can't splice in the class name, hence the runtime-type check.
macro_rules! bytes_id_mirror {
    ($mirror:ident, $name:ident, $len:literal) => {
        #[frb(
            mirror($name),
            non_hash,
            non_eq,
            dart_code = "
  @override
  int get hashCode => Object.hashAll(field0);

  @override
  bool operator ==(Object other) {
    if (identical(this, other)) return true;
    if (other.runtimeType != runtimeType) return false;
    final List<int> otherBytes = (other as dynamic).field0;
    if (otherBytes.length != field0.length) return false;
    for (var i = 0; i < field0.length; i++) {
      if (field0[i] != otherBytes[i]) return false;
    }
    return true;
  }
"
        )]
        pub struct $mirror(pub [u8; $len]);
    };
}

bytes_id_mirror!(_KeygenId, KeygenId, 16);
bytes_id_mirror!(_AccessStructureId, AccessStructureId, 32);
bytes_id_mirror!(_DeviceId, DeviceId, 33);
bytes_id_mirror!(_MasterAppkey, MasterAppkey, 65);
bytes_id_mirror!(_KeyId, KeyId, 32);
bytes_id_mirror!(_SessionHash, SessionHash, 32);
bytes_id_mirror!(_EncodedSignature, EncodedSignature, 64);
bytes_id_mirror!(_SignSessionId, SignSessionId, 32);
bytes_id_mirror!(_RestorationId, RestorationId, 16);
bytes_id_mirror!(_SymmetricKey, SymmetricKey, 32);

#[frb(mirror(AccessStructureRef))]
pub struct _AccessStructureRef {
    pub key_id: KeyId,
    pub access_structure_id: AccessStructureId,
}

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
