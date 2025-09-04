use crate::{frb_generated::StreamSink, sink_wrap::SinkWrap};
use anyhow::Result;
use bitcoin::Network as BitcoinNetwork;
use flutter_rust_bridge::frb;
pub use frostsnap_coordinator::enter_physical_backup::EnterPhysicalBackupState;
pub use frostsnap_coordinator::wait_for_recovery_share::WaitForRecoveryShareState;
pub use frostsnap_core::coordinator::restoration::{
    PhysicalBackupPhase, RecoverShare, RecoverShareError, RecoveringAccessStructure,
    RestorationProblem, RestorationShare, RestorationShareValidity, RestorationState,
};
use frostsnap_core::{
    device::KeyPurpose, AccessStructureRef, DeviceId, RestorationId, SymmetricKey,
};

pub use frostsnap_core::{message::HeldShare, schnorr_fun::frost::ShareImage};
use std::collections::HashSet;
use tracing::{event, Level};

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
        threshold: u16,
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

    pub fn continue_restoring_wallet_from_device_share(
        &self,
        restoration_id: RestorationId,
        recover_share: &RecoverShare,
    ) -> Result<()> {
        self.0
            .continue_restoring_wallet_from_device_share(restoration_id, recover_share)
    }

    #[frb(sync)]
    pub fn restoration_check_share_compatible(
        &self,
        restoration_id: RestorationId,
        recover_share: &RecoverShare,
    ) -> ShareCompatibility {
        use frostsnap_core::coordinator::restoration::RestoreRecoverShareError::*;
        match self
            .0
            .inner()
            .check_recover_share_compatible_with_restoration(restoration_id, &recover_share)
        {
            Ok(_) => ShareCompatibility::Compatible,
            Err(e) => match e {
                NameMismatch => ShareCompatibility::NameMismatch,
                AcccessStructureMismatch | UnknownRestorationId | PurposeNotCompatible => {
                    ShareCompatibility::Incompatible
                }
                AlreadyGotThisShare => ShareCompatibility::AlreadyGotIt,
                ConflictingShareImage { conflicts_with } => ShareCompatibility::ConflictsWith {
                    device_id: conflicts_with,
                    index: recover_share
                        .held_share
                        .share_image
                        .index
                        .try_into()
                        .expect("should be small"),
                },
            },
        }
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

    #[frb(sync)]
    pub fn check_recover_share_compatible(
        &self,
        access_structure_ref: AccessStructureRef,
        recover_share: &RecoverShare,
        encryption_key: SymmetricKey,
    ) -> ShareCompatibility {
        let res = self.0.inner().check_recover_share_compatible_with_key(
            access_structure_ref,
            recover_share,
            encryption_key,
        );

        match res {
            Ok(_) => ShareCompatibility::Compatible,
            Err(e) => match e {
                RecoverShareError::AlreadyGotThisShare => ShareCompatibility::AlreadyGotIt,
                RecoverShareError::NoSuchAccessStructure => ShareCompatibility::Incompatible,
                RecoverShareError::ShareImageIsWrong => ShareCompatibility::Incompatible,
                RecoverShareError::DecryptionError => {
                    event!(Level::ERROR, "share decryption error");
                    ShareCompatibility::Incompatible
                }
                RecoverShareError::AccessStructureMismatch => ShareCompatibility::Incompatible,
            },
        }
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

    #[frb(sync)]
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

    #[frb(sync)]
    pub fn check_physical_backup_compatible(
        &self,
        restoration_id: RestorationId,
        phase: &PhysicalBackupPhase,
    ) -> ShareCompatibility {
        use frostsnap_core::coordinator::restoration::RestorePhysicalBackupError::*;
        let res = self
            .0
            .inner()
            .check_physical_backup_compatible_with_restoration(restoration_id, *phase);

        match res {
            Ok(_) => ShareCompatibility::Compatible,
            Err(e) => match e {
                UnknownRestorationId => ShareCompatibility::Incompatible,
                AlreadyGotThisShare => ShareCompatibility::AlreadyGotIt,
                ConflictingShareImage { conflicts_with } => ShareCompatibility::ConflictsWith {
                    device_id: conflicts_with,
                    index: phase
                        .backup
                        .share_image
                        .index
                        .try_into()
                        .expect("should be small"),
                },
            },
        }
    }
}

pub enum ShareCompatibility {
    Compatible,
    AlreadyGotIt,
    Incompatible,
    NameMismatch,
    ConflictsWith { device_id: DeviceId, index: u16 },
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
    pub held_share: HeldShare,
}

#[frb(mirror(HeldShare))]
pub struct _HeldShare {
    pub access_structure_ref: Option<AccessStructureRef>,
    pub share_image: ShareImage,
    pub threshold: u16,
    pub key_name: String,
    pub purpose: KeyPurpose,
}

#[frb(mirror(RestorationState))]
pub struct _RestorationState {
    pub restoration_id: RestorationId,
    pub key_name: String,
    pub access_structure_ref: Option<AccessStructureRef>,
    pub access_structure: RecoveringAccessStructure,
    pub need_to_consolidate: HashSet<DeviceId>,
    pub key_purpose: KeyPurpose,
}

#[frb(mirror(RecoveringAccessStructure))]
struct _RecoveringAccessStructure {
    pub threshold: u16,
    pub share_images: Vec<(DeviceId, ShareImage)>,
}

#[frb(external)]
impl KeyPurpose {
    #[frb(sync)]
    pub fn bitcoin_network(&self) -> Option<BitcoinNetwork> {}
}

#[frb(mirror(RestorationShareValidity))]
pub enum _RestorationShareValidity {
    Valid,
    Invalid,
    Unknown,
}

#[derive(Clone, Debug)]
pub struct RestoringKey {
    pub restoration_id: RestorationId,
    pub name: String,
    pub threshold: u16,
    pub shares_obtained: Vec<RestorationShare>,
    pub bitcoin_network: Option<BitcoinNetwork>,
    pub problem: Option<RestorationProblem>,
}

#[frb(mirror(RestorationShare))]
pub struct _RestorationShare {
    pub device_id: DeviceId,
    pub index: u16,
    pub validity: RestorationShareValidity,
}

#[frb(mirror(RestorationProblem))]
pub enum _RestorationProblem {
    NotEnoughShares { need_more: u16 },
    InvalidShares,
}
