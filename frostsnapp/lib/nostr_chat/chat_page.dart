import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnap/nostr_chat/group_info_page.dart';
import 'package:frostsnap/nostr_chat/nostr_profile.dart';
import 'package:frostsnap/nostr_chat/nostr_state.dart';
import 'package:frostsnap/nostr_chat/signing_card.dart';
import 'package:frostsnap/device_action.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/secure_key_provider.dart';
import 'package:frostsnap/sign_message.dart';
import 'package:frostsnap/stream_ext.dart';
import 'package:frostsnap/src/rust/api/nostr.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/theme.dart';

enum MessageStatus { pending, sent, failed }

class ChatMessage {
  final NostrEventId messageId;
  final PublicKey author;
  final String content;
  DateTime timestamp;
  final bool isMe;
  final NostrEventId? replyTo;
  MessageStatus status;
  String? failureReason;

  ChatMessage({
    required this.messageId,
    required this.author,
    required this.content,
    required this.timestamp,
    required this.isMe,
    this.replyTo,
    this.status = MessageStatus.sent,
    this.failureReason,
  });
}

class ChatPage extends StatefulWidget {
  final KeyId keyId;
  final String walletName;

  const ChatPage({super.key, required this.keyId, required this.walletName});

  @override
  State<ChatPage> createState() => _ChatPageState();
}

class _ChatPageState extends State<ChatPage> {
  final TextEditingController _messageController = TextEditingController();
  final ScrollController _scrollController = ScrollController();
  late final FocusNode _inputFocusNode;
  final List<ChatMessage> _messages = [];
  final Map<NostrEventId, ChatMessage> _messageById = {};
  final Map<NostrEventId, SigningRequestState> _signingRequests = {};
  final List<FfiSigningEvent> _signingEvents = [];
  final Set<String> _seenSigningEventIds = {};
  final List<({NostrEventId eventId, int timestamp, String reason})> _errorEvents = [];
  List<PublicKey> _memberPubkeys = [];
  StreamSubscription<FfiChannelEvent>? _subscription;
  NostrClient? _client;
  FfiConnectionState _connectionState = const FfiConnectionState.connecting();
  ChatMessage? _replyingTo;
  ({AccessStructureRef asRef, String testMessage})? _pendingSignRequest;

  NostrContext? _nostrContext;
  PublicKey? get _myPubkey => _nostrContext?.myPubkey;

  @override
  void initState() {
    super.initState();
    _inputFocusNode = FocusNode(onKeyEvent: _handleKeyEvent);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _inputFocusNode.requestFocus();
    });
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final nostr = NostrContext.of(context);
    if (_nostrContext == null) {
      _nostrContext = nostr;
      _connect();
    }
  }

  KeyEventResult _handleKeyEvent(FocusNode node, KeyEvent event) {
    if (event is KeyDownEvent &&
        event.logicalKey == LogicalKeyboardKey.enter &&
        !HardwareKeyboard.instance.isShiftPressed) {
      _sendMessage();
      return KeyEventResult.handled;
    }
    return KeyEventResult.ignored;
  }

  Future<void> _connect() async {
    _client = await NostrClient.connect(coordinator: coord);
    final stream = _client!.connectToChannel(keyId: widget.keyId);
    _subscription = stream.listen(_handleEvent);
  }

  FfiNostrProfile? _getProfile(PublicKey pubkey) {
    return _nostrContext!.getProfile(pubkey);
  }

  void _handleEvent(FfiChannelEvent event) {
    if (!mounted) return;

    setState(() {
      switch (event) {
        case FfiChannelEvent_ChatMessage(
          :final messageId,
          :final author,
          :final content,
          :final timestamp,
          :final replyTo,
          :final pending,
        ):
          final existing = _messageById[messageId];
          if (existing != null) {
            return;
          }
          final isMe = _myPubkey != null && author.equals(other: _myPubkey!);
          final message = ChatMessage(
            messageId: messageId,
            author: author,
            content: content,
            timestamp: DateTime.fromMillisecondsSinceEpoch(timestamp * 1000),
            isMe: isMe,
            replyTo: replyTo,
            status: pending ? MessageStatus.pending : MessageStatus.sent,
          );
          _messages.add(message);
          _messageById[messageId] = message;
          _messages.sort((a, b) => a.timestamp.compareTo(b.timestamp));
          WidgetsBinding.instance.addPostFrameCallback(
            (_) => _scrollToBottom(),
          );

        case FfiChannelEvent_MessageSent(:final messageId):
          // ✅ Our message was confirmed by relay
          final msg = _messageById[messageId];
          if (msg != null) {
            msg.status = MessageStatus.sent;
          }

        case FfiChannelEvent_MessageSendFailed(:final messageId, :final reason):
          // ❌ Our message failed to send
          final msg = _messageById[messageId];
          if (msg != null) {
            msg.status = MessageStatus.failed;
            msg.failureReason = reason;
          }

        case FfiChannelEvent_ConnectionState(:final field0):
          _connectionState = field0;

        case FfiChannelEvent_GroupMetadata(:final members):
          _memberPubkeys = members.map((m) => m.pubkey).toList();
          _nostrContext!.updateProfilesFromChannel(members);

        case FfiChannelEvent_SigningEvent(:final field0):
          final eventId = switch (field0) {
            FfiSigningEvent_Request(:final eventId) => eventId,
            FfiSigningEvent_Offer(:final eventId) => eventId,
            FfiSigningEvent_Partial(:final eventId) => eventId,
          };
          final idHex = eventId.toHex();
          if (!_seenSigningEventIds.add(idHex)) break;

          _signingEvents.add(field0);

          switch (field0) {
            case FfiSigningEvent_Request():
              _signingRequests[field0.eventId] = SigningRequestState(field0);
              // 🔁 replay any offers/partials that arrived before this request
              for (final prev in _signingEvents) {
                switch (prev) {
                  case FfiSigningEvent_Offer(:final requestId):
                    if (requestId == field0.eventId) {
                      _signingRequests[field0.eventId]!
                          .offers[prev.author.toHex()] = prev;
                    }
                  case FfiSigningEvent_Partial(:final requestId):
                    if (requestId == field0.eventId) {
                      _signingRequests[field0.eventId]!
                          .partials[prev.author.toHex()] = prev;
                    }
                  default:
                    break;
                }
              }
            case FfiSigningEvent_Offer():
              final state = _signingRequests[field0.requestId];
              if (state != null) {
                state.offers[field0.author.toHex()] = field0;
              }
            case FfiSigningEvent_Partial():
              final state = _signingRequests[field0.requestId];
              if (state != null) {
                state.partials[field0.author.toHex()] = field0;
              }
          }

        case FfiChannelEvent_SessionPromoted(
          :final requestId,
          :final sessionId,
        ):
          final state = _signingRequests[requestId];
          if (state != null) {
            state.sessionId = sessionId;
          }

        case FfiChannelEvent_Error(
          :final eventId,
          :final timestamp,
          :final reason,
        ):
          _errorEvents.add((
            eventId: eventId,
            timestamp: timestamp,
            reason: reason,
          ));
      }
    });
  }

  List<({DateTime time, Object item})> _buildTimelineItems() {
    final items = <({DateTime time, Object item})>[];
    for (final msg in _messages) {
      items.add((time: msg.timestamp, item: msg));
    }
    for (final event in _signingEvents) {
      final timestamp = switch (event) {
        FfiSigningEvent_Request(:final timestamp) => timestamp,
        FfiSigningEvent_Offer(:final timestamp) => timestamp,
        FfiSigningEvent_Partial(:final timestamp) => timestamp,
      };
      items.add((
        time: DateTime.fromMillisecondsSinceEpoch(timestamp * 1000),
        item: event,
      ));
    }
    for (final err in _errorEvents) {
      items.add((
        time: DateTime.fromMillisecondsSinceEpoch(err.timestamp * 1000),
        item: err,
      ));
    }
    items.sort((a, b) => a.time.compareTo(b.time));
    return items;
  }

  Widget _buildTimeline(ThemeData theme) {
    final items = _buildTimelineItems();
    if (items.isEmpty) {
      return Center(
        child: _connectionState is FfiConnectionState_Connected
            ? Text(
                'No messages yet.\nSend a message to start the conversation.',
                textAlign: TextAlign.center,
                style: theme.textTheme.bodyMedium?.copyWith(
                  color: theme.colorScheme.onSurfaceVariant,
                ),
              )
            : const CircularProgressIndicator(),
      );
    }
    return ListView.builder(
      controller: _scrollController,
      padding: const EdgeInsets.all(16),
      itemCount: items.length,
      itemBuilder: (context, index) {
        final entry = items[index];
        if (entry.item is ChatMessage) {
          final message = entry.item as ChatMessage;
          return _MessageBubble(
            message: message,
            profile: _getProfile(message.author),
            replyToMessage:
                message.replyTo != null ? _messageById[message.replyTo] : null,
            onReply: () => _startReply(message),
            onRetry: () => _retryMessage(message),
            onCopy: () => _copyMessage(message),
          );
        } else if (entry.item is FfiSigningEvent) {
          final event = entry.item as FfiSigningEvent;
          return switch (event) {
            FfiSigningEvent_Request() => _buildRequestCard(event),
            FfiSigningEvent_Offer() => _buildOfferCard(event),
            FfiSigningEvent_Partial() => _buildPartialCard(event),
          };
        } else if (entry.item
            is ({NostrEventId eventId, int timestamp, String reason})) {
          final err = entry.item
              as ({NostrEventId eventId, int timestamp, String reason});
          return SigningEventCard.error(text: err.reason);
        }
        return const SizedBox.shrink();
      },
    );
  }

  Widget _buildRequestCard(FfiSigningEvent_Request request) {
    final state = _signingRequests[request.eventId];
    if (state == null) {
      return SigningEventCard.error(text: 'Unknown signing request');
    }
    final myPubkey = _myPubkey;
    final iOffered =
        myPubkey != null && state.offers.containsKey(myPubkey.toHex());
    final accessStruct =
        coord.getAccessStructure(asRef: state.request.accessStructureRef);
    final threshold = accessStruct?.threshold() ?? 0;
    final reqIsMe = myPubkey != null && state.request.author.equals(other: myPubkey);
    return SigningRequestCard(
      state: state,
      threshold: threshold,
      isMe: reqIsMe,
      profile: _getProfile(state.request.author),
      getDisplayName: (pubkey) => getDisplayName(_getProfile(pubkey), pubkey),
      onOfferToSign: iOffered ? null : () => _onOfferToSign(state),
    );
  }

  Widget _buildOfferCard(FfiSigningEvent_Offer offer) {
    final requestState = _signingRequests[offer.requestId];
    final isMe =
        _myPubkey != null && offer.author.equals(other: _myPubkey!);
    int? threshold;
    if (requestState != null) {
      final accessStruct = coord.getAccessStructure(
          asRef: requestState.request.accessStructureRef);
      threshold = accessStruct?.threshold();
    }
    String? requestAuthorName;
    if (requestState != null) {
      final reqAuthor = requestState.request.author;
      final reqIsMe = _myPubkey != null && reqAuthor.equals(other: _myPubkey!);
      requestAuthorName = reqIsMe
          ? 'You'
          : getDisplayName(_getProfile(reqAuthor), reqAuthor);
    }
    final canSign = requestState != null &&
        requestState.sessionId != null &&
        requestState.myOfferedDevice != null &&
        !requestState.signingInProgress;
    return SigningEventCard.offer(
      author: offer.author,
      profile: _getProfile(offer.author),
      isMe: isMe,
      shareIndex: offer.shareIndex,
      isOrphaned: requestState == null,
      requestState: requestState,
      requestAuthorName: requestAuthorName,
      threshold: threshold,
      onSign: canSign ? () => _triggerDeviceSigning(requestState) : null,
    );
  }

  Widget _buildPartialCard(FfiSigningEvent_Partial partial) {
    final hasRequest = _signingRequests.containsKey(partial.requestId);
    final isMe =
        _myPubkey != null && partial.author.equals(other: _myPubkey!);
    return SigningEventCard.partial(
      author: partial.author,
      profile: _getProfile(partial.author),
      isMe: isMe,
      isOrphaned: !hasRequest,
    );
  }

  Future<void> _onOfferToSign(SigningRequestState state) async {
    final frostKey = coord.getFrostKey(keyId: widget.keyId);
    if (frostKey == null || _client == null) return;

    final accessStructure = frostKey.accessStructures()[0];
    final devices = accessStructure.devices();

    final selectedDevice = await showDialog<DeviceId>(
      context: context,
      builder: (context) {
        return AlertDialog(
          title: const Text('Select Device'),
          content: SizedBox(
            width: double.maxFinite,
            child: ListView.builder(
              shrinkWrap: true,
              itemCount: devices.length,
              itemBuilder: (context, index) {
                final id = devices[index];
                final name = coord.getDeviceName(id: id);
                final enoughNonces = coord.noncesAvailable(id: id) >= 1;
                return ListTile(
                  title: Text(
                    '${name ?? '<unknown>'}${enoughNonces ? '' : ' (no nonces)'}',
                  ),
                  enabled: enoughNonces,
                  onTap: enoughNonces ? () => Navigator.pop(context, id) : null,
                );
              },
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context),
              child: const Text('Cancel'),
            ),
          ],
        );
      },
    );

    if (selectedDevice == null) return;

    final nsec = _nostrContext!.nostrSettings.getNsec();
    try {
      await _client!.offerToSign(
        keyId: widget.keyId,
        nsec: nsec,
        requestId: state.request.eventId,
        deviceId: selectedDevice,
      );
      setState(() {
        state.myOfferedDevice = selectedDevice;
      });
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Failed to offer to sign: $e')),
        );
      }
    }
  }

  Future<void> _triggerDeviceSigning(SigningRequestState state) async {
    final deviceId = state.myOfferedDevice!;
    final sessionId = state.sessionId!;
    state.signingInProgress = true;

    try {
      final signingStream = coord
          .tryRestoreSigningSession(sessionId: sessionId)
          .toBehaviorSubject();

      final gotShares = signingStream
          .asyncMap((s) => deviceIdSet(s.gotShares).contains(deviceId) ? true : null)
          .firstWhere((done) => done != null);

      signingStream.forEach((signingState) async {
        final needRequest = deviceIdSet(signingState.connectedButNeedRequest);
        if (needRequest.contains(deviceId)) {
          final encryptionKey = await SecureKeyProvider.getEncryptionKey();
          coord.requestDeviceSign(
            deviceId: deviceId,
            sessionId: sessionId,
            encryptionKey: encryptionKey,
          );
        }
      });

      final result = await showDeviceActionDialog(
        context: context,
        complete: gotShares,
        builder: (context) {
          return Column(
            children: [
              DialogHeader(
                child: Column(
                  children: [
                    const Text('Signing'),
                    const SizedBox(height: 10),
                    const Text('Plug in your device'),
                  ],
                ),
              ),
              DeviceSigningProgress(stream: signingStream),
            ],
          );
        },
      );

      if (result == null) {
        coord.cancelSignSession(ssid: sessionId);
        coord.cancelProtocol();
        return;
      }

      final nsec = _nostrContext!.nostrSettings.getNsec();
      await _client!.sendSignPartial(
        keyId: widget.keyId,
        nsec: nsec,
        requestId: state.request.eventId,
        sessionId: sessionId,
        deviceId: deviceId,
      );
    } catch (e) {
      debugPrint('Device signing failed: $e');
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Signing failed: $e')),
        );
      }
    }
  }

  void _showActionMenu() {
    showBottomSheetOrDialog(
      context,
      title: const Text('Actions'),
      builder: (context, scrollController) {
        final theme = Theme.of(context);
        return ListView(
          controller: scrollController,
          shrinkWrap: true,
          padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
          children: [
            ListTile(
              shape: RoundedRectangleBorder(
                borderRadius: BorderRadius.circular(24),
              ),
              tileColor: theme.colorScheme.surfaceContainer,
              leading: const Icon(Icons.draw),
              title: const Text('Sign Message'),
              onTap: () {
                Navigator.pop(context);
                _proposeTestSign();
              },
            ),
          ],
        );
      },
    );
  }

  Future<void> _proposeTestSign() async {
    final testMessage = await showDialog<String>(
      context: context,
      builder: (context) {
        final controller = TextEditingController();
        return AlertDialog(
          title: const Text('Sign Message'),
          content: TextField(
            controller: controller,
            autofocus: true,
            decoration: const InputDecoration(
              labelText: 'Message to sign',
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context),
              child: const Text('Cancel'),
            ),
            FilledButton(
              onPressed: () => Navigator.pop(context, controller.text),
              child: const Text('Attach'),
            ),
          ],
        );
      },
    );
    if (testMessage == null || testMessage.trim().isEmpty) return;

    final frostKey = coord.getFrostKey(keyId: widget.keyId);
    if (frostKey == null) return;
    final asRef = frostKey.accessStructures()[0].accessStructureRef();

    setState(() {
      _pendingSignRequest = (asRef: asRef, testMessage: testMessage.trim());
    });
    _inputFocusNode.requestFocus();
  }

  void _scrollToBottom() {
    if (_scrollController.hasClients) {
      _scrollController.animateTo(
        _scrollController.position.maxScrollExtent,
        duration: const Duration(milliseconds: 300),
        curve: Curves.easeOut,
      );
    }
  }

  Future<void> _sendMessage() async {
    final content = _messageController.text.trim();
    final pending = _pendingSignRequest;
    if (content.isEmpty && pending == null) return;
    if (_client == null) return;

    final nsec = _nostrContext!.nostrSettings.getNsec();
    final replyToId = _replyingTo?.messageId;
    _messageController.clear();
    setState(() {
      _replyingTo = null;
      _pendingSignRequest = null;
    });

    try {
      if (pending != null) {
        await _client!.sendTestSignRequest(
          accessStructureRef: pending.asRef,
          nsec: nsec,
          testMessage: pending.testMessage,
          message: content.isEmpty ? null : content,
        );
      } else {
        await _client!.sendMessage(
          keyId: widget.keyId,
          nsec: nsec,
          content: content,
          replyTo: replyToId,
        );
      }
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Failed to send: $e')),
        );
      }
    }
    _inputFocusNode.requestFocus();
  }

  Future<void> _retryMessage(ChatMessage message) async {
    if (message.status != MessageStatus.failed || _client == null) return;

    final nsec = _nostrContext!.nostrSettings.getNsec();
    setState(() {
      _messages.remove(message);
      _messageById.remove(message.messageId);
    });

    await _client!.sendMessage(
      keyId: widget.keyId,
      nsec: nsec,
      content: message.content,
      replyTo: message.replyTo,
    );
  }

  void _copyMessage(ChatMessage message) {
    Clipboard.setData(ClipboardData(text: message.content));
  }

  void _startReply(ChatMessage message) {
    setState(() => _replyingTo = message);
    _inputFocusNode.requestFocus();
  }

  void _cancelReply() {
    setState(() => _replyingTo = null);
  }

  void _openGroupInfo() {
    Navigator.of(context).push(
      MaterialPageRoute(
        builder: (context) => GroupInfoPage(
          walletName: widget.walletName,
          members: _memberPubkeys,
        ),
      ),
    );
  }

  @override
  void dispose() {
    _subscription?.cancel();
    _client?.disconnectChannel(keyId: widget.keyId);
    _messageController.dispose();
    _scrollController.dispose();
    _inputFocusNode.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return Scaffold(
      appBar: AppBar(
        title: Text(widget.walletName),
        actions: [
          _buildConnectionIndicator(),
          const SizedBox(width: 8),
          IconButton(
            icon: const Icon(Icons.group),
            tooltip: 'Group Info',
            onPressed: _openGroupInfo,
          ),
        ],
      ),
      body: Column(
        children: [
          Expanded(child: _buildTimeline(theme)),
          _buildMessageInput(),
        ],
      ),
    );
  }

  Widget _buildConnectionIndicator() {
    final (color, tooltip) = switch (_connectionState) {
      FfiConnectionState_Connecting() => (Colors.orange, 'Connecting...'),
      FfiConnectionState_Connected() => (Colors.green, 'Connected'),
      FfiConnectionState_Disconnected(:final reason) => (
        Colors.red,
        'Disconnected${reason != null ? ': $reason' : ''}',
      ),
    };

    return Tooltip(
      message: tooltip,
      child: Container(
        width: 12,
        height: 12,
        decoration: BoxDecoration(color: color, shape: BoxShape.circle),
      ),
    );
  }

  Widget _buildMessageInput() {
    final theme = Theme.of(context);
    final isConnected = _connectionState is FfiConnectionState_Connected;

    return SafeArea(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (_replyingTo != null)
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
              decoration: BoxDecoration(
                color: theme.colorScheme.surfaceContainerHighest,
                border: Border(
                  top: BorderSide(
                    color: theme.colorScheme.outline.withValues(alpha: 0.2),
                  ),
                ),
              ),
              child: Row(
                children: [
                  Container(
                    width: 3,
                    height: 32,
                    margin: const EdgeInsets.only(right: 8),
                    decoration: BoxDecoration(
                      color: theme.colorScheme.primary,
                      borderRadius: BorderRadius.circular(2),
                    ),
                  ),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          _replyingTo!.isMe
                              ? 'Replying to yourself'
                              : 'Replying to ${getDisplayName(_getProfile(_replyingTo!.author), _replyingTo!.author)}',
                          style: theme.textTheme.labelSmall?.copyWith(
                            color: theme.colorScheme.primary,
                            fontWeight: FontWeight.w600,
                          ),
                        ),
                        Text(
                          _replyingTo!.content,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: theme.textTheme.bodySmall?.copyWith(
                            color: theme.colorScheme.onSurfaceVariant,
                          ),
                        ),
                      ],
                    ),
                  ),
                  IconButton(
                    icon: const Icon(Icons.close, size: 18),
                    onPressed: _cancelReply,
                    padding: EdgeInsets.zero,
                    constraints: const BoxConstraints(),
                  ),
                ],
              ),
            ),
          if (_pendingSignRequest != null)
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
              decoration: BoxDecoration(
                color: theme.colorScheme.secondaryContainer.withValues(alpha: 0.5),
                border: Border(
                  top: BorderSide(
                    color: theme.colorScheme.outline.withValues(alpha: 0.2),
                  ),
                ),
              ),
              child: Row(
                children: [
                  Container(
                    width: 3,
                    height: 32,
                    margin: const EdgeInsets.only(right: 8),
                    decoration: BoxDecoration(
                      color: theme.colorScheme.secondary,
                      borderRadius: BorderRadius.circular(2),
                    ),
                  ),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          'Signing Request',
                          style: theme.textTheme.labelSmall?.copyWith(
                            color: theme.colorScheme.secondary,
                            fontWeight: FontWeight.w600,
                          ),
                        ),
                        Text(
                          _pendingSignRequest!.testMessage,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: theme.textTheme.bodySmall?.copyWith(
                            color: theme.colorScheme.onSurfaceVariant,
                          ),
                        ),
                      ],
                    ),
                  ),
                  IconButton(
                    icon: const Icon(Icons.close, size: 18),
                    onPressed: () => setState(() => _pendingSignRequest = null),
                    padding: EdgeInsets.zero,
                    constraints: const BoxConstraints(),
                  ),
                ],
              ),
            ),
          Padding(
            padding: const EdgeInsets.all(8),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.end,
              children: [
                Expanded(
                  child: TextField(
                    controller: _messageController,
                    focusNode: _inputFocusNode,
                    autofocus: true,
                    enabled: isConnected,
                    minLines: 1,
                    maxLines: 6,
                    decoration: InputDecoration(
                      hintText: isConnected
                          ? 'Type a message...'
                          : 'Connecting...',
                      border: OutlineInputBorder(
                        borderRadius: BorderRadius.circular(24),
                      ),
                      contentPadding: const EdgeInsets.symmetric(
                        horizontal: 16,
                        vertical: 12,
                      ),
                    ),
                    textInputAction: TextInputAction.newline,
                  ),
                ),
                const SizedBox(width: 8),
                IconButton(
                  onPressed: isConnected ? _showActionMenu : null,
                  icon: const Icon(Icons.add),
                  tooltip: 'Actions',
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

class _MessageBubble extends StatefulWidget {
  final ChatMessage message;
  final FfiNostrProfile? profile;
  final ChatMessage? replyToMessage;
  final VoidCallback onReply;
  final VoidCallback onRetry;
  final VoidCallback onCopy;

  const _MessageBubble({
    required this.message,
    this.profile,
    required this.replyToMessage,
    required this.onReply,
    required this.onRetry,
    required this.onCopy,
  });

  @override
  State<_MessageBubble> createState() => _MessageBubbleState();
}

class _MessageBubbleState extends State<_MessageBubble> {
  bool _isHovered = false;

  bool _isMobile(BuildContext context) {
    return MediaQuery.of(context).size.width < 600;
  }

  void _showMobileActions(BuildContext context) {
    final theme = Theme.of(context);
    final isFailed = widget.message.status == MessageStatus.failed;

    showDialog(
      context: context,
      barrierColor: Colors.black54,
      builder: (dialogContext) {
        return GestureDetector(
          onTap: () => Navigator.of(dialogContext).pop(),
          behavior: HitTestBehavior.opaque,
          child: Stack(
            children: [
              Center(
                child: GestureDetector(
                  onTap: () {},
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      _buildBubbleContent(context, theme),
                      const SizedBox(height: 8),
                      Material(
                        elevation: 8,
                        borderRadius: BorderRadius.circular(12),
                        color: theme.colorScheme.surface,
                        child: Column(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            if (isFailed)
                              _buildActionTile(
                                icon: Icons.refresh,
                                label: 'Retry',
                                color: theme.colorScheme.error,
                                onTap: () {
                                  Navigator.of(dialogContext).pop();
                                  widget.onRetry();
                                },
                              ),
                            _buildActionTile(
                              icon: Icons.copy,
                              label: 'Copy',
                              onTap: () {
                                Navigator.of(dialogContext).pop();
                                widget.onCopy();
                              },
                            ),
                            _buildActionTile(
                              icon: Icons.reply,
                              label: 'Reply',
                              onTap: () {
                                Navigator.of(dialogContext).pop();
                                widget.onReply();
                              },
                            ),
                          ],
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ],
          ),
        );
      },
    );
  }

  Widget _buildActionTile({
    required IconData icon,
    required String label,
    Color? color,
    required VoidCallback onTap,
  }) {
    final theme = Theme.of(context);
    return InkWell(
      onTap: onTap,
      borderRadius: BorderRadius.circular(12),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 24, vertical: 14),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(icon, size: 20, color: color ?? theme.colorScheme.onSurface),
            const SizedBox(width: 12),
            Text(
              label,
              style: theme.textTheme.bodyMedium?.copyWith(
                color: color ?? theme.colorScheme.onSurface,
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildBubbleContent(BuildContext context, ThemeData theme) {
    final isMe = widget.message.isMe;
    final isFailed = widget.message.status == MessageStatus.failed;

    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
      constraints: BoxConstraints(
        maxWidth: MediaQuery.of(context).size.width * 0.7,
      ),
      decoration: BoxDecoration(
        color: isFailed
            ? theme.colorScheme.errorContainer
            : isMe
            ? theme.colorScheme.primaryContainer
            : theme.colorScheme.surfaceContainerHighest,
        borderRadius: BorderRadius.circular(16),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          if (!isMe)
            Text(
              getDisplayName(widget.profile, widget.message.author),
              style: theme.textTheme.labelSmall?.copyWith(
                color: theme.colorScheme.primary,
              ),
            ),
          if (widget.replyToMessage != null) _buildReplyQuote(theme),
          Text(widget.message.content),
          const SizedBox(height: 2),
          Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(
                _formatTime(widget.message.timestamp),
                style: theme.textTheme.labelSmall?.copyWith(
                  color: theme.colorScheme.onSurfaceVariant,
                ),
              ),
              if (isMe) ...[
                const SizedBox(width: 4),
                _buildStatusIndicator(theme),
              ],
            ],
          ),
          if (isFailed)
            Padding(
              padding: const EdgeInsets.only(top: 4),
              child: Text(
                'Tap to retry',
                style: theme.textTheme.labelSmall?.copyWith(
                  color: theme.colorScheme.error,
                  fontWeight: FontWeight.w600,
                ),
              ),
            ),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final isMe = widget.message.isMe;
    final isFailed = widget.message.status == MessageStatus.failed;
    final isMobile = _isMobile(context);
    final bubble = _buildBubbleContent(context, theme);

    final hoverActions = AnimatedOpacity(
            opacity: _isHovered ? 1.0 : 0.0,
            duration: const Duration(milliseconds: 150),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                if (isFailed)
                  IconButton(
                    icon: Icon(
                      Icons.refresh,
                      size: 18,
                      color: theme.colorScheme.error,
                    ),
                    onPressed: widget.onRetry,
                    padding: EdgeInsets.zero,
                    constraints: const BoxConstraints(),
                    splashRadius: 14,
                    tooltip: 'Retry',
                  ),
                IconButton(
                  icon: Icon(
                    Icons.copy,
                    size: 18,
                    color: theme.colorScheme.onSurfaceVariant,
                  ),
                  onPressed: widget.onCopy,
                  padding: EdgeInsets.zero,
                  constraints: const BoxConstraints(),
                  splashRadius: 14,
                  tooltip: 'Copy',
                ),
                IconButton(
                  icon: Icon(
                    Icons.reply,
                    size: 18,
                    color: theme.colorScheme.onSurfaceVariant,
                  ),
                  onPressed: widget.onReply,
                  padding: EdgeInsets.zero,
                  constraints: const BoxConstraints(),
                  splashRadius: 14,
                  tooltip: 'Reply',
                ),
              ],
            ),
          );

    final avatar = !isMe
        ? Padding(
            padding: const EdgeInsets.only(right: 8),
            child: NostrAvatar.small(
              profile: widget.profile,
              pubkey: widget.message.author,
            ),
          )
        : null;

    return Align(
      alignment: isMe ? Alignment.centerRight : Alignment.centerLeft,
      child: MouseRegion(
        onEnter: isMobile ? null : (_) => setState(() => _isHovered = true),
        onExit: isMobile ? null : (_) => setState(() => _isHovered = false),
        child: GestureDetector(
          onTap: isFailed ? widget.onRetry : null,
          onLongPress: isMobile ? () => _showMobileActions(context) : null,
          child: Padding(
            padding: const EdgeInsets.symmetric(vertical: 4),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: isMe
                  ? [
                      if (!isMobile) hoverActions,
                      if (!isMobile) const SizedBox(width: 4),
                      bubble,
                    ]
                  : [
                      if (avatar != null) avatar,
                      bubble,
                      if (!isMobile) const SizedBox(width: 4),
                      if (!isMobile) hoverActions,
                    ],
            ),
          ),
        ),
      ),
    );
  }

  Widget _buildStatusIndicator(ThemeData theme) {
    return switch (widget.message.status) {
      MessageStatus.pending => Icon(
        Icons.access_time,
        size: 12,
        color: theme.colorScheme.outline,
      ),
      MessageStatus.sent => Icon(
        Icons.check,
        size: 12,
        color: theme.colorScheme.outline,
      ),
      MessageStatus.failed => Icon(
        Icons.error_outline,
        size: 12,
        color: theme.colorScheme.error,
      ),
    };
  }

  Widget _buildReplyQuote(ThemeData theme) {
    final replyTo = widget.replyToMessage!;
    return Container(
      margin: const EdgeInsets.only(bottom: 6),
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
      decoration: BoxDecoration(
        border: Border(
          left: BorderSide(color: theme.colorScheme.primary, width: 2),
        ),
        color: theme.colorScheme.surface.withValues(alpha: 0.5),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            replyTo.isMe ? 'You' : getDisplayName(null, replyTo.author),
            style: theme.textTheme.labelSmall?.copyWith(
              color: theme.colorScheme.primary,
              fontWeight: FontWeight.w600,
            ),
          ),
          Text(
            replyTo.content,
            maxLines: 2,
            overflow: TextOverflow.ellipsis,
            style: theme.textTheme.bodySmall?.copyWith(
              color: theme.colorScheme.onSurfaceVariant,
            ),
          ),
        ],
      ),
    );
  }

  String _formatTime(DateTime time) {
    return '${time.hour.toString().padLeft(2, '0')}:${time.minute.toString().padLeft(2, '0')}';
  }
}
