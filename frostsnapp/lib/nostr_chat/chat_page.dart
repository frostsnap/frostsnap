import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnap/nostr_chat/group_info_page.dart';
import 'package:frostsnap/nostr_chat/member_detail_sheet.dart';
import 'package:frostsnap/nostr_chat/nostr_profile.dart';
import 'package:frostsnap/nostr_chat/nostr_state.dart';
import 'package:frostsnap/nostr_chat/signing_card.dart';
import 'package:frostsnap/device_action.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/secure_key_provider.dart';
import 'package:frostsnap/sign_message.dart';
import 'package:frostsnap/snackbar.dart';
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
  final IconData? quoteIcon;
  MessageStatus status;
  String? failureReason;

  ChatMessage({
    required this.messageId,
    required this.author,
    required this.content,
    required this.timestamp,
    required this.isMe,
    this.replyTo,
    this.quoteIcon,
    this.status = MessageStatus.sent,
    this.failureReason,
  });
}

class ReplyTarget {
  final NostrEventId eventId;
  final PublicKey author;
  final String preview;
  final bool isMe;

  ReplyTarget({
    required this.eventId,
    required this.author,
    required this.preview,
    required this.isMe,
  });
}

sealed class TimelineItem {
  DateTime get timestamp;
}

class TimelineChat extends TimelineItem {
  final ChatMessage message;
  @override
  DateTime get timestamp => message.timestamp;
  TimelineChat(this.message);
}

class TimelineSigning extends TimelineItem {
  final FfiSigningEvent event;
  @override
  final DateTime timestamp;
  TimelineSigning(this.event)
      : timestamp = DateTime.fromMillisecondsSinceEpoch(
            switch (event) {
              FfiSigningEvent_Request(:final timestamp) => timestamp,
              FfiSigningEvent_Offer(:final timestamp) => timestamp,
              FfiSigningEvent_Partial(:final timestamp) => timestamp,
            } *
            1000);
}

class TimelineError extends TimelineItem {
  final NostrEventId eventId;
  final PublicKey author;
  final String reason;
  @override
  final DateTime timestamp;
  TimelineError({
    required this.eventId,
    required this.author,
    required int timestamp,
    required this.reason,
  }) : timestamp = DateTime.fromMillisecondsSinceEpoch(timestamp * 1000);
}

class TimelineSigningComplete extends TimelineItem {
  final SigningRequestState requestState;
  @override
  final DateTime timestamp;
  TimelineSigningComplete(this.requestState)
      : timestamp = DateTime.now();
}

class ChatPage extends StatefulWidget {
  final AccessStructureRef accessStructureRef;
  final String walletName;

  const ChatPage({super.key, required this.accessStructureRef, required this.walletName});

  KeyId get keyId => accessStructureRef.keyId;
  AccessStructureId get accessStructureId => accessStructureRef.accessStructureId;

  @override
  State<ChatPage> createState() => _ChatPageState();
}

class _ChatPageState extends State<ChatPage> {
  final TextEditingController _messageController = TextEditingController();
  final ScrollController _scrollController = ScrollController();
  late final FocusNode _inputFocusNode;
  final List<TimelineItem> _timeline = [];
  final Map<NostrEventId, ChatMessage> _messageById = {};
  final Map<String, GlobalKey> _timelineKeys = {};
  String? _highlightedId;
  final Map<NostrEventId, SigningRequestState> _signingRequests = {};
  final Set<String> _seenSigningEventIds = {};
  List<PublicKey> _memberPubkeys = [];
  StreamSubscription<FfiChannelEvent>? _subscription;
  NostrClient? _client;
  FfiConnectionState _connectionState = const FfiConnectionState.connecting();
  ReplyTarget? _replyingTo;
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
    _client = await NostrClient.connect();
    final encryptionKey = await SecureKeyProvider.getEncryptionKey();
    final stream = _client!.connectToChannel(
      coord: coord,
      accessStructureRef: widget.accessStructureRef,
      encryptionKey: encryptionKey,
    );
    _subscription = stream.listen(_handleEvent);
  }

  FfiNostrProfile? _getProfile(PublicKey pubkey) {
    return _nostrContext!.getProfile(pubkey);
  }

  void _showMemberProfile(PublicKey pubkey) {
    showModalBottomSheet(
      context: context,
      isScrollControlled: true,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(16)),
      ),
      builder: (_) => MemberDetailSheet(
        pubkey: pubkey,
        profile: _getProfile(pubkey),
      ),
    );
  }

  DeviceId? _getMyDevice(SigningRequestState? state) {
    if (state == null || _myPubkey == null) return null;
    final myOffer = state.offers[_myPubkey!.toHex()];
    if (myOffer == null) return null;
    return _deviceForShareIndex(
      state.request.accessStructureRef,
      myOffer.shareIndex,
    );
  }

  DeviceId? _deviceForShareIndex(AccessStructureRef asRef, int shareIndex) {
    final accessStruct = coord.getAccessStructure(asRef: asRef);
    if (accessStruct == null) return null;
    for (final deviceId in accessStruct.devices()) {
      if (accessStruct.getDeviceShortShareIndex(deviceId: deviceId) == shareIndex) {
        return deviceId;
      }
    }
    return null;
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
          _messageById[messageId] = message;
          _insertTimelineItem(TimelineChat(message));
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

          final (author, timestamp, content) = switch (field0) {
            FfiSigningEvent_Request(:final author, :final timestamp, :final signingDetails, :final message) =>
              (author, timestamp, message.isNotEmpty ? message : signingDetailsText(signingDetails)),
            FfiSigningEvent_Offer(:final author, :final timestamp) =>
              (author, timestamp, 'offered to sign'),
            FfiSigningEvent_Partial(:final author, :final timestamp) =>
              (author, timestamp, 'signed'),
          };
          _messageById[eventId] = ChatMessage(
            messageId: eventId,
            author: author,
            content: content,
            timestamp: DateTime.fromMillisecondsSinceEpoch(timestamp * 1000),
            isMe: _myPubkey != null && author.equals(other: _myPubkey!),
            quoteIcon: Icons.draw,
          );

          _insertTimelineItem(TimelineSigning(field0));

          switch (field0) {
            case FfiSigningEvent_Request():
              _signingRequests[field0.eventId] = SigningRequestState(field0);
              for (final item in _timeline) {
                if (item is! TimelineSigning) continue;
                switch (item.event) {
                  case FfiSigningEvent_Offer(:final requestId):
                    if (requestId == field0.eventId) {
                      _signingRequests[field0.eventId]!
                          .offers[item.event.author.toHex()] = item.event as FfiSigningEvent_Offer;
                    }
                  case FfiSigningEvent_Partial(:final requestId):
                    if (requestId == field0.eventId) {
                      _signingRequests[field0.eventId]!
                          .partials[item.event.author.toHex()] = item.event as FfiSigningEvent_Partial;
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
                final accessStruct = coord.getAccessStructure(
                    asRef: state.request.accessStructureRef);
                final threshold = accessStruct?.threshold() ?? 0;
                if (state.partials.length >= threshold) {
                  _insertTimelineItem(TimelineSigningComplete(state));
                }
              }
          }

        case FfiChannelEvent_Error(
          :final eventId,
          :final author,
          :final timestamp,
          :final reason,
        ):
          _insertTimelineItem(TimelineError(
            eventId: eventId,
            author: author,
            timestamp: timestamp,
            reason: reason,
          ));
      }
    });
  }

  void _insertTimelineItem(TimelineItem item) {
    final ts = item.timestamp;
    var i = _timeline.length;
    while (i > 0 && _timeline[i - 1].timestamp.isAfter(ts)) {
      i--;
    }
    _timeline.insert(i, item);
  }

  Widget _buildTaskCard(SigningRequestState state) {
    final accessStruct =
        coord.getAccessStructure(asRef: state.request.accessStructureRef);
    final threshold = accessStruct?.threshold() ?? 0;
    final myPubkey = _myPubkey;
    final alreadySigned =
        myPubkey != null && state.partials.containsKey(myPubkey.toHex());
    final sealed = state.sealedData;
    final myDevice = _getMyDevice(state);
    final VoidCallback? onSign;
    final String? deviceName;
    if (sealed != null && myDevice != null && !alreadySigned) {
      deviceName = coord.getDeviceName(id: myDevice);
      onSign = () => _triggerDeviceSigning(state, sealed, myDevice);
    } else {
      deviceName = null;
      onSign = null;
    }

    return TransactionTaskCard(
      state: state,
      threshold: threshold,
      getDisplayName: _displayName,
      deviceName: deviceName,
      onSign: onSign,
    );
  }

  SigningRequestState? get _activeSigningRequest {
    final frostKey = coord.getFrostKey(keyId: widget.keyId);
    final threshold = frostKey?.accessStructures()[0].threshold() ?? 0;
    SigningRequestState? best;
    for (final state in _signingRequests.values) {
      if (state.partials.length >= threshold) continue;
      if (best == null || state.timestamp.isAfter(best.timestamp)) {
        best = state;
      }
    }
    return best;
  }

  Widget _buildTimeline(ThemeData theme) {
    if (_timeline.isEmpty) {
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
    final topPadding = MediaQuery.of(context).padding.top + kToolbarHeight + 8;
    return ListView.builder(
      controller: _scrollController,
      padding: EdgeInsets.fromLTRB(16, topPadding, 16, 16),
      itemCount: _timeline.length,
      itemBuilder: (context, index) {
        final item = _timeline[index];
        final showDateDivider = index == 0 ||
            !_isSameDay(item.timestamp, _timeline[index - 1].timestamp);
        final child = switch (item) {
          TimelineChat(:final message) => _MessageBubble(
            key: _timelineKeys.putIfAbsent(
                message.messageId.toHex(), () => GlobalKey()),
            message: message,
            profile: _getProfile(message.author),
            isHighlighted:
                _highlightedId == message.messageId.toHex(),
            replyToMessage:
                message.replyTo != null ? _messageById[message.replyTo] : null,
            onTapQuote: message.replyTo != null
                ? () => _scrollToAndHighlight(message.replyTo!)
                : null,
            onTapAvatar: message.isMe
                ? null
                : () => _showMemberProfile(message.author),
            onReply: () => _startReply(ReplyTarget(
              eventId: message.messageId,
              author: message.author,
              preview: message.content,
              isMe: message.isMe,
            )),
            onRetry: () => _retryMessage(message),
            onCopy: () => _copyMessage(message),
          ),
          TimelineSigning(:final event) => switch (event) {
            FfiSigningEvent_Request() => _buildRequestCard(event),
            FfiSigningEvent_Offer() => _buildOfferCard(event),
            FfiSigningEvent_Partial() => _buildPartialCard(event),
          },
          TimelineSigningComplete(:final requestState) =>
            _SigningCompleteCard(
              details: signingDetailsText(requestState.request.signingDetails),
              onShowSignature: () => _completeAndShowSignature(requestState),
            ),
          TimelineError() => SigningErrorCard(
            text: item.reason,
            author: item.author,
            profile: _getProfile(item.author),
            isMe: _myPubkey != null && item.author.equals(other: _myPubkey!),
            onCopy: () => Clipboard.setData(ClipboardData(text: item.reason)),
            onReply: () => _startReply(ReplyTarget(
              eventId: item.eventId,
              author: item.author,
              preview: 'Error: ${item.reason}',
              isMe: _myPubkey != null && item.author.equals(other: _myPubkey!),
            )),
            onTapAvatar: _myPubkey != null && item.author.equals(other: _myPubkey!)
                ? null
                : () => _showMemberProfile(item.author),
          ),
        };
        if (!showDateDivider) return child;
        return Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            _DateDivider(date: item.timestamp),
            child,
          ],
        );
      },
    );
  }

  bool _isSameDay(DateTime a, DateTime b) =>
      a.year == b.year && a.month == b.month && a.day == b.day;

  void _scrollToAndHighlight(NostrEventId eventId) {
    final idHex = eventId.toHex();
    final key = _timelineKeys[idHex];
    if (key?.currentContext == null) return;
    Scrollable.ensureVisible(
      key!.currentContext!,
      duration: const Duration(milliseconds: 300),
      curve: Curves.easeOut,
      alignment: 0.3,
    );
    setState(() => _highlightedId = idHex);
    Future.delayed(const Duration(milliseconds: 1200), () {
      if (mounted) setState(() => _highlightedId = null);
    });
  }

  Widget _buildRequestCard(FfiSigningEvent_Request request) {
    final state = _signingRequests[request.eventId];
    if (state == null) {
      return SigningErrorCard(text: 'Unknown signing request');
    }
    final myPubkey = _myPubkey;
    final iOffered =
        myPubkey != null && state.offers.containsKey(myPubkey.toHex());
    final accessStruct =
        coord.getAccessStructure(asRef: state.request.accessStructureRef);
    final threshold = accessStruct?.threshold() ?? 0;
    final reqIsMe = myPubkey != null && state.request.author.equals(other: myPubkey);
    final idHex = request.eventId.toHex();
    final key = _timelineKeys.putIfAbsent(idHex, () => GlobalKey());
    return SigningRequestCard(
      key: key,
      state: state,
      threshold: threshold,
      isMe: reqIsMe,
      isHighlighted: _highlightedId == idHex,
      profile: _getProfile(state.request.author),
      getDisplayName: (pubkey) => getDisplayName(_getProfile(pubkey), pubkey),
      onOfferToSign: iOffered ? null : () => _onOfferToSign(state),
      onCopy: () => _copySigningText(request),
      onReply: () => _startReply(_signingReplyTarget(request)),
      onTapAvatar: reqIsMe ? null : () => _showMemberProfile(state.request.author),
    );
  }

  String _signingMessagePreview(NostrEventId requestId) {
    final state = _signingRequests[requestId];
    if (state == null) return '?';
    final text = signingDetailsText(state.request.signingDetails);
    return text.length > 30 ? '${text.substring(0, 30)}...' : text;
  }

  Widget _buildOfferCard(FfiSigningEvent_Offer offer) {
    final isMe =
        _myPubkey != null && offer.author.equals(other: _myPubkey!);
    final preview = _signingMessagePreview(offer.requestId);
    return _SigningEventLine(
      profile: _getProfile(offer.author),
      author: offer.author,
      isMe: isMe,
      label: 'offered to sign',
      messagePill: preview,
      suffix: 'with key #${offer.shareIndex}',
      timestamp: DateTime.fromMillisecondsSinceEpoch(offer.timestamp * 1000),
      onTapPill: () => _scrollToAndHighlight(offer.requestId),
      onTapAvatar: isMe ? null : () => _showMemberProfile(offer.author),
    );
  }

  Widget _buildPartialCard(FfiSigningEvent_Partial partial) {
    final isMe =
        _myPubkey != null && partial.author.equals(other: _myPubkey!);
    final preview = _signingMessagePreview(partial.requestId);
    return _SigningEventLine(
      profile: _getProfile(partial.author),
      author: partial.author,
      isMe: isMe,
      label: 'signed',
      messagePill: preview,
      timestamp: DateTime.fromMillisecondsSinceEpoch(partial.timestamp * 1000),
      onTapPill: () => _scrollToAndHighlight(partial.requestId),
      onTapAvatar: isMe ? null : () => _showMemberProfile(partial.author),
    );
  }

  Future<void> _onOfferToSign(SigningRequestState state) async {
    final frostKey = coord.getFrostKey(keyId: widget.keyId);
    if (frostKey == null || _client == null) return;

    final accessStructure = frostKey.accessStructures()[0];
    final devices = accessStructure.devices();
    final offeredIndices = state.offers.values.map((o) => o.shareIndex).toSet();

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
                final shareIndex = accessStructure.getDeviceShortShareIndex(deviceId: id);
                final alreadyOffered = offeredIndices.contains(shareIndex);
                final enabled = enoughNonces && !alreadyOffered;
                final subtitle = alreadyOffered
                    ? 'already offered'
                    : !enoughNonces
                        ? 'no nonces'
                        : null;
                return ListTile(
                  title: Text(name ?? '<unknown>'),
                  subtitle: subtitle != null ? Text(subtitle) : null,
                  enabled: enabled,
                  onTap: enabled ? () => Navigator.pop(context, id) : null,
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
      final binonces = await coord.reserveNonces(
        deviceId: selectedDevice,
        nSignatures: 1,
      );
      await _client!.sendSignOffer(
        accessStructureId: widget.accessStructureId,
        nsec: nsec,
        replyTo: state.chainTip,
        binonces: binonces,
      );
    } catch (e) {
      if (mounted) {
        showErrorSnackbar(context, 'Failed to offer to sign: $e');
      }
    }
  }

  Future<void> _triggerDeviceSigning(SigningRequestState state, SealedSigningData sealed, DeviceId deviceId) async {
    try {
      final sessionId = await coord.signWithNonceReservation(
        signTask: sealed.signTask(),
        accessStructureRef: sealed.accessStructureRef(),
        allBinonces: sealed.binonces(),
        deviceId: deviceId,
      );

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
        coord.cancelProtocol();
        return;
      }

      final shares = coord.getDeviceSignatureShares(
        sessionId: sessionId,
        deviceId: deviceId,
      );
      if (shares == null) throw Exception('No signature shares from device');

      final nsec = _nostrContext!.nostrSettings.getNsec();
      await _client!.sendSignPartial(
        accessStructureId: widget.accessStructureId,
        nsec: nsec,
        requestId: state.request.eventId,
        sessionId: sessionId,
        shares: shares,
      );
    } catch (e) {
      if (mounted) {
        showErrorSnackbar(context, 'Signing failed: $e');
      }
    }
  }

  Future<void> _completeAndShowSignature(SigningRequestState state) async {
    final sealed = state.sealedData;
    if (sealed == null) return;

    try {
      final signatures = sealed.combineSignatures(
        allShares: state.partials.values.map((p) => p.shares).toList(),
      );
      if (signatures.isNotEmpty && mounted) {
        await showSignatureDialog(context, signatures[0]);
      }
    } catch (e) {
      if (mounted) {
        showErrorSnackbar(context, 'Failed to complete signing: $e');
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
            onSubmitted: (value) => Navigator.pop(context, value),
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
    final replyToId = _replyingTo?.eventId;
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
          message: content,
        );
      } else {
        await _client!.sendMessage(
          accessStructureId: widget.accessStructureId,
          nsec: nsec,
          content: content,
          replyTo: replyToId,
        );
      }
    } catch (e) {
      if (mounted) {
        showErrorSnackbar(context, 'Failed to send: $e');
      }
    }
    _inputFocusNode.requestFocus();
  }

  Future<void> _retryMessage(ChatMessage message) async {
    if (message.status != MessageStatus.failed || _client == null) return;

    final nsec = _nostrContext!.nostrSettings.getNsec();
    setState(() {
      _timeline.removeWhere((item) => item is TimelineChat && item.message == message);
      _messageById.remove(message.messageId);
    });

    await _client!.sendMessage(
      accessStructureId: widget.accessStructureId,
      nsec: nsec,
      content: message.content,
      replyTo: message.replyTo,
    );
  }

  void _copyMessage(ChatMessage message) {
    Clipboard.setData(ClipboardData(text: message.content));
  }

  String _displayName(PublicKey author) {
    if (_myPubkey != null && author.equals(other: _myPubkey!)) return 'You';
    return getDisplayName(_getProfile(author), author);
  }

  void _copySigningText(FfiSigningEvent event) {
    final text = switch (event) {
      FfiSigningEvent_Request(:final signingDetails, :final message) =>
        '${signingDetailsText(signingDetails)}${message.isNotEmpty ? '\n$message' : ''}',
      FfiSigningEvent_Offer(:final shareIndex, :final author) =>
        '${_displayName(author)} offered to sign with key #$shareIndex',
      FfiSigningEvent_Partial(:final author) =>
        '${_displayName(author)} signed',
    };
    Clipboard.setData(ClipboardData(text: text));
  }

  ReplyTarget _signingReplyTarget(FfiSigningEvent event) {
    final (eventId, author) = switch (event) {
      FfiSigningEvent_Request(:final eventId, :final author) => (eventId, author),
      FfiSigningEvent_Offer(:final eventId, :final author) => (eventId, author),
      FfiSigningEvent_Partial(:final eventId, :final author) => (eventId, author),
    };
    final preview = switch (event) {
      FfiSigningEvent_Request(:final signingDetails) =>
        'Signing request: ${signingDetailsText(signingDetails)}',
      FfiSigningEvent_Offer(:final shareIndex) =>
        'Sign offer — key #$shareIndex',
      FfiSigningEvent_Partial() => 'Signed',
    };
    return ReplyTarget(
      eventId: eventId,
      author: author,
      preview: preview,
      isMe: _myPubkey != null && author.equals(other: _myPubkey!),
    );
  }

  void _startReply(ReplyTarget target) {
    setState(() => _replyingTo = target);
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
          accessStructureId: widget.accessStructureId,
        ),
      ),
    );
  }

  @override
  void dispose() {
    _subscription?.cancel();
    _client?.disconnectChannel(accessStructureId: widget.accessStructureId);
    _messageController.dispose();
    _scrollController.dispose();
    _inputFocusNode.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return Scaffold(
      extendBodyBehindAppBar: true,
      appBar: AppBar(
        backgroundColor: theme.colorScheme.surface.withValues(alpha: 0.4),
        scrolledUnderElevation: 0,
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
      body: Builder(builder: (context) {
        final activeRequest = _activeSigningRequest;
        return Column(
          children: [
            Expanded(
              child: Stack(
                children: [
                  _buildTimeline(theme),
                  if (activeRequest != null)
                    Positioned(
                      top: MediaQuery.of(context).padding.top + kToolbarHeight + 8,
                      left: 0,
                      right: 0,
                      child: _buildTaskCard(activeRequest),
                    ),
                ],
              ),
            ),
            _buildMessageInput(),
          ],
        );
      }),
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

    final hasAttachment = _replyingTo != null || _pendingSignRequest != null;

    return Container(
      color: hasAttachment
          ? theme.colorScheme.surface
          : Colors.transparent,
      child: SafeArea(
        top: false,
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
                          _replyingTo!.preview,
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
      ),
    );
  }
}

class _MessageBubble extends StatefulWidget {
  final ChatMessage message;
  final FfiNostrProfile? profile;
  final bool isHighlighted;
  final ChatMessage? replyToMessage;
  final VoidCallback? onTapQuote;
  final VoidCallback? onTapAvatar;
  final VoidCallback onReply;
  final VoidCallback onRetry;
  final VoidCallback onCopy;

  const _MessageBubble({
    super.key,
    required this.message,
    this.profile,
    this.isHighlighted = false,
    required this.replyToMessage,
    this.onTapQuote,
    this.onTapAvatar,
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

    final baseColor = isFailed
        ? theme.colorScheme.errorContainer
        : isMe
        ? theme.colorScheme.primaryContainer
        : theme.colorScheme.surfaceContainerHighest;

    return AnimatedContainer(
      duration: const Duration(milliseconds: 400),
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
      constraints: BoxConstraints(
        maxWidth: MediaQuery.of(context).size.width * 0.7,
      ),
      decoration: BoxDecoration(
        color: widget.isHighlighted
            ? Color.lerp(baseColor, theme.colorScheme.primary, 0.2)
            : baseColor,
        borderRadius: BorderRadius.circular(16),
        boxShadow: widget.isHighlighted
            ? [BoxShadow(
                color: theme.colorScheme.primary.withValues(alpha: 0.4),
                blurRadius: 10,
                spreadRadius: 1,
              )]
            : [],
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          if (!isMe)
            Text(
              getDisplayName(widget.profile, widget.message.author),
              style: theme.textTheme.labelSmall?.copyWith(
                color: theme.colorScheme.primary,
              ),
            ),
          if (widget.replyToMessage != null) _buildReplyQuote(theme),
          Row(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.end,
            children: [
              Flexible(child: Text(widget.message.content)),
              const SizedBox(width: 8),
              Padding(
                padding: const EdgeInsets.only(bottom: 1),
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Text(
                      _formatTime(widget.message.timestamp),
                      style: theme.textTheme.labelSmall?.copyWith(
                        fontSize: 10,
                        color: theme.colorScheme.onSurfaceVariant,
                      ),
                    ),
                    if (isMe) ...[
                      const SizedBox(width: 3),
                      _buildStatusIndicator(theme),
                    ],
                  ],
                ),
              ),
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
        ? GestureDetector(
            onTap: widget.onTapAvatar,
            child: Padding(
              padding: const EdgeInsets.only(right: 8),
              child: NostrAvatar.small(
                profile: widget.profile,
                pubkey: widget.message.author,
              ),
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
    return GestureDetector(
      onTap: widget.onTapQuote,
      child: Container(
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
            Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                if (replyTo.quoteIcon != null) ...[
                  Icon(replyTo.quoteIcon, size: 12, color: theme.colorScheme.primary),
                  const SizedBox(width: 4),
                ],
                Text(
                  replyTo.isMe ? 'You' : getDisplayName(null, replyTo.author),
                  style: theme.textTheme.labelSmall?.copyWith(
                    color: theme.colorScheme.primary,
                    fontWeight: FontWeight.w600,
                  ),
                ),
              ],
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
      ),
    );
  }

  String _formatTime(DateTime time) {
    return '${time.hour.toString().padLeft(2, '0')}:${time.minute.toString().padLeft(2, '0')}';
  }
}

class _SigningEventLine extends StatelessWidget {
  final FfiNostrProfile? profile;
  final PublicKey author;
  final bool isMe;
  final String label;
  final String messagePill;
  final String? suffix;
  final DateTime timestamp;
  final VoidCallback? onTapPill;
  final VoidCallback? onTapAvatar;

  const _SigningEventLine({
    required this.profile,
    required this.author,
    required this.isMe,
    required this.label,
    required this.messagePill,
    this.suffix,
    required this.timestamp,
    this.onTapPill,
    this.onTapAvatar,
  });

  String _formatTime(DateTime t) =>
      '${t.hour.toString().padLeft(2, '0')}:${t.minute.toString().padLeft(2, '0')}';

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final eventStyle = theme.textTheme.bodySmall?.copyWith(
      fontStyle: FontStyle.italic,
      color: theme.colorScheme.onSurfaceVariant,
    );

    return Align(
      alignment: isMe ? Alignment.centerRight : Alignment.centerLeft,
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 2),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            if (!isMe) ...[
              GestureDetector(
                onTap: onTapAvatar,
                child: NostrAvatar.small(profile: profile, pubkey: author),
              ),
              const SizedBox(width: 8),
            ],
            Flexible(
              child: Text.rich(
                TextSpan(children: [
                  TextSpan(text: '${isMe ? 'you ' : ''}$label ', style: eventStyle),
                  WidgetSpan(
                    alignment: PlaceholderAlignment.middle,
                    child: MouseRegion(
                      cursor: SystemMouseCursors.click,
                      child: GestureDetector(
                        onTap: onTapPill,
                        child: Container(
                          padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                          decoration: BoxDecoration(
                            color: theme.colorScheme.surfaceContainerHighest,
                            borderRadius: BorderRadius.circular(10),
                          ),
                          child: Text(
                            messagePill,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: theme.textTheme.bodySmall?.copyWith(
                              color: theme.colorScheme.primary,
                            ),
                          ),
                        ),
                      ),
                    ),
                  ),
                  if (suffix != null)
                    TextSpan(text: ' $suffix', style: eventStyle),
                ]),
              ),
            ),
            const SizedBox(width: 6),
            Text(
              _formatTime(timestamp),
              style: theme.textTheme.labelSmall?.copyWith(
                fontSize: 10,
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _SigningCompleteCard extends StatelessWidget {
  final String details;
  final VoidCallback? onShowSignature;

  const _SigningCompleteCard({
    required this.details,
    this.onShowSignature,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Align(
      alignment: Alignment.center,
      child: Container(
        margin: const EdgeInsets.symmetric(vertical: 4),
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
        decoration: BoxDecoration(
          color: theme.colorScheme.tertiaryContainer,
          borderRadius: BorderRadius.circular(12),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(Icons.check_circle, size: 18, color: Colors.green),
            const SizedBox(width: 8),
            Flexible(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                mainAxisSize: MainAxisSize.min,
                children: [
                  Text(
                    'Signing Complete',
                    style: theme.textTheme.titleSmall?.copyWith(
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                  Text(
                    details,
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                    style: theme.textTheme.bodySmall?.copyWith(
                      color: theme.colorScheme.onTertiaryContainer,
                    ),
                  ),
                ],
              ),
            ),
            if (onShowSignature != null) ...[
              const SizedBox(width: 8),
              FilledButton(
                onPressed: onShowSignature,
                child: const Text('Show Signature'),
              ),
            ],
          ],
        ),
      ),
    );
  }
}

class _DateDivider extends StatelessWidget {
  final DateTime date;
  const _DateDivider({required this.date});

  String _formatDate(DateTime d) {
    final now = DateTime.now();
    if (d.year == now.year && d.month == now.month && d.day == now.day) {
      return 'Today';
    }
    final yesterday = now.subtract(const Duration(days: 1));
    if (d.year == yesterday.year &&
        d.month == yesterday.month &&
        d.day == yesterday.day) {
      return 'Yesterday';
    }
    final months = [
      'Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun',
      'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec',
    ];
    return '${d.day} ${months[d.month - 1]} ${d.year}';
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 12),
      child: Row(
        children: [
          Expanded(
            child: Divider(color: theme.colorScheme.outline.withValues(alpha: 0.3)),
          ),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 12),
            child: Text(
              _formatDate(date),
              style: theme.textTheme.labelSmall?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
          ),
          Expanded(
            child: Divider(color: theme.colorScheme.outline.withValues(alpha: 0.3)),
          ),
        ],
      ),
    );
  }
}
