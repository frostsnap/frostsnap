use super::super_wallet::SuperWallet;
use bitcoin::Address;
use flutter_rust_bridge::frb;
use frostsnap_coordinator::bitcoin::address_search::{self, ADDRESS_SEARCH_STEP};
use frostsnap_coordinator::bitcoin::wallet::AddressInfo;
use frostsnap_core::MasterAppkey;

#[frb(sync)]
pub fn address_search_step() -> u32 {
    ADDRESS_SEARCH_STEP
}

impl SuperWallet {
    /// Searches the receive and change addresses from index `start` for one
    /// [`address_search_step`].
    pub fn search_for_address(
        &self,
        master_appkey: MasterAppkey,
        address: &Address,
        start: u32,
    ) -> Option<AddressInfo> {
        let found =
            address_search::search_for_address(master_appkey, &address.script_pubkey(), start);
        found.map(|path| {
            self.inner
                .lock()
                .unwrap()
                .address_info_for_path(master_appkey, path)
        })
    }
}
