#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>
typedef struct _Dart_Handle* Dart_Handle;

typedef struct DartCObject DartCObject;

typedef int64_t DartPort;

typedef bool (*DartPostCObjectFnType)(DartPort port_id, void *message);

typedef struct wire_FfiCoordinator {
  const void *ptr;
} wire_FfiCoordinator;

typedef struct wire_uint_8_list {
  uint8_t *ptr;
  int32_t len;
} wire_uint_8_list;

typedef struct wire_PortDesc {
  struct wire_uint_8_list *id;
  uint16_t vid;
  uint16_t pid;
} wire_PortDesc;

typedef struct wire_list_port_desc {
  struct wire_PortDesc *ptr;
  int32_t len;
} wire_list_port_desc;

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

typedef struct DartCObject *WireSyncReturn;

void store_dart_post_cobject(DartPostCObjectFnType ptr);

Dart_Handle get_dart_object(uintptr_t ptr);

void drop_dart_object(uintptr_t ptr);

uintptr_t new_dart_opaque(Dart_Handle handle);

intptr_t init_frb_dart_api_dl(void *obj);

void wire_sub_port_events(int64_t port_);

void wire_sub_device_events(int64_t port_);

void wire_new_ffi_coordinator(int64_t port_, bool host_handles_serial);

void wire_turn_stderr_logging_on(int64_t port_, int32_t level);

void wire_turn_logcat_logging_on(int64_t port_, int32_t _level);

void wire_announce_available_ports(int64_t port_,
                                   struct wire_FfiCoordinator coordinator,
                                   struct wire_list_port_desc *ports);

void wire_set_device_label(int64_t port_,
                           struct wire_FfiCoordinator coordinator,
                           struct wire_uint_8_list *device_id,
                           struct wire_uint_8_list *label);

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

struct wire_FfiCoordinator new_FfiCoordinator(void);

struct wire_PortBytesToReadSender new_PortBytesToReadSender(void);

struct wire_PortOpenSender new_PortOpenSender(void);

struct wire_PortReadSender new_PortReadSender(void);

struct wire_PortWriteSender new_PortWriteSender(void);

struct wire_PortBytesToRead *new_box_autoadd_port_bytes_to_read_0(void);

struct wire_PortOpen *new_box_autoadd_port_open_0(void);

struct wire_PortRead *new_box_autoadd_port_read_0(void);

struct wire_PortWrite *new_box_autoadd_port_write_0(void);

struct wire_list_port_desc *new_list_port_desc_0(int32_t len);

struct wire_uint_8_list *new_uint_8_list_0(int32_t len);

void drop_opaque_FfiCoordinator(const void *ptr);

const void *share_opaque_FfiCoordinator(const void *ptr);

void drop_opaque_PortBytesToReadSender(const void *ptr);

const void *share_opaque_PortBytesToReadSender(const void *ptr);

void drop_opaque_PortOpenSender(const void *ptr);

const void *share_opaque_PortOpenSender(const void *ptr);

void drop_opaque_PortReadSender(const void *ptr);

const void *share_opaque_PortReadSender(const void *ptr);

void drop_opaque_PortWriteSender(const void *ptr);

const void *share_opaque_PortWriteSender(const void *ptr);

void free_WireSyncReturn(WireSyncReturn ptr);

static int64_t dummy_method_to_enforce_bundling(void) {
    int64_t dummy_var = 0;
    dummy_var ^= ((int64_t) (void*) wire_sub_port_events);
    dummy_var ^= ((int64_t) (void*) wire_sub_device_events);
    dummy_var ^= ((int64_t) (void*) wire_new_ffi_coordinator);
    dummy_var ^= ((int64_t) (void*) wire_turn_stderr_logging_on);
    dummy_var ^= ((int64_t) (void*) wire_turn_logcat_logging_on);
    dummy_var ^= ((int64_t) (void*) wire_announce_available_ports);
    dummy_var ^= ((int64_t) (void*) wire_set_device_label);
    dummy_var ^= ((int64_t) (void*) wire_satisfy__method__PortOpen);
    dummy_var ^= ((int64_t) (void*) wire_satisfy__method__PortRead);
    dummy_var ^= ((int64_t) (void*) wire_satisfy__method__PortWrite);
    dummy_var ^= ((int64_t) (void*) wire_satisfy__method__PortBytesToRead);
    dummy_var ^= ((int64_t) (void*) new_FfiCoordinator);
    dummy_var ^= ((int64_t) (void*) new_PortBytesToReadSender);
    dummy_var ^= ((int64_t) (void*) new_PortOpenSender);
    dummy_var ^= ((int64_t) (void*) new_PortReadSender);
    dummy_var ^= ((int64_t) (void*) new_PortWriteSender);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_port_bytes_to_read_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_port_open_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_port_read_0);
    dummy_var ^= ((int64_t) (void*) new_box_autoadd_port_write_0);
    dummy_var ^= ((int64_t) (void*) new_list_port_desc_0);
    dummy_var ^= ((int64_t) (void*) new_uint_8_list_0);
    dummy_var ^= ((int64_t) (void*) drop_opaque_FfiCoordinator);
    dummy_var ^= ((int64_t) (void*) share_opaque_FfiCoordinator);
    dummy_var ^= ((int64_t) (void*) drop_opaque_PortBytesToReadSender);
    dummy_var ^= ((int64_t) (void*) share_opaque_PortBytesToReadSender);
    dummy_var ^= ((int64_t) (void*) drop_opaque_PortOpenSender);
    dummy_var ^= ((int64_t) (void*) share_opaque_PortOpenSender);
    dummy_var ^= ((int64_t) (void*) drop_opaque_PortReadSender);
    dummy_var ^= ((int64_t) (void*) share_opaque_PortReadSender);
    dummy_var ^= ((int64_t) (void*) drop_opaque_PortWriteSender);
    dummy_var ^= ((int64_t) (void*) share_opaque_PortWriteSender);
    dummy_var ^= ((int64_t) (void*) free_WireSyncReturn);
    dummy_var ^= ((int64_t) (void*) store_dart_post_cobject);
    dummy_var ^= ((int64_t) (void*) get_dart_object);
    dummy_var ^= ((int64_t) (void*) drop_dart_object);
    dummy_var ^= ((int64_t) (void*) new_dart_opaque);
    return dummy_var;
}
