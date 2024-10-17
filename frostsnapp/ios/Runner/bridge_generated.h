#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>
typedef struct _Dart_Handle* Dart_Handle;

typedef struct DartCObject DartCObject;

typedef int64_t DartPort;

typedef bool (*DartPostCObjectFnType)(DartPort port_id, void *message);

typedef struct DartCObject *WireSyncReturn;

typedef struct wire_uint_8_list {
  uint8_t *ptr;
  int32_t len;
} wire_uint_8_list;

typedef struct wire_DeviceId {
  struct wire_uint_8_list *field0;
} wire_DeviceId;

typedef struct wire_KeyId {
  struct wire_uint_8_list *field0;
} wire_KeyId;

typedef struct wire_ArcRTransaction {
  const void *ptr;
} wire_ArcRTransaction;

typedef struct wire_ConfirmationTime {
  uint32_t height;
  uint64_t time;
} wire_ConfirmationTime;

typedef struct wire_Transaction {
  int64_t net_value;
  struct wire_ArcRTransaction inner;
  struct wire_ConfirmationTime *confirmation_time;
} wire_Transaction;

typedef struct wire_ConnectedDevice {
  struct wire_uint_8_list *name;
  struct wire_uint_8_list *firmware_digest;
  struct wire_uint_8_list *latest_digest;
  struct wire_DeviceId id;
} wire_ConnectedDevice;

typedef struct wire_FrostsnapCoreCoordinatorCoordinatorFrostKey {
  const void *ptr;
} wire_FrostsnapCoreCoordinatorCoordinatorFrostKey;

typedef struct wire_FrostKey {
  struct wire_FrostsnapCoreCoordinatorCoordinatorFrostKey field0;
} wire_FrostKey;

typedef struct wire_PortOpenSender {
  const void *ptr;
} wire_PortOpenSender;

typedef struct wire_PortOpen {
  struct wire_uint_8_list *id;
  uint32_t baud_rate;
  struct wire_PortOpenSender ready;
} wire_PortOpen;

typedef struct wire_PortReadSender {
  const void *ptr;
} wire_PortReadSender;

typedef struct wire_PortRead {
  struct wire_uint_8_list *id;
  uintptr_t len;
  struct wire_PortReadSender ready;
} wire_PortRead;

typedef struct wire_PortWriteSender {
  const void *ptr;
} wire_PortWriteSender;

typedef struct wire_PortWrite {
  struct wire_uint_8_list *id;
  struct wire_uint_8_list *bytes;
  struct wire_PortWriteSender ready;
} wire_PortWrite;

typedef struct wire_PortBytesToReadSender {
  const void *ptr;
} wire_PortBytesToReadSender;

typedef struct wire_PortBytesToRead {
  struct wire_uint_8_list *id;
  struct wire_PortBytesToReadSender ready;
} wire_PortBytesToRead;

typedef struct wire_list_connected_device {
  struct wire_ConnectedDevice *ptr;
  int32_t len;
} wire_list_connected_device;

typedef struct wire_DeviceListState {
  struct wire_list_connected_device *devices;
  uintptr_t state_id;
} wire_DeviceListState;

typedef struct wire_ArcMutexFrostsnapWallet {
  const void *ptr;
} wire_ArcMutexFrostsnapWallet;

typedef struct wire_ArcWalletStreams {
  const void *ptr;
} wire_ArcWalletStreams;

typedef struct wire_ChainClient {
  const void *ptr;
} wire_ChainClient;

typedef struct wire_RBitcoinNetwork {
  const void *ptr;
} wire_RBitcoinNetwork;

typedef struct wire_BitcoinNetwork {
  struct wire_RBitcoinNetwork field0;
} wire_BitcoinNetwork;

typedef struct wire_Wallet {
  struct wire_ArcMutexFrostsnapWallet inner;
  struct wire_ArcWalletStreams wallet_streams;
  struct wire_ChainClient chain_sync;
  struct wire_BitcoinNetwork network;
} wire_Wallet;

typedef struct wire_StringList {
  struct wire_uint_8_list **ptr;
  int32_t len;
} wire_StringList;

typedef struct wire_RTransaction {
  const void *ptr;
} wire_RTransaction;

typedef struct wire_FrostsnapCoreBitcoinTransactionTransactionTemplate {
  const void *ptr;
} wire_FrostsnapCoreBitcoinTransactionTransactionTemplate;

typedef struct wire_UnsignedTx {
  struct wire_FrostsnapCoreBitcoinTransactionTransactionTemplate template_tx;
} wire_UnsignedTx;

typedef struct wire_SignedTx {
  struct wire_RTransaction signed_tx;
  struct wire_UnsignedTx unsigned_tx;
} wire_SignedTx;

typedef struct wire_BitcoinPsbt {
  const void *ptr;
} wire_BitcoinPsbt;

typedef struct wire_Psbt {
  struct wire_BitcoinPsbt inner;
} wire_Psbt;

typedef struct wire_ArcMutexVecPortDesc {
  const void *ptr;
} wire_ArcMutexVecPortDesc;

typedef struct wire_FfiSerial {
  struct wire_ArcMutexVecPortDesc available_ports;
} wire_FfiSerial;

typedef struct wire_PortDesc {
  struct wire_uint_8_list *id;
  uint16_t vid;
  uint16_t pid;
} wire_PortDesc;

typedef struct wire_list_port_desc {
  struct wire_PortDesc *ptr;
  int32_t len;
} wire_list_port_desc;

typedef struct wire_FfiCoordinator {
  const void *ptr;
} wire_FfiCoordinator;

typedef struct wire_Coordinator {
  struct wire_FfiCoordinator field0;
} wire_Coordinator;

typedef struct wire_list_device_id {
  struct wire_DeviceId *ptr;
  int32_t len;
} wire_list_device_id;

typedef struct wire_EncodedSignature {
  struct wire_uint_8_list *field0;
} wire_EncodedSignature;

typedef struct wire_list_encoded_signature {
  struct wire_EncodedSignature *ptr;
  int32_t len;
} wire_list_encoded_signature;

typedef struct wire_FfiQrReader {
  const void *ptr;
} wire_FfiQrReader;

typedef struct wire_QrReader {
  struct wire_FfiQrReader field0;
} wire_QrReader;

typedef struct wire_FfiQrEncoder {
  const void *ptr;
} wire_FfiQrEncoder;

typedef struct wire_QrEncoder {
  struct wire_FfiQrEncoder field0;
} wire_QrEncoder;

typedef struct wire_MutexPersistedRSettings {
  const void *ptr;
} wire_MutexPersistedRSettings;

typedef struct wire_ArcMutexRusqliteConnection {
  const void *ptr;
} wire_ArcMutexRusqliteConnection;

typedef struct wire_HashMapRBitcoinNetworkChainClient {
  const void *ptr;
} wire_HashMapRBitcoinNetworkChainClient;

typedef struct wire_PathBuf {
  const void *ptr;
} wire_PathBuf;

typedef struct wire_MutexHashMapRBitcoinNetworkWallet {
  const void *ptr;
} wire_MutexHashMapRBitcoinNetworkWallet;

typedef struct wire_MaybeSinkWalletSettings {
  const void *ptr;
} wire_MaybeSinkWalletSettings;

typedef struct wire_MaybeSinkDeveloperSettings {
  const void *ptr;
} wire_MaybeSinkDeveloperSettings;

typedef struct wire_MaybeSinkElectrumSettings {
  const void *ptr;
} wire_MaybeSinkElectrumSettings;

typedef struct wire_Settings {
  struct wire_MutexPersistedRSettings settings;
  struct wire_ArcMutexRusqliteConnection db;
  struct wire_HashMapRBitcoinNetworkChainClient chain_clients;
  struct wire_PathBuf app_directory;
  struct wire_MutexHashMapRBitcoinNetworkWallet loaded_wallets;
  struct wire_MaybeSinkWalletSettings wallet_settings_stream;
  struct wire_MaybeSinkDeveloperSettings developer_settings_stream;
  struct wire_MaybeSinkElectrumSettings electrum_settings_stream;
} wire_Settings;

void store_dart_post_cobject(DartPostCObjectFnType ptr);

Dart_Handle get_dart_object(uintptr_t ptr);

void drop_dart_object(uintptr_t ptr);

uintptr_t new_dart_opaque(Dart_Handle handle);

intptr_t init_frb_dart_api_dl(void *obj);

void wire_sub_port_events(int64_t port_);

void wire_sub_device_events(int64_t port_);

WireSyncReturn wire_log(int32_t level, struct wire_uint_8_list *message);

void wire_turn_stderr_logging_on(int64_t port_, int32_t level);

void wire_turn_logcat_logging_on(int64_t port_, int32_t level);

WireSyncReturn wire_device_at_index(uintptr_t index);

WireSyncReturn wire_device_list_state(void);

WireSyncReturn wire_get_connected_device(struct wire_DeviceId *id);

void wire_load(int64_t port_, struct wire_uint_8_list *app_dir);

void wire_load_host_handles_serial(int64_t port_, struct wire_uint_8_list *app_dir);

void wire_echo_key_id(int64_t port_, struct wire_KeyId *key_id);

WireSyncReturn wire_psbt_bytes_to_psbt(struct wire_uint_8_list *psbt_bytes);

void wire_new_qr_reader(int64_t port_);

void wire_new_qr_encoder(int64_t port_, struct wire_uint_8_list *bytes);

WireSyncReturn wire_txid__method__Transaction(struct wire_Transaction *that);

WireSyncReturn wire_ready__method__ConnectedDevice(struct wire_ConnectedDevice *that);

WireSyncReturn wire_needs_firmware_upgrade__method__ConnectedDevice(struct wire_ConnectedDevice *that);

WireSyncReturn wire_threshold__method__FrostKey(struct wire_FrostKey *that);

WireSyncReturn wire_id__method__FrostKey(struct wire_FrostKey *that);

WireSyncReturn wire_key_name__method__FrostKey(struct wire_FrostKey *that);

WireSyncReturn wire_devices__method__FrostKey(struct wire_FrostKey *that);

WireSyncReturn wire_polynomial_identifier__method__FrostKey(struct wire_FrostKey *that);

void wire_satisfy__method__PortOpen(int64_t port_,
                                    struct wire_PortOpen *that,
                                    struct wire_uint_8_list *err);

void wire_satisfy__method__PortRead(int64_t port_,
                                    struct wire_PortRead *that,
                                    struct wire_uint_8_list *bytes,
                                    struct wire_uint_8_list *err);

void wire_satisfy__method__PortWrite(int64_t port_,
                                     struct wire_PortWrite *that,
                                     struct wire_uint_8_list *err);

void wire_satisfy__method__PortBytesToRead(int64_t port_,
                                           struct wire_PortBytesToRead *that,
                                           uint32_t bytes_to_read);

WireSyncReturn wire_get_device__method__DeviceListState(struct wire_DeviceListState *that,
                                                        struct wire_DeviceId *id);

void wire_sub_tx_state__method__Wallet(int64_t port_,
                                       struct wire_Wallet *that,
                                       struct wire_KeyId *key_id);

WireSyncReturn wire_tx_state__method__Wallet(struct wire_Wallet *that, struct wire_KeyId *key_id);

void wire_sync_txids__method__Wallet(int64_t port_,
                                     struct wire_Wallet *that,
                                     struct wire_KeyId *key_id,
                                     struct wire_StringList *txids);

void wire_sync__method__Wallet(int64_t port_, struct wire_Wallet *that, struct wire_KeyId *key_id);

void wire_next_address__method__Wallet(int64_t port_,
                                       struct wire_Wallet *that,
                                       struct wire_KeyId *key_id);

WireSyncReturn wire_addresses_state__method__Wallet(struct wire_Wallet *that,
                                                    struct wire_KeyId *key_id);

void wire_send_to__method__Wallet(int64_t port_,
                                  struct wire_Wallet *that,
                                  struct wire_KeyId *key_id,
                                  struct wire_uint_8_list *to_address,
                                  uint64_t value,
                                  double feerate);

void wire_broadcast_tx__method__Wallet(int64_t port_,
                                       struct wire_Wallet *that,
                                       struct wire_KeyId *key_id,
                                       struct wire_SignedTx *tx);

WireSyncReturn wire_psbt_to_unsigned_tx__method__Wallet(struct wire_Wallet *that,
                                                        struct wire_Psbt *psbt,
                                                        struct wire_KeyId *key_id);

WireSyncReturn wire_signet__static_method__BitcoinNetwork(void);

WireSyncReturn wire_name__method__BitcoinNetwork(struct wire_BitcoinNetwork *that);

WireSyncReturn wire_is_mainnet__method__BitcoinNetwork(struct wire_BitcoinNetwork *that);

WireSyncReturn wire_descriptor_for_key__method__BitcoinNetwork(struct wire_BitcoinNetwork *that,
                                                               struct wire_KeyId *key_id);

WireSyncReturn wire_validate_amount__method__BitcoinNetwork(struct wire_BitcoinNetwork *that,
                                                            struct wire_uint_8_list *address,
                                                            uint64_t value);

WireSyncReturn wire_validate_destination_address__method__BitcoinNetwork(struct wire_BitcoinNetwork *that,
                                                                         struct wire_uint_8_list *address);

WireSyncReturn wire_default_electrum_server__method__BitcoinNetwork(struct wire_BitcoinNetwork *that);

void wire_set_available_ports__method__FfiSerial(int64_t port_,
                                                 struct wire_FfiSerial *that,
                                                 struct wire_list_port_desc *ports);

void wire_start_thread__method__Coordinator(int64_t port_, struct wire_Coordinator *that);

void wire_update_name_preview__method__Coordinator(int64_t port_,
                                                   struct wire_Coordinator *that,
                                                   struct wire_DeviceId *id,
                                                   struct wire_uint_8_list *name);

void wire_finish_naming__method__Coordinator(int64_t port_,
                                             struct wire_Coordinator *that,
                                             struct wire_DeviceId *id,
                                             struct wire_uint_8_list *name);

void wire_send_cancel__method__Coordinator(int64_t port_,
                                           struct wire_Coordinator *that,
                                           struct wire_DeviceId *id);

void wire_display_backup__method__Coordinator(int64_t port_,
                                              struct wire_Coordinator *that,
                                              struct wire_DeviceId *id,
                                              struct wire_KeyId *key_id);

WireSyncReturn wire_key_state__method__Coordinator(struct wire_Coordinator *that);

void wire_sub_key_events__method__Coordinator(int64_t port_, struct wire_Coordinator *that);

WireSyncReturn wire_get_key__method__Coordinator(struct wire_Coordinator *that,
                                                 struct wire_KeyId *key_id);

WireSyncReturn wire_get_key_name__method__Coordinator(struct wire_Coordinator *that,
                                                      struct wire_KeyId *key_id);

WireSyncReturn wire_keys_for_device__method__Coordinator(struct wire_Coordinator *that,
                                                         struct wire_DeviceId *device_id);

void wire_start_signing__method__Coordinator(int64_t port_,
                                             struct wire_Coordinator *that,
                                             struct wire_KeyId *key_id,
                                             struct wire_list_device_id *devices,
                                             struct wire_uint_8_list *message);

void wire_start_signing_tx__method__Coordinator(int64_t port_,
                                                struct wire_Coordinator *that,
                                                struct wire_KeyId *key_id,
                                                struct wire_UnsignedTx *unsigned_tx,
                                                struct wire_list_device_id *devices);

WireSyncReturn wire_nonces_available__method__Coordinator(struct wire_Coordinator *that,
                                                          struct wire_DeviceId *id);

WireSyncReturn wire_current_nonce__method__Coordinator(struct wire_Coordinator *that,
                                                       struct wire_DeviceId *id);

void wire_generate_new_key__method__Coordinator(int64_t port_,
                                                struct wire_Coordinator *that,
                                                uint16_t threshold,
                                                struct wire_list_device_id *devices,
                                                struct wire_uint_8_list *key_name);

WireSyncReturn wire_persisted_sign_session_description__method__Coordinator(struct wire_Coordinator *that,
                                                                            struct wire_KeyId *key_id);

void wire_try_restore_signing_session__method__Coordinator(int64_t port_,
                                                           struct wire_Coordinator *that,
                                                           struct wire_KeyId *key_id);

void wire_start_firmware_upgrade__method__Coordinator(int64_t port_, struct wire_Coordinator *that);

WireSyncReturn wire_upgrade_firmware_digest__method__Coordinator(struct wire_Coordinator *that);

void wire_cancel_protocol__method__Coordinator(int64_t port_, struct wire_Coordinator *that);

void wire_enter_firmware_upgrade_mode__method__Coordinator(int64_t port_,
                                                           struct wire_Coordinator *that);

WireSyncReturn wire_get_device_name__method__Coordinator(struct wire_Coordinator *that,
                                                         struct wire_DeviceId *id);

void wire_final_keygen_ack__method__Coordinator(int64_t port_, struct wire_Coordinator *that);

void wire_check_share_on_device__method__Coordinator(int64_t port_,
                                                     struct wire_Coordinator *that,
                                                     struct wire_DeviceId *device_id,
                                                     struct wire_KeyId *key_id);

WireSyncReturn wire_effect__method__SignedTx(struct wire_SignedTx *that,
                                             struct wire_KeyId *key_id,
                                             struct wire_BitcoinNetwork *network);

void wire_attach_signatures_to_psbt__method__UnsignedTx(int64_t port_,
                                                        struct wire_UnsignedTx *that,
                                                        struct wire_list_encoded_signature *signatures,
                                                        struct wire_Psbt *psbt);

void wire_complete__method__UnsignedTx(int64_t port_,
                                       struct wire_UnsignedTx *that,
                                       struct wire_list_encoded_signature *signatures);

WireSyncReturn wire_effect__method__UnsignedTx(struct wire_UnsignedTx *that,
                                               struct wire_KeyId *key_id,
                                               struct wire_BitcoinNetwork *network);

WireSyncReturn wire_to_bytes__method__Psbt(struct wire_Psbt *that);

void wire_decode_from_bytes__method__QrReader(int64_t port_,
                                              struct wire_QrReader *that,
                                              struct wire_uint_8_list *bytes);

WireSyncReturn wire_next__method__QrEncoder(struct wire_QrEncoder *that);

void wire_sub_developer_settings__method__Settings(int64_t port_, struct wire_Settings *that);

void wire_sub_electrum_settings__method__Settings(int64_t port_, struct wire_Settings *that);

void wire_sub_wallet_settings__method__Settings(int64_t port_, struct wire_Settings *that);

void wire_load_wallet__method__Settings(int64_t port_,
                                        struct wire_Settings *that,
                                        struct wire_BitcoinNetwork *network);

void wire_set_wallet_network__method__Settings(int64_t port_,
                                               struct wire_Settings *that,
                                               struct wire_KeyId *key_id,
                                               struct wire_BitcoinNetwork *network);

void wire_set_developer_mode__method__Settings(int64_t port_,
                                               struct wire_Settings *that,
                                               bool value);

void wire_check_and_set_electrum_server__method__Settings(int64_t port_,
                                                          struct wire_Settings *that,
                                                          struct wire_BitcoinNetwork *network,
                                                          struct wire_uint_8_list *url);

void wire_subscribe_chain_status__method__Settings(int64_t port_,
                                                   struct wire_Settings *that,
                                                   struct wire_BitcoinNetwork *network);

struct wire_ArcMutexFrostsnapWallet new_ArcMutexFrostsnapWallet(void);

struct wire_ArcMutexRusqliteConnection new_ArcMutexRusqliteConnection(void);

struct wire_ArcMutexVecPortDesc new_ArcMutexVecPortDesc(void);

struct wire_ArcRTransaction new_ArcRTransaction(void);

struct wire_ArcWalletStreams new_ArcWalletStreams(void);

struct wire_BitcoinPsbt new_BitcoinPsbt(void);

struct wire_ChainClient new_ChainClient(void);

struct wire_FfiCoordinator new_FfiCoordinator(void);

struct wire_FfiQrEncoder new_FfiQrEncoder(void);

struct wire_FfiQrReader new_FfiQrReader(void);

struct wire_FrostsnapCoreBitcoinTransactionTransactionTemplate new_FrostsnapCoreBitcoinTransactionTransactionTemplate(void);

struct wire_FrostsnapCoreCoordinatorCoordinatorFrostKey new_FrostsnapCoreCoordinatorCoordinatorFrostKey(void);

struct wire_HashMapRBitcoinNetworkChainClient new_HashMapRBitcoinNetworkChainClient(void);

struct wire_MaybeSinkDeveloperSettings new_MaybeSinkDeveloperSettings(void);

struct wire_MaybeSinkElectrumSettings new_MaybeSinkElectrumSettings(void);

struct wire_MaybeSinkWalletSettings new_MaybeSinkWalletSettings(void);

struct wire_MutexHashMapRBitcoinNetworkWallet new_MutexHashMapRBitcoinNetworkWallet(void);

struct wire_MutexPersistedRSettings new_MutexPersistedRSettings(void);

struct wire_PathBuf new_PathBuf(void);

struct wire_PortBytesToReadSender new_PortBytesToReadSender(void);

struct wire_PortOpenSender new_PortOpenSender(void);

struct wire_PortReadSender new_PortReadSender(void);

struct wire_PortWriteSender new_PortWriteSender(void);

struct wire_RBitcoinNetwork new_RBitcoinNetwork(void);

struct wire_RTransaction new_RTransaction(void);

struct wire_StringList *new_StringList_0(int32_t len);

struct wire_BitcoinNetwork *new_box_autoadd_bitcoin_network_0(void);

struct wire_ConfirmationTime *new_box_autoadd_confirmation_time_0(void);

struct wire_ConnectedDevice *new_box_autoadd_connected_device_0(void);

struct wire_Coordinator *new_box_autoadd_coordinator_0(void);

struct wire_DeviceId *new_box_autoadd_device_id_0(void);

struct wire_DeviceListState *new_box_autoadd_device_list_state_0(void);

struct wire_FfiSerial *new_box_autoadd_ffi_serial_0(void);

struct wire_FrostKey *new_box_autoadd_frost_key_0(void);

struct wire_KeyId *new_box_autoadd_key_id_0(void);

struct wire_PortBytesToRead *new_box_autoadd_port_bytes_to_read_0(void);

struct wire_PortOpen *new_box_autoadd_port_open_0(void);

struct wire_PortRead *new_box_autoadd_port_read_0(void);

struct wire_PortWrite *new_box_autoadd_port_write_0(void);

struct wire_Psbt *new_box_autoadd_psbt_0(void);

struct wire_QrEncoder *new_box_autoadd_qr_encoder_0(void);

struct wire_QrReader *new_box_autoadd_qr_reader_0(void);

struct wire_Settings *new_box_autoadd_settings_0(void);

struct wire_SignedTx *new_box_autoadd_signed_tx_0(void);

struct wire_Transaction *new_box_autoadd_transaction_0(void);

struct wire_UnsignedTx *new_box_autoadd_unsigned_tx_0(void);

struct wire_Wallet *new_box_autoadd_wallet_0(void);

struct wire_list_connected_device *new_list_connected_device_0(int32_t len);

struct wire_list_device_id *new_list_device_id_0(int32_t len);

struct wire_list_encoded_signature *new_list_encoded_signature_0(int32_t len);

struct wire_list_port_desc *new_list_port_desc_0(int32_t len);

struct wire_uint_8_list *new_uint_8_list_0(int32_t len);

void drop_opaque_ArcMutexFrostsnapWallet(const void *ptr);

const void *share_opaque_ArcMutexFrostsnapWallet(const void *ptr);

void drop_opaque_ArcMutexRusqliteConnection(const void *ptr);

const void *share_opaque_ArcMutexRusqliteConnection(const void *ptr);

void drop_opaque_ArcMutexVecPortDesc(const void *ptr);

const void *share_opaque_ArcMutexVecPortDesc(const void *ptr);

void drop_opaque_ArcRTransaction(const void *ptr);

const void *share_opaque_ArcRTransaction(const void *ptr);

void drop_opaque_ArcWalletStreams(const void *ptr);

const void *share_opaque_ArcWalletStreams(const void *ptr);

void drop_opaque_BitcoinPsbt(const void *ptr);

const void *share_opaque_BitcoinPsbt(const void *ptr);

void drop_opaque_ChainClient(const void *ptr);

const void *share_opaque_ChainClient(const void *ptr);

void drop_opaque_FfiCoordinator(const void *ptr);

const void *share_opaque_FfiCoordinator(const void *ptr);

void drop_opaque_FfiQrEncoder(const void *ptr);

const void *share_opaque_FfiQrEncoder(const void *ptr);

void drop_opaque_FfiQrReader(const void *ptr);

const void *share_opaque_FfiQrReader(const void *ptr);

void drop_opaque_FrostsnapCoreBitcoinTransactionTransactionTemplate(const void *ptr);

const void *share_opaque_FrostsnapCoreBitcoinTransactionTransactionTemplate(const void *ptr);

void drop_opaque_FrostsnapCoreCoordinatorCoordinatorFrostKey(const void *ptr);

const void *share_opaque_FrostsnapCoreCoordinatorCoordinatorFrostKey(const void *ptr);

void drop_opaque_HashMapRBitcoinNetworkChainClient(const void *ptr);

const void *share_opaque_HashMapRBitcoinNetworkChainClient(const void *ptr);

void drop_opaque_MaybeSinkDeveloperSettings(const void *ptr);

const void *share_opaque_MaybeSinkDeveloperSettings(const void *ptr);

void drop_opaque_MaybeSinkElectrumSettings(const void *ptr);

const void *share_opaque_MaybeSinkElectrumSettings(const void *ptr);

void drop_opaque_MaybeSinkWalletSettings(const void *ptr);

const void *share_opaque_MaybeSinkWalletSettings(const void *ptr);

void drop_opaque_MutexHashMapRBitcoinNetworkWallet(const void *ptr);

const void *share_opaque_MutexHashMapRBitcoinNetworkWallet(const void *ptr);

void drop_opaque_MutexPersistedRSettings(const void *ptr);

const void *share_opaque_MutexPersistedRSettings(const void *ptr);

void drop_opaque_PathBuf(const void *ptr);

const void *share_opaque_PathBuf(const void *ptr);

void drop_opaque_PortBytesToReadSender(const void *ptr);

const void *share_opaque_PortBytesToReadSender(const void *ptr);

void drop_opaque_PortOpenSender(const void *ptr);

const void *share_opaque_PortOpenSender(const void *ptr);

void drop_opaque_PortReadSender(const void *ptr);

const void *share_opaque_PortReadSender(const void *ptr);

void drop_opaque_PortWriteSender(const void *ptr);

const void *share_opaque_PortWriteSender(const void *ptr);

void drop_opaque_RBitcoinNetwork(const void *ptr);

const void *share_opaque_RBitcoinNetwork(const void *ptr);

void drop_opaque_RTransaction(const void *ptr);

const void *share_opaque_RTransaction(const void *ptr);

void free_WireSyncReturn(WireSyncReturn ptr);

static int64_t dummy_method_to_enforce_bundling(void) {
    int64_t dummy_var = 0;
    dummy_var ^= ((int64_t) (void*) wire_sub_port_events);
    dummy_var ^= ((int64_t) (void*) wire_sub_device_events);
    dummy_var ^= ((int64_t) (void*) wire_log);
    dummy_var ^= ((int64_t) (void*) wire_turn_stderr_logging_on);
    dummy_var ^= ((int64_t) (void*) wire_turn_logcat_logging_on);
    dummy_var ^= ((int64_t) (void*) wire_device_at_index);
    dummy_var ^= ((int64_t) (void*) wire_device_list_state);
    dummy_var ^= ((int64_t) (void*) wire_get_connected_device);
    dummy_var ^= ((int64_t) (void*) wire_load);
    dummy_var ^= ((int64_t) (void*) wire_load_host_handles_serial);
    dummy_var ^= ((int64_t) (void*) wire_echo_key_id);
    dummy_var ^= ((int64_t) (void*) wire_psbt_bytes_to_psbt);
    dummy_var ^= ((int64_t) (void*) wire_new_qr_reader);
    dummy_var ^= ((int64_t) (void*) wire_new_qr_encoder);
    dummy_var ^= ((int64_t) (void*) wire_txid__method__Transaction);
    dummy_var ^= ((int64_t) (void*) wire_ready__method__ConnectedDevice);
    dummy_var ^= ((int64_t) (void*) wire_needs_firmware_upgrade__method__ConnectedDevice);
    dummy_var ^= ((int64_t) (void*) wire_threshold__method__FrostKey);
    dummy_var ^= ((int64_t) (void*) wire_id__method__FrostKey);
    dummy_var ^= ((int64_t) (void*) wire_key_name__method__FrostKey);
    dummy_var ^= ((int64_t) (void*) wire_devices__method__FrostKey);
    dummy_var ^= ((int64_t) (void*) wire_polynomial_identifier__method__FrostKey);
    dummy_var ^= ((int64_t) (void*) wire_satisfy__method__PortOpen);
    dummy_var ^= ((int64_t) (void*) wire_satisfy__method__PortRead);
    dummy_var ^= ((int64_t) (void*) wire_satisfy__method__PortWrite);
    dummy_var ^= ((int64_t) (void*) wire_satisfy__method__PortBytesToRead);
    dummy_var ^= ((int64_t) (void*) wire_get_device__method__DeviceListState);
    dummy_var ^= ((int64_t) (void*) wire_sub_tx_state__method__Wallet);
    dummy_var ^= ((int64_t) (void*) wire_tx_state__method__Wallet);
    dummy_var ^= ((int64_t) (void*) wire_sync_txids__method__Wallet);
    dummy_var ^= ((int64_t) (void*) wire_sync__method__Wallet);
    dummy_var ^= ((int64_t) (void*) wire_next_address__method__Wallet);
    dummy_var ^= ((int64_t) (void*) wire_addresses_state__method__Wallet);
    dummy_var ^= ((int64_t) (void*) wire_send_to__method__Wallet);
    dummy_var ^= ((int64_t) (void*) wire_broadcast_tx__method__Wallet);
    dummy_var ^= ((int64_t) (void*) wire_psbt_to_unsigned_tx__method__Wallet);
    dummy_var ^= ((int64_t) (void*) wire_signet__static_method__BitcoinNetwork);
    dummy_var ^= ((int64_t) (void*) wire_name__method__BitcoinNetwork);
    dummy_var ^= ((int64_t) (void*) wire_is_mainnet__method__BitcoinNetwork);
    dummy_var ^= ((int64_t) (void*) wire_descriptor_for_key__method__BitcoinNetwork);
    dummy_var ^= ((int64_t) (void*) wire_validate_amount__method__BitcoinNetwork);
    dummy_var ^= ((int64_t) (void*) wire_validate_destination_address__method__BitcoinNetwork);
    dummy_var ^= ((int64_t) (void*) wire_default_electrum_server__method__BitcoinNetwork);
    dummy_var ^= ((int64_t) (void*) wire_set_available_ports__method__FfiSerial);
    dummy_var ^= ((int64_t) (void*) wire_start_thread__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_update_name_preview__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_finish_naming__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_send_cancel__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_display_backup__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_key_state__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_sub_key_events__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_get_key__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_get_key_name__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_keys_for_device__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_start_signing__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_start_signing_tx__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_nonces_available__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_current_nonce__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_generate_new_key__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_persisted_sign_session_description__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_try_restore_signing_session__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_start_firmware_upgrade__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_upgrade_firmware_digest__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_cancel_protocol__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_enter_firmware_upgrade_mode__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_get_device_name__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_final_keygen_ack__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_check_share_on_device__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_effect__method__SignedTx);
    dummy_var ^= ((int64_t) (void*) wire_attach_signatures_to_psbt__method__UnsignedTx);
    dummy_var ^= ((int64_t) (void*) wire_complete__method__UnsignedTx);
    dummy_var ^= ((int64_t) (void*) wire_effect__method__UnsignedTx);
    dummy_var ^= ((int64_t) (void*) wire_to_bytes__method__Psbt);
    dummy_var ^= ((int64_t) (void*) wire_decode_from_bytes__method__QrReader);
    dummy_var ^= ((int64_t) (void*) wire_next__method__QrEncoder);
    dummy_var ^= ((int64_t) (void*) wire_sub_developer_settings__method__Settings);
    dummy_var ^= ((int64_t) (void*) wire_sub_electrum_settings__method__Settings);
    dummy_var ^= ((int64_t) (void*) wire_sub_wallet_settings__method__Settings);
    dummy_var ^= ((int64_t) (void*) wire_load_wallet__method__Settings);
    dummy_var ^= ((int64_t) (void*) wire_set_wallet_network__method__Settings);
    dummy_var ^= ((int64_t) (void*) wire_set_developer_mode__method__Settings);
    dummy_var ^= ((int64_t) (void*) wire_check_and_set_electrum_server__method__Settings);
    dummy_var ^= ((int64_t) (void*) wire_subscribe_chain_status__method__Settings);
    dummy_var ^= ((int64_t) (void*) new_ArcMutexFrostsnapWallet);
    dummy_var ^= ((int64_t) (void*) new_ArcMutexRusqliteConnection);
    dummy_var ^= ((int64_t) (void*) new_ArcMutexVecPortDesc);
    dummy_var ^= ((int64_t) (void*) new_ArcRTransaction);
    dummy_var ^= ((int64_t) (void*) new_ArcWalletStreams);
    dummy_var ^= ((int64_t) (void*) new_BitcoinPsbt);
    dummy_var ^= ((int64_t) (void*) new_ChainClient);
    dummy_var ^= ((int64_t) (void*) new_FfiCoordinator);
    dummy_var ^= ((int64_t) (void*) new_FfiQrEncoder);
    dummy_var ^= ((int64_t) (void*) new_FfiQrReader);
    dummy_var ^= ((int64_t) (void*) new_FrostsnapCoreBitcoinTransactionTransactionTemplate);
    dummy_var ^= ((int64_t) (void*) new_FrostsnapCoreCoordinatorCoordinatorFrostKey);
    dummy_var ^= ((int64_t) (void*) new_HashMapRBitcoinNetworkChainClient);
    dummy_var ^= ((int64_t) (void*) new_MaybeSinkDeveloperSettings);
    dummy_var ^= ((int64_t) (void*) new_MaybeSinkElectrumSettings);
    dummy_var ^= ((int64_t) (void*) new_MaybeSinkWalletSettings);
    dummy_var ^= ((int64_t) (void*) new_MutexHashMapRBitcoinNetworkWallet);
    dummy_var ^= ((int64_t) (void*) new_MutexPersistedRSettings);
    dummy_var ^= ((int64_t) (void*) new_PathBuf);
    dummy_var ^= ((int64_t) (void*) new_PortBytesToReadSender);
    dummy_var ^= ((int64_t) (void*) new_PortOpenSender);
    dummy_var ^= ((int64_t) (void*) new_PortReadSender);
    dummy_var ^= ((int64_t) (void*) new_PortWriteSender);
    dummy_var ^= ((int64_t) (void*) new_RBitcoinNetwork);
    dummy_var ^= ((int64_t) (void*) new_RTransaction);
    dummy_var ^= ((int64_t) (void*) new_StringList_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_bitcoin_network_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_confirmation_time_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_connected_device_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_coordinator_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_device_id_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_device_list_state_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_ffi_serial_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_frost_key_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_key_id_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_port_bytes_to_read_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_port_open_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_port_read_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_port_write_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_psbt_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_qr_encoder_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_qr_reader_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_settings_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_signed_tx_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_transaction_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_unsigned_tx_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_wallet_0);
    dummy_var ^= ((int64_t) (void*) new_list_connected_device_0);
    dummy_var ^= ((int64_t) (void*) new_list_device_id_0);
    dummy_var ^= ((int64_t) (void*) new_list_encoded_signature_0);
    dummy_var ^= ((int64_t) (void*) new_list_port_desc_0);
    dummy_var ^= ((int64_t) (void*) new_uint_8_list_0);
    dummy_var ^= ((int64_t) (void*) drop_opaque_ArcMutexFrostsnapWallet);
    dummy_var ^= ((int64_t) (void*) share_opaque_ArcMutexFrostsnapWallet);
    dummy_var ^= ((int64_t) (void*) drop_opaque_ArcMutexRusqliteConnection);
    dummy_var ^= ((int64_t) (void*) share_opaque_ArcMutexRusqliteConnection);
    dummy_var ^= ((int64_t) (void*) drop_opaque_ArcMutexVecPortDesc);
    dummy_var ^= ((int64_t) (void*) share_opaque_ArcMutexVecPortDesc);
    dummy_var ^= ((int64_t) (void*) drop_opaque_ArcRTransaction);
    dummy_var ^= ((int64_t) (void*) share_opaque_ArcRTransaction);
    dummy_var ^= ((int64_t) (void*) drop_opaque_ArcWalletStreams);
    dummy_var ^= ((int64_t) (void*) share_opaque_ArcWalletStreams);
    dummy_var ^= ((int64_t) (void*) drop_opaque_BitcoinPsbt);
    dummy_var ^= ((int64_t) (void*) share_opaque_BitcoinPsbt);
    dummy_var ^= ((int64_t) (void*) drop_opaque_ChainClient);
    dummy_var ^= ((int64_t) (void*) share_opaque_ChainClient);
    dummy_var ^= ((int64_t) (void*) drop_opaque_FfiCoordinator);
    dummy_var ^= ((int64_t) (void*) share_opaque_FfiCoordinator);
    dummy_var ^= ((int64_t) (void*) drop_opaque_FfiQrEncoder);
    dummy_var ^= ((int64_t) (void*) share_opaque_FfiQrEncoder);
    dummy_var ^= ((int64_t) (void*) drop_opaque_FfiQrReader);
    dummy_var ^= ((int64_t) (void*) share_opaque_FfiQrReader);
    dummy_var ^= ((int64_t) (void*) drop_opaque_FrostsnapCoreBitcoinTransactionTransactionTemplate);
    dummy_var ^= ((int64_t) (void*) share_opaque_FrostsnapCoreBitcoinTransactionTransactionTemplate);
    dummy_var ^= ((int64_t) (void*) drop_opaque_FrostsnapCoreCoordinatorCoordinatorFrostKey);
    dummy_var ^= ((int64_t) (void*) share_opaque_FrostsnapCoreCoordinatorCoordinatorFrostKey);
    dummy_var ^= ((int64_t) (void*) drop_opaque_HashMapRBitcoinNetworkChainClient);
    dummy_var ^= ((int64_t) (void*) share_opaque_HashMapRBitcoinNetworkChainClient);
    dummy_var ^= ((int64_t) (void*) drop_opaque_MaybeSinkDeveloperSettings);
    dummy_var ^= ((int64_t) (void*) share_opaque_MaybeSinkDeveloperSettings);
    dummy_var ^= ((int64_t) (void*) drop_opaque_MaybeSinkElectrumSettings);
    dummy_var ^= ((int64_t) (void*) share_opaque_MaybeSinkElectrumSettings);
    dummy_var ^= ((int64_t) (void*) drop_opaque_MaybeSinkWalletSettings);
    dummy_var ^= ((int64_t) (void*) share_opaque_MaybeSinkWalletSettings);
    dummy_var ^= ((int64_t) (void*) drop_opaque_MutexHashMapRBitcoinNetworkWallet);
    dummy_var ^= ((int64_t) (void*) share_opaque_MutexHashMapRBitcoinNetworkWallet);
    dummy_var ^= ((int64_t) (void*) drop_opaque_MutexPersistedRSettings);
    dummy_var ^= ((int64_t) (void*) share_opaque_MutexPersistedRSettings);
    dummy_var ^= ((int64_t) (void*) drop_opaque_PathBuf);
    dummy_var ^= ((int64_t) (void*) share_opaque_PathBuf);
    dummy_var ^= ((int64_t) (void*) drop_opaque_PortBytesToReadSender);
    dummy_var ^= ((int64_t) (void*) share_opaque_PortBytesToReadSender);
    dummy_var ^= ((int64_t) (void*) drop_opaque_PortOpenSender);
    dummy_var ^= ((int64_t) (void*) share_opaque_PortOpenSender);
    dummy_var ^= ((int64_t) (void*) drop_opaque_PortReadSender);
    dummy_var ^= ((int64_t) (void*) share_opaque_PortReadSender);
    dummy_var ^= ((int64_t) (void*) drop_opaque_PortWriteSender);
    dummy_var ^= ((int64_t) (void*) share_opaque_PortWriteSender);
    dummy_var ^= ((int64_t) (void*) drop_opaque_RBitcoinNetwork);
    dummy_var ^= ((int64_t) (void*) share_opaque_RBitcoinNetwork);
    dummy_var ^= ((int64_t) (void*) drop_opaque_RTransaction);
    dummy_var ^= ((int64_t) (void*) share_opaque_RTransaction);
    dummy_var ^= ((int64_t) (void*) free_WireSyncReturn);
    dummy_var ^= ((int64_t) (void*) store_dart_post_cobject);
    dummy_var ^= ((int64_t) (void*) get_dart_object);
    dummy_var ^= ((int64_t) (void*) drop_dart_object);
    dummy_var ^= ((int64_t) (void*) new_dart_opaque);
    return dummy_var;
}
