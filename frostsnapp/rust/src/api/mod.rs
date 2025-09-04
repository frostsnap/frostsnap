pub mod backup_manager;
pub mod bitcoin;
pub mod coordinator;
pub mod device_list;
pub mod firmware_upgrade;
pub mod init;
pub mod keygen;
pub mod log;
pub mod port;
pub mod qr;
pub mod recovery;
pub mod settings;
pub mod signing;
pub mod super_wallet;

use flutter_rust_bridge::frb;

use frostsnap_coordinator::frostsnap_core;

pub use frostsnap_core::{
    message::EncodedSignature, AccessStructureId, AccessStructureRef, DeviceId, KeyId, KeygenId,
    MasterAppkey, RestorationId, SessionHash, SignSessionId, SymmetricKey,
};

#[frb(mirror(KeygenId))]
pub struct _KeygenId(pub [u8; 16]);

#[frb(mirror(AccessStructureId))]
pub struct _AccessStructureId(pub [u8; 32]);

#[frb(mirror(DeviceId))]
pub struct _DeviceId(pub [u8; 33]);

#[frb(mirror(MasterAppkey))]
pub struct _MasterAppkey(pub [u8; 65]);

#[frb(mirror(KeyId))]
pub struct _KeyId(pub [u8; 32]);

#[frb(mirror(SessionHash))]
pub struct _SessionHash(pub [u8; 32]);

#[frb(mirror(EncodedSignature))]
pub struct _EncodedSignature(pub [u8; 64]);

#[frb(mirror(SignSessionId))]
pub struct _SignSessionId(pub [u8; 32]);

#[frb(mirror(AccessStructureRef))]
pub struct _AccessStructureRef {
    pub key_id: KeyId,
    pub access_structure_id: AccessStructureId,
}

#[frb(mirror(RestorationId))]
pub struct _RestorattionId([u8; 16]);

#[frb(mirror(SymmetricKey))]
pub struct _SymmetricKey(pub [u8; 32]);

pub struct Api {}

impl Api {}

#[frb(external)]
impl MasterAppkey {
    #[frb(sync)]
    pub fn key_id(&self) -> KeyId {}
}
