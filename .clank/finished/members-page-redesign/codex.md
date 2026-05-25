APPROVE Ready after relay-accepted init handling

No blocking issues found in this revision. The previous creation durability issue is addressed by only setting `published_init_event` when `send_prepared_message` reports at least one relay success, so the Dart `ChannelState` wait now corresponds to a relay-accepted creation event. The Dart wait also has a bounded timeout, so init publish failures no longer hang indefinitely.

Checks run:
- `cargo check -p frostsnap_nostr`
- `cargo check -p rust_lib_frostsnapp` (existing `request_id` dead-code warning)
- `flutter analyze frostsnapp/lib/org_keygen_page.dart frostsnapp/lib/wallet.dart frostsnapp/lib/nostr_chat/group_info_page.dart frostsnapp/lib/nostr_chat/member_detail_sheet.dart frostsnapp/lib/nostr_chat/chat_page.dart`
