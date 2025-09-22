pub use crate::api::KeyPurpose;
use crate::{frb_generated::StreamSink, sink_wrap::SinkWrap};
use anyhow::Result;
use bitcoin::Network as BitcoinNetwork;
use flutter_rust_bridge::frb;
pub use frostsnap_coordinator::enter_physical_backup::EnterPhysicalBackupState;
pub use frostsnap_coordinator::wait_for_recovery_share::WaitForRecoveryShareState;
pub use frostsnap_core::coordinator::restoration::{
    PhysicalBackupPhase, RecoverShare, RecoverShareError, RecoverShareErrorKind,
    RecoveringAccessStructure, RestorationShare, RestorationState, RestorationStatus,
    RestorePhysicalBackupError, RestoreRecoverShareError, ShareCompatibility, ShareCount,
};
pub use frostsnap_core::coordinator::{KeyLocationState, ShareLocation};
pub use frostsnap_core::{
    message::HeldShare2,
    schnorr_fun::frost::{Fingerprint, ShareImage, ShareIndex, SharedKey},
};
use frostsnap_core::{AccessStructureRef, DeviceId, RestorationId, SymmetricKey};
use std::collections::HashSet;
use std::fmt;

#[frb(mirror(WaitForRecoveryShareState))]
pub struct _WaitForRecoveryShareState {
    pub shares: Vec<RecoverShare>,
    pub connected: HashSet<DeviceId>,
    pub blank: HashSet<DeviceId>,
}

impl super::coordinator::Coordinator {
    pub fn wait_for_recovery_share(
        &self,
        sink: StreamSink<WaitForRecoveryShareState>,
    ) -> Result<()> {
        self.0.wait_for_recovery_share(SinkWrap(sink));
        Ok(())
    }

    pub fn start_restoring_wallet(
        &self,
        name: String,
        threshold: Option<u16>,
        network: BitcoinNetwork,
    ) -> Result<RestorationId> {
        self.0
            .start_restoring_wallet(name, threshold, KeyPurpose::Bitcoin(network))
    }

    pub fn start_restoring_wallet_from_device_share(
        &self,
        recover_share: &RecoverShare,
    ) -> Result<RestorationId> {
        self.0
            .start_restoring_wallet_from_device_share(recover_share)
    }

    pub fn check_continue_restoring_wallet_from_device_share(
        &self,
        restoration_id: RestorationId,
        recover_share: &RecoverShare,
        encryption_key: SymmetricKey,
    ) -> Option<RestoreRecoverShareError> {
        self.0
            .inner()
            .check_recover_share_compatible_with_restoration(
                restoration_id,
                recover_share,
                encryption_key,
            )
            .err()
    }

    pub fn continue_restoring_wallet_from_device_share(
        &self,
        restoration_id: RestorationId,
        recover_share: &RecoverShare,
        encryption_key: SymmetricKey,
    ) -> Result<()> {
        self.0.continue_restoring_wallet_from_device_share(
            restoration_id,
            recover_share,
            encryption_key,
        )
    }

    pub fn finish_restoring(
        &self,
        restoration_id: RestorationId,
        encryption_key: SymmetricKey,
    ) -> Result<AccessStructureRef> {
        self.0.finish_restoring(restoration_id, encryption_key)
    }

    #[frb(sync)]
    pub fn get_restoration_state(&self, restoration_id: RestorationId) -> Option<RestorationState> {
        self.0.get_restoration_state(restoration_id)
    }

    pub fn cancel_restoration(&self, restoration_id: RestorationId) -> Result<()> {
        self.0.cancel_restoration(restoration_id)
    }

    pub fn check_recover_share(
        &self,
        access_structure_ref: AccessStructureRef,
        recover_share: &RecoverShare,
        encryption_key: SymmetricKey,
    ) -> Option<RecoverShareError> {
        self.0
            .inner()
            .check_recover_share_compatible_with_key(
                access_structure_ref,
                recover_share,
                encryption_key,
            )
            .err()
    }

    pub fn check_start_restoring_key_from_device_share(
        &self,
        recover_share: &RecoverShare,
        encryption_key: SymmetricKey,
    ) -> Option<StartRestorationError> {
        // Use find_share to check if this share already exists
        if let Some(location) = self
            .0
            .inner()
            .find_share(recover_share.held_share.share_image, encryption_key)
        {
            return Some(StartRestorationError::ShareBelongsElsewhere {
                location: Box::new(location),
            });
        }
        None
    }

    pub fn check_physical_backup(
        &self,
        access_structure_ref: AccessStructureRef,
        phase: &PhysicalBackupPhase,
        encryption_key: SymmetricKey,
    ) -> bool {
        self.0
            .inner()
            .check_physical_backup(access_structure_ref, *phase, encryption_key)
            .is_ok()
    }

    pub fn recover_share(
        &self,
        access_structure_ref: AccessStructureRef,
        recover_share: &RecoverShare,
        encryption_key: SymmetricKey,
    ) -> Result<()> {
        self.0
            .recover_share(access_structure_ref, recover_share, encryption_key)
    }

    pub fn tell_device_to_enter_physical_backup(
        &self,
        device_id: DeviceId,
        sink: StreamSink<EnterPhysicalBackupState>,
    ) -> Result<()> {
        self.0
            .tell_device_to_enter_physical_backup(device_id, SinkWrap(sink))?;

        Ok(())
    }

    pub fn check_physical_backup_for_restoration(
        &self,
        restoration_id: RestorationId,
        phase: &PhysicalBackupPhase,
        encryption_key: SymmetricKey,
    ) -> Option<RestorePhysicalBackupError> {
        self.0
            .inner()
            .check_physical_backup_compatible_with_restoration(
                restoration_id,
                *phase,
                encryption_key,
            )
            .err()
    }

    pub fn tell_device_to_save_physical_backup(
        &self,
        phase: &PhysicalBackupPhase,
        restoration_id: RestorationId,
    ) {
        self.0
            .tell_device_to_save_physical_backup(*phase, restoration_id)
    }

    pub fn tell_device_to_consolidate_physical_backup(
        &self,
        access_structure_ref: AccessStructureRef,
        phase: &PhysicalBackupPhase,
        encryption_key: SymmetricKey,
    ) -> anyhow::Result<()> {
        self.0.tell_device_to_consolidate_physical_backup(
            access_structure_ref,
            *phase,
            encryption_key,
        )?;
        Ok(())
    }

    pub fn exit_recovery_mode(&self, device_id: DeviceId, encryption_key: SymmetricKey) {
        self.0.exit_recovery_mode(device_id, encryption_key);
    }

    pub fn delete_restoration_share(
        &self,
        restoration_id: RestorationId,
        device_id: DeviceId,
    ) -> Result<()> {
        self.0.delete_restoration_share(restoration_id, device_id)
    }
}

#[frb(external)]
impl PhysicalBackupPhase {
    #[frb(sync)]
    pub fn device_id(&self) -> DeviceId {}

    #[frb(sync)]
    pub fn share_image(&self) -> ShareImage {}
}

#[derive(Debug, Clone)]
#[frb(mirror(EnterPhysicalBackupState))]
pub struct _EnterPhysicalBackupState {
    pub device_id: DeviceId,
    pub entered: Option<PhysicalBackupPhase>,
    /// null if the user is entering the backup not to save but to check it
    pub saved: bool,
    pub abort: Option<String>,
}

#[frb(mirror(RecoverShare))]
pub struct _RecoverShare {
    pub held_by: DeviceId,
    pub held_share: HeldShare2,
}

#[frb(mirror(HeldShare2))]
pub struct _HeldShare2 {
    pub access_structure_ref: Option<AccessStructureRef>,
    pub share_image: ShareImage,
    pub threshold: Option<u16>,
    pub key_name: Option<String>,
    pub purpose: Option<KeyPurpose>,
    pub needs_consolidation: bool,
}

#[frb(mirror(RestorationState))]
pub struct _RestorationState {
    pub restoration_id: RestorationId,
    pub key_name: String,
    pub access_structure: RecoveringAccessStructure,
    pub key_purpose: KeyPurpose,
    pub fingerprint: Fingerprint,
}

#[frb(external)]
impl RestorationState {
    #[frb(sync)]
    pub fn status(&self) -> RestorationStatus {}

    #[frb(sync)]
    pub fn is_restorable(&self) -> bool {}
}

#[frb(mirror(RecoveringAccessStructure))]
struct _RecoveringAccessStructure {
    pub starting_threshold: Option<u16>,
    pub held_shares: Vec<RecoverShare>,
    pub shared_key: Option<SharedKey>,
}

#[frb(external)]
impl RecoveringAccessStructure {
    #[frb(sync)]
    pub fn effective_threshold(&self) -> Option<u16> {}
}

#[frb(mirror(RestorationStatus))]
pub struct _RestorationStatus {
    pub threshold: Option<u16>,
    pub shares: Vec<RestorationShare>,
    pub shared_key: Option<SharedKey>,
}

#[frb(external)]
impl RestorationStatus {
    #[frb(sync)]
    pub fn share_count(&self) -> ShareCount {}
}

#[frb(mirror(ShareCount))]
pub struct _ShareCount {
    pub got: u16,
    pub needed: Option<u16>,
    pub incompatible: u16,
}

#[frb(mirror(ShareCompatibility))]
pub enum _ShareCompatibility {
    Compatible,
    Incompatible,
    Uncertain,
}

#[frb(mirror(RestorationShare))]
pub struct _RestorationShare {
    pub device_id: DeviceId,
    pub index: u16,
    pub compatibility: ShareCompatibility,
}

#[frb(mirror(ShareLocation))]
pub struct _ShareLocation {
    pub device_ids: Vec<DeviceId>,
    pub share_index: ShareIndex,
    pub key_name: String,
    pub key_state: KeyLocationState,
}

#[frb(mirror(KeyLocationState))]
pub enum _KeyLocationState {
    Complete {
        access_structure_ref: AccessStructureRef,
    },
    Restoring {
        restoration_id: RestorationId,
    },
}

#[frb(mirror(RestoreRecoverShareError))]
pub enum _RestoreRecoverShareError {
    UnknownRestorationId,
    AcccessStructureMismatch,
    AlreadyGotThisShare,
    ShareBelongsElsewhere { location: Box<ShareLocation> },
}

#[frb(mirror(RestorePhysicalBackupError))]
pub enum _RestorePhysicalBackupError {
    UnknownRestorationId,
    AlreadyGotThisShare,
    ShareBelongsElsewhere { location: Box<ShareLocation> },
}

#[frb(mirror(RecoverShareError))]
pub struct _RecoverShareError {
    pub key_purpose: KeyPurpose,
    pub kind: RecoverShareErrorKind,
}

#[frb(mirror(RecoverShareErrorKind))]
pub enum _RecoverShareErrorKind {
    AlreadyGotThisShare,
    NoSuchAccessStructure,
    AccessStructureMismatch,
    ShareImageIsWrong,
    DecryptionError,
}

#[derive(Debug, Clone)]
pub enum StartRestorationError {
    ShareBelongsElsewhere { location: Box<ShareLocation> },
}

impl fmt::Display for StartRestorationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StartRestorationError::ShareBelongsElsewhere { location } => {
                write!(f, "This key share belongs to existing {} '{}' and cannot be used to start a new restoration", location.key_purpose.key_type_noun(), location.key_name)
            }
        }
    }
}

impl std::error::Error for StartRestorationError {}

#[frb(external)]
impl StartRestorationError {
    #[frb(sync)]
    pub fn to_string(&self) -> String {}
}

#[frb(external)]
impl RestoreRecoverShareError {
    #[frb(sync)]
    pub fn to_string(&self) -> String {}
}

#[frb(external)]
impl RestorePhysicalBackupError {
    #[frb(sync)]
    pub fn to_string(&self) -> String {}
}

#[frb(external)]
impl RecoverShareError {
    #[frb(sync)]
    pub fn to_string(&self) -> String {}
}
