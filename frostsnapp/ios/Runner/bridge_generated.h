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

typedef struct wire_RTransaction {
  const void *ptr;
} wire_RTransaction;

typedef struct wire_ConfirmationTime {
  uint32_t height;
  uint64_t time;
} wire_ConfirmationTime;

typedef struct wire_Transaction {
  int64_t net_value;
  struct wire_RTransaction inner;
  struct wire_ConfirmationTime *confirmation_time;
} wire_Transaction;

typedef struct wire_FrostsnapCoreCoordinatorFrostKey {
  const void *ptr;
} wire_FrostsnapCoreCoordinatorFrostKey;

typedef struct wire_FrostKey {
  struct wire_FrostsnapCoreCoordinatorFrostKey field0;
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

typedef struct wire_SigningState {
  struct wire_list_device_id *got_shares;
  struct wire_list_device_id *needed_from;
  struct wire_list_encoded_signature *finished_signatures;
} wire_SigningState;

typedef struct wire_Device {
  struct wire_uint_8_list *name;
  struct wire_DeviceId id;
} wire_Device;

typedef struct wire_list_device {
  struct wire_Device *ptr;
  int32_t len;
} wire_list_device;

typedef struct wire_DeviceListState {
  struct wire_list_device *devices;
  uintptr_t state_id;
} wire_DeviceListState;

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

typedef struct wire_FrostsnapCoreMessageTransactionSignTask {
  const void *ptr;
} wire_FrostsnapCoreMessageTransactionSignTask;

typedef struct wire_UnsignedTx {
  struct wire_FrostsnapCoreMessageTransactionSignTask task;
} wire_UnsignedTx;

typedef struct wire_FrostsnapCoreNostrUnsignedEvent {
  const void *ptr;
} wire_FrostsnapCoreNostrUnsignedEvent;

typedef struct wire_UnsignedNostrEvent {
  struct wire_FrostsnapCoreNostrUnsignedEvent unsigned_event;
} wire_UnsignedNostrEvent;

typedef struct wire_MutexCrateWalletWallet {
  const void *ptr;
} wire_MutexCrateWalletWallet;

typedef struct wire_MutexBTreeMapKeyIdStreamSinkTxState {
  const void *ptr;
} wire_MutexBTreeMapKeyIdStreamSinkTxState;

typedef struct wire_ChainSync {
  const void *ptr;
} wire_ChainSync;

typedef struct wire_Wallet {
  struct wire_MutexCrateWalletWallet inner;
  struct wire_MutexBTreeMapKeyIdStreamSinkTxState wallet_streams;
  struct wire_ChainSync chain_sync;
} wire_Wallet;

typedef struct wire_StringList {
  struct wire_uint_8_list **ptr;
  int32_t len;
} wire_StringList;

typedef struct wire_SignedTx {
  struct wire_RTransaction inner;
} wire_SignedTx;

typedef struct wire_FrostsnapCoreNostrEvent {
  const void *ptr;
} wire_FrostsnapCoreNostrEvent;

typedef struct wire_SignedNostrEvent {
  struct wire_FrostsnapCoreNostrEvent signed_event;
} wire_SignedNostrEvent;

void store_dart_post_cobject(DartPostCObjectFnType ptr);

Dart_Handle get_dart_object(uintptr_t ptr);

void drop_dart_object(uintptr_t ptr);

uintptr_t new_dart_opaque(Dart_Handle handle);

intptr_t init_frb_dart_api_dl(void *obj);

void wire_sub_port_events(int64_t port_);

void wire_sub_device_events(int64_t port_);

void wire_turn_stderr_logging_on(int64_t port_, int32_t level);

void wire_turn_logcat_logging_on(int64_t port_, int32_t _level);

WireSyncReturn wire_device_at_index(uintptr_t index);

WireSyncReturn wire_device_list_state(void);

WireSyncReturn wire_get_device(struct wire_DeviceId *id);

void wire_load(int64_t port_, struct wire_uint_8_list *db_file);

void wire_load_host_handles_serial(int64_t port_, struct wire_uint_8_list *db_file);

void wire_echo_key_id(int64_t port_, struct wire_KeyId *key_id);

WireSyncReturn wire_txid__method__Transaction(struct wire_Transaction *that);

WireSyncReturn wire_threshold__method__FrostKey(struct wire_FrostKey *that);

WireSyncReturn wire_id__method__FrostKey(struct wire_FrostKey *that);

WireSyncReturn wire_name__method__FrostKey(struct wire_FrostKey *that);

WireSyncReturn wire_devices__method__FrostKey(struct wire_FrostKey *that);

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

WireSyncReturn wire_is_finished__method__SigningState(struct wire_SigningState *that);

WireSyncReturn wire_named_devices__method__DeviceListState(struct wire_DeviceListState *that);

WireSyncReturn wire_get_device__method__DeviceListState(struct wire_DeviceListState *that,
                                                        struct wire_DeviceId *id);

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

void wire_cancel_all__method__Coordinator(int64_t port_, struct wire_Coordinator *that);

void wire_display_backup__method__Coordinator(int64_t port_,
                                              struct wire_Coordinator *that,
                                              struct wire_DeviceId *id,
                                              struct wire_KeyId *key_id);

WireSyncReturn wire_key_state__method__Coordinator(struct wire_Coordinator *that);

void wire_sub_key_events__method__Coordinator(int64_t port_, struct wire_Coordinator *that);

WireSyncReturn wire_get_key__method__Coordinator(struct wire_Coordinator *that,
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

void wire_create_nostr_event__method__Coordinator(int64_t port_,
                                                  struct wire_Coordinator *that,
                                                  struct wire_KeyId *key_id,
                                                  struct wire_uint_8_list *event_content);

void wire_start_signing_nostr__method__Coordinator(int64_t port_,
                                                   struct wire_Coordinator *that,
                                                   struct wire_KeyId *key_id,
                                                   struct wire_UnsignedNostrEvent *unsigned_event,
                                                   struct wire_list_device_id *devices);

WireSyncReturn wire_get_npub__method__Coordinator(struct wire_Coordinator *that,
                                                  struct wire_KeyId *key_id);

WireSyncReturn wire_get_signing_state__method__Coordinator(struct wire_Coordinator *that);

WireSyncReturn wire_nonces_available__method__Coordinator(struct wire_Coordinator *that,
                                                          struct wire_DeviceId *id);

void wire_generate_new_key__method__Coordinator(int64_t port_,
                                                struct wire_Coordinator *that,
                                                uintptr_t threshold,
                                                struct wire_list_device_id *devices);

WireSyncReturn wire_can_restore_signing_session__method__Coordinator(struct wire_Coordinator *that,
                                                                     struct wire_KeyId *key_id);

WireSyncReturn wire_persisted_sign_session_description__method__Coordinator(struct wire_Coordinator *that,
                                                                            struct wire_KeyId *key_id);

void wire_try_restore_signing_session__method__Coordinator(int64_t port_,
                                                           struct wire_Coordinator *that,
                                                           struct wire_KeyId *key_id);

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

WireSyncReturn wire_validate_destination_address__method__Wallet(struct wire_Wallet *that,
                                                                 struct wire_uint_8_list *address);

WireSyncReturn wire_validate_amount__method__Wallet(struct wire_Wallet *that,
                                                    struct wire_uint_8_list *address,
                                                    uint64_t value);

void wire_send_to__method__Wallet(int64_t port_,
                                  struct wire_Wallet *that,
                                  struct wire_KeyId *key_id,
                                  struct wire_uint_8_list *to_address,
                                  uint64_t value,
                                  double feerate);

WireSyncReturn wire_complete_unsigned_tx__method__Wallet(struct wire_Wallet *that,
                                                         struct wire_UnsignedTx *unsigned_tx,
                                                         struct wire_list_encoded_signature *signatures);

void wire_broadcast_tx__method__Wallet(int64_t port_,
                                       struct wire_Wallet *that,
                                       struct wire_KeyId *key_id,
                                       struct wire_SignedTx *tx);

WireSyncReturn wire_effect_of_tx__method__Wallet(struct wire_Wallet *that,
                                                 struct wire_KeyId *key_id,
                                                 struct wire_RTransaction tx);

WireSyncReturn wire_tx__method__SignedTx(struct wire_SignedTx *that);

WireSyncReturn wire_tx__method__UnsignedTx(struct wire_UnsignedTx *that);

WireSyncReturn wire_note_id__method__UnsignedNostrEvent(struct wire_UnsignedNostrEvent *that);

WireSyncReturn wire_add_signature__method__UnsignedNostrEvent(struct wire_UnsignedNostrEvent *that,
                                                              struct wire_EncodedSignature *signature);

void wire_broadcast__method__SignedNostrEvent(int64_t port_, struct wire_SignedNostrEvent *that);

struct wire_ArcMutexVecPortDesc new_ArcMutexVecPortDesc(void);

struct wire_ChainSync new_ChainSync(void);

struct wire_FfiCoordinator new_FfiCoordinator(void);

struct wire_FrostsnapCoreCoordinatorFrostKey new_FrostsnapCoreCoordinatorFrostKey(void);

struct wire_FrostsnapCoreMessageTransactionSignTask new_FrostsnapCoreMessageTransactionSignTask(void);

struct wire_FrostsnapCoreNostrEvent new_FrostsnapCoreNostrEvent(void);

struct wire_FrostsnapCoreNostrUnsignedEvent new_FrostsnapCoreNostrUnsignedEvent(void);

struct wire_MutexBTreeMapKeyIdStreamSinkTxState new_MutexBTreeMapKeyIdStreamSinkTxState(void);

struct wire_MutexCrateWalletWallet new_MutexCrateWalletWallet(void);

struct wire_PortBytesToReadSender new_PortBytesToReadSender(void);

struct wire_PortOpenSender new_PortOpenSender(void);

struct wire_PortReadSender new_PortReadSender(void);

struct wire_PortWriteSender new_PortWriteSender(void);

struct wire_RTransaction new_RTransaction(void);

struct wire_StringList *new_StringList_0(int32_t len);

struct wire_ConfirmationTime *new_box_autoadd_confirmation_time_0(void);

struct wire_Coordinator *new_box_autoadd_coordinator_0(void);

struct wire_DeviceId *new_box_autoadd_device_id_0(void);

struct wire_DeviceListState *new_box_autoadd_device_list_state_0(void);

struct wire_EncodedSignature *new_box_autoadd_encoded_signature_0(void);

struct wire_FfiSerial *new_box_autoadd_ffi_serial_0(void);

struct wire_FrostKey *new_box_autoadd_frost_key_0(void);

struct wire_KeyId *new_box_autoadd_key_id_0(void);

struct wire_PortBytesToRead *new_box_autoadd_port_bytes_to_read_0(void);

struct wire_PortOpen *new_box_autoadd_port_open_0(void);

struct wire_PortRead *new_box_autoadd_port_read_0(void);

struct wire_PortWrite *new_box_autoadd_port_write_0(void);

struct wire_SignedNostrEvent *new_box_autoadd_signed_nostr_event_0(void);

struct wire_SignedTx *new_box_autoadd_signed_tx_0(void);

struct wire_SigningState *new_box_autoadd_signing_state_0(void);

struct wire_Transaction *new_box_autoadd_transaction_0(void);

struct wire_UnsignedNostrEvent *new_box_autoadd_unsigned_nostr_event_0(void);

struct wire_UnsignedTx *new_box_autoadd_unsigned_tx_0(void);

struct wire_Wallet *new_box_autoadd_wallet_0(void);

struct wire_list_device *new_list_device_0(int32_t len);

struct wire_list_device_id *new_list_device_id_0(int32_t len);

struct wire_list_encoded_signature *new_list_encoded_signature_0(int32_t len);

struct wire_list_port_desc *new_list_port_desc_0(int32_t len);

struct wire_uint_8_list *new_uint_8_list_0(int32_t len);

void drop_opaque_ArcMutexVecPortDesc(const void *ptr);

const void *share_opaque_ArcMutexVecPortDesc(const void *ptr);

void drop_opaque_ChainSync(const void *ptr);

const void *share_opaque_ChainSync(const void *ptr);

void drop_opaque_FfiCoordinator(const void *ptr);

const void *share_opaque_FfiCoordinator(const void *ptr);

void drop_opaque_FrostsnapCoreCoordinatorFrostKey(const void *ptr);

const void *share_opaque_FrostsnapCoreCoordinatorFrostKey(const void *ptr);

void drop_opaque_FrostsnapCoreMessageTransactionSignTask(const void *ptr);

const void *share_opaque_FrostsnapCoreMessageTransactionSignTask(const void *ptr);

void drop_opaque_FrostsnapCoreNostrEvent(const void *ptr);

const void *share_opaque_FrostsnapCoreNostrEvent(const void *ptr);

void drop_opaque_FrostsnapCoreNostrUnsignedEvent(const void *ptr);

const void *share_opaque_FrostsnapCoreNostrUnsignedEvent(const void *ptr);

void drop_opaque_MutexBTreeMapKeyIdStreamSinkTxState(const void *ptr);

const void *share_opaque_MutexBTreeMapKeyIdStreamSinkTxState(const void *ptr);

void drop_opaque_MutexCrateWalletWallet(const void *ptr);

const void *share_opaque_MutexCrateWalletWallet(const void *ptr);

void drop_opaque_PortBytesToReadSender(const void *ptr);

const void *share_opaque_PortBytesToReadSender(const void *ptr);

void drop_opaque_PortOpenSender(const void *ptr);

const void *share_opaque_PortOpenSender(const void *ptr);

void drop_opaque_PortReadSender(const void *ptr);

const void *share_opaque_PortReadSender(const void *ptr);

void drop_opaque_PortWriteSender(const void *ptr);

const void *share_opaque_PortWriteSender(const void *ptr);

void drop_opaque_RTransaction(const void *ptr);

const void *share_opaque_RTransaction(const void *ptr);

void free_WireSyncReturn(WireSyncReturn ptr);

static int64_t dummy_method_to_enforce_bundling(void) {
    int64_t dummy_var = 0;
    dummy_var ^= ((int64_t) (void*) wire_sub_port_events);
    dummy_var ^= ((int64_t) (void*) wire_sub_device_events);
    dummy_var ^= ((int64_t) (void*) wire_turn_stderr_logging_on);
    dummy_var ^= ((int64_t) (void*) wire_turn_logcat_logging_on);
    dummy_var ^= ((int64_t) (void*) wire_device_at_index);
    dummy_var ^= ((int64_t) (void*) wire_device_list_state);
    dummy_var ^= ((int64_t) (void*) wire_get_device);
    dummy_var ^= ((int64_t) (void*) wire_load);
    dummy_var ^= ((int64_t) (void*) wire_load_host_handles_serial);
    dummy_var ^= ((int64_t) (void*) wire_echo_key_id);
    dummy_var ^= ((int64_t) (void*) wire_txid__method__Transaction);
    dummy_var ^= ((int64_t) (void*) wire_threshold__method__FrostKey);
    dummy_var ^= ((int64_t) (void*) wire_id__method__FrostKey);
    dummy_var ^= ((int64_t) (void*) wire_name__method__FrostKey);
    dummy_var ^= ((int64_t) (void*) wire_devices__method__FrostKey);
    dummy_var ^= ((int64_t) (void*) wire_satisfy__method__PortOpen);
    dummy_var ^= ((int64_t) (void*) wire_satisfy__method__PortRead);
    dummy_var ^= ((int64_t) (void*) wire_satisfy__method__PortWrite);
    dummy_var ^= ((int64_t) (void*) wire_satisfy__method__PortBytesToRead);
    dummy_var ^= ((int64_t) (void*) wire_is_finished__method__SigningState);
    dummy_var ^= ((int64_t) (void*) wire_named_devices__method__DeviceListState);
    dummy_var ^= ((int64_t) (void*) wire_get_device__method__DeviceListState);
    dummy_var ^= ((int64_t) (void*) wire_set_available_ports__method__FfiSerial);
    dummy_var ^= ((int64_t) (void*) wire_start_thread__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_update_name_preview__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_finish_naming__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_send_cancel__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_cancel_all__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_display_backup__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_key_state__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_sub_key_events__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_get_key__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_keys_for_device__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_start_signing__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_start_signing_tx__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_create_nostr_event__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_start_signing_nostr__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_get_npub__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_get_signing_state__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_nonces_available__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_generate_new_key__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_can_restore_signing_session__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_persisted_sign_session_description__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_try_restore_signing_session__method__Coordinator);
    dummy_var ^= ((int64_t) (void*) wire_sub_tx_state__method__Wallet);
    dummy_var ^= ((int64_t) (void*) wire_tx_state__method__Wallet);
    dummy_var ^= ((int64_t) (void*) wire_sync_txids__method__Wallet);
    dummy_var ^= ((int64_t) (void*) wire_sync__method__Wallet);
    dummy_var ^= ((int64_t) (void*) wire_next_address__method__Wallet);
    dummy_var ^= ((int64_t) (void*) wire_addresses_state__method__Wallet);
    dummy_var ^= ((int64_t) (void*) wire_validate_destination_address__method__Wallet);
    dummy_var ^= ((int64_t) (void*) wire_validate_amount__method__Wallet);
    dummy_var ^= ((int64_t) (void*) wire_send_to__method__Wallet);
    dummy_var ^= ((int64_t) (void*) wire_complete_unsigned_tx__method__Wallet);
    dummy_var ^= ((int64_t) (void*) wire_broadcast_tx__method__Wallet);
    dummy_var ^= ((int64_t) (void*) wire_effect_of_tx__method__Wallet);
    dummy_var ^= ((int64_t) (void*) wire_tx__method__SignedTx);
    dummy_var ^= ((int64_t) (void*) wire_tx__method__UnsignedTx);
    dummy_var ^= ((int64_t) (void*) wire_note_id__method__UnsignedNostrEvent);
    dummy_var ^= ((int64_t) (void*) wire_add_signature__method__UnsignedNostrEvent);
    dummy_var ^= ((int64_t) (void*) wire_broadcast__method__SignedNostrEvent);
    dummy_var ^= ((int64_t) (void*) new_ArcMutexVecPortDesc);
    dummy_var ^= ((int64_t) (void*) new_ChainSync);
    dummy_var ^= ((int64_t) (void*) new_FfiCoordinator);
    dummy_var ^= ((int64_t) (void*) new_FrostsnapCoreCoordinatorFrostKey);
    dummy_var ^= ((int64_t) (void*) new_FrostsnapCoreMessageTransactionSignTask);
    dummy_var ^= ((int64_t) (void*) new_FrostsnapCoreNostrEvent);
    dummy_var ^= ((int64_t) (void*) new_FrostsnapCoreNostrUnsignedEvent);
    dummy_var ^= ((int64_t) (void*) new_MutexBTreeMapKeyIdStreamSinkTxState);
    dummy_var ^= ((int64_t) (void*) new_MutexCrateWalletWallet);
    dummy_var ^= ((int64_t) (void*) new_PortBytesToReadSender);
    dummy_var ^= ((int64_t) (void*) new_PortOpenSender);
    dummy_var ^= ((int64_t) (void*) new_PortReadSender);
    dummy_var ^= ((int64_t) (void*) new_PortWriteSender);
    dummy_var ^= ((int64_t) (void*) new_RTransaction);
    dummy_var ^= ((int64_t) (void*) new_StringList_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_confirmation_time_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_coordinator_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_device_id_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_device_list_state_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_encoded_signature_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_ffi_serial_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_frost_key_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_key_id_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_port_bytes_to_read_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_port_open_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_port_read_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_port_write_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_signed_nostr_event_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_signed_tx_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_signing_state_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_transaction_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_unsigned_nostr_event_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_unsigned_tx_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_wallet_0);
    dummy_var ^= ((int64_t) (void*) new_list_device_0);
    dummy_var ^= ((int64_t) (void*) new_list_device_id_0);
    dummy_var ^= ((int64_t) (void*) new_list_encoded_signature_0);
    dummy_var ^= ((int64_t) (void*) new_list_port_desc_0);
    dummy_var ^= ((int64_t) (void*) new_uint_8_list_0);
    dummy_var ^= ((int64_t) (void*) drop_opaque_ArcMutexVecPortDesc);
    dummy_var ^= ((int64_t) (void*) share_opaque_ArcMutexVecPortDesc);
    dummy_var ^= ((int64_t) (void*) drop_opaque_ChainSync);
    dummy_var ^= ((int64_t) (void*) share_opaque_ChainSync);
    dummy_var ^= ((int64_t) (void*) drop_opaque_FfiCoordinator);
    dummy_var ^= ((int64_t) (void*) share_opaque_FfiCoordinator);
    dummy_var ^= ((int64_t) (void*) drop_opaque_FrostsnapCoreCoordinatorFrostKey);
    dummy_var ^= ((int64_t) (void*) share_opaque_FrostsnapCoreCoordinatorFrostKey);
    dummy_var ^= ((int64_t) (void*) drop_opaque_FrostsnapCoreMessageTransactionSignTask);
    dummy_var ^= ((int64_t) (void*) share_opaque_FrostsnapCoreMessageTransactionSignTask);
    dummy_var ^= ((int64_t) (void*) drop_opaque_FrostsnapCoreNostrEvent);
    dummy_var ^= ((int64_t) (void*) share_opaque_FrostsnapCoreNostrEvent);
    dummy_var ^= ((int64_t) (void*) drop_opaque_FrostsnapCoreNostrUnsignedEvent);
    dummy_var ^= ((int64_t) (void*) share_opaque_FrostsnapCoreNostrUnsignedEvent);
    dummy_var ^= ((int64_t) (void*) drop_opaque_MutexBTreeMapKeyIdStreamSinkTxState);
    dummy_var ^= ((int64_t) (void*) share_opaque_MutexBTreeMapKeyIdStreamSinkTxState);
    dummy_var ^= ((int64_t) (void*) drop_opaque_MutexCrateWalletWallet);
    dummy_var ^= ((int64_t) (void*) share_opaque_MutexCrateWalletWallet);
    dummy_var ^= ((int64_t) (void*) drop_opaque_PortBytesToReadSender);
    dummy_var ^= ((int64_t) (void*) share_opaque_PortBytesToReadSender);
    dummy_var ^= ((int64_t) (void*) drop_opaque_PortOpenSender);
    dummy_var ^= ((int64_t) (void*) share_opaque_PortOpenSender);
    dummy_var ^= ((int64_t) (void*) drop_opaque_PortReadSender);
    dummy_var ^= ((int64_t) (void*) share_opaque_PortReadSender);
    dummy_var ^= ((int64_t) (void*) drop_opaque_PortWriteSender);
    dummy_var ^= ((int64_t) (void*) share_opaque_PortWriteSender);
    dummy_var ^= ((int64_t) (void*) drop_opaque_RTransaction);
    dummy_var ^= ((int64_t) (void*) share_opaque_RTransaction);
    dummy_var ^= ((int64_t) (void*) free_WireSyncReturn);
    dummy_var ^= ((int64_t) (void*) store_dart_post_cobject);
    dummy_var ^= ((int64_t) (void*) get_dart_object);
    dummy_var ^= ((int64_t) (void*) drop_dart_object);
    dummy_var ^= ((int64_t) (void*) new_dart_opaque);
    return dummy_var;
}
