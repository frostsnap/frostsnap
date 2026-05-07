import 'dart:async';
import 'package:flutter/material.dart' hide ConnectionState;
import 'package:flutter/services.dart';
import 'package:frostsnap/nostr_chat/group_info_page.dart';
import 'package:frostsnap/nostr_chat/member_detail_sheet.dart';
import 'package:frostsnap/nostr_chat/nostr_profile.dart';
import 'package:frostsnap/nostr_chat/nostr_state.dart';
import 'package:frostsnap/device_selector.dart';
import 'package:frostsnap/nostr_chat/nostr_signing_page.dart';
import 'package:frostsnap/nostr_chat/signing_card.dart';
import 'package:frostsnap/sign_message.dart';
import 'package:frostsnap/snackbar.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/src/rust/api/bitcoin.dart';
import 'package:frostsnap/src/rust/api/nostr.dart';
import 'package:frostsnap/src/rust/api/signing.dart'
    show
        RemoteSignSessionId,
        UnsignedTx,
        SigningDetails_Message,
        SigningDetails_Transaction,
        wireSignTaskBitcoinTransaction,
        wireSignTaskTest;
import 'package:frostsnap/src/rust/lib.dart'
    show WireSignTask, ParticipantBinonces;
import 'package:frostsnap/wallet_send.dart';
import 'package:frostsnap/src/rust/api/super_wallet.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/theme.dart';
import 'package:frostsnap/wallet.dart';
import 'package:frostsnap/wallet_tx_details.dart';
import 'package:dynamic_color/dynamic_color.dart';

enum MessageStatus { pending, sent, failed }

class ChatMessage {
  final EventId messageId;
  final PublicKey author;
  final String content;
  DateTime timestamp;
  final bool isMe;
  final EventId? replyTo;
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
  final EventId eventId;
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
  final SigningEvent event;
  final int? progressCount;
  final int? progressTotal;
  @override
  final DateTime timestamp;
  TimelineSigning(this.event, {this.progressCount, this.progressTotal})
    : timestamp = DateTime.fromMillisecondsSinceEpoch(
        switch (event) {
              SigningEvent_Request(:final timestamp) => timestamp,
              SigningEvent_Offer(:final timestamp) => timestamp,
              SigningEvent_Partial(:final timestamp) => timestamp,
              SigningEvent_Cancel(:final timestamp) => timestamp,
              SigningEvent_RoundConfirmed(:final timestamp) => timestamp,
              SigningEvent_RoundPending(:final timestamp) => timestamp,
              SigningEvent_Rejected(:final timestamp) => timestamp,
            } *
            1000,
      );
}

class TimelineError extends TimelineItem {
  final EventId eventId;
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
  TimelineSigningComplete(this.requestState, {required int completedAtSecs})
    : timestamp = DateTime.fromMillisecondsSinceEpoch(completedAtSecs * 1000);
}

enum TxTimelineKind { needsBroadcast, mempool, confirmed }

class TimelineTransaction extends TimelineItem {
  final Transaction tx;
  final TxTimelineKind kind;
  final SigningRequestState? signingState;
  @override
  final DateTime timestamp;
  TimelineTransaction(
    this.tx, {
    required this.kind,
    required int timestampSecs,
    this.signingState,
  }) : timestamp = DateTime.fromMillisecondsSinceEpoch(timestampSecs * 1000);
}

class ChatPage extends StatefulWidget {
  final AccessStructureRef accessStructureRef;
  final String walletName;
  final ChannelConnectionParams channelParams;

  const ChatPage({
    super.key,
    required this.accessStructureRef,
    required this.walletName,
    required this.channelParams,
  });

  KeyId get keyId => accessStructureRef.keyId;
  AccessStructureId get accessStructureId =>
      accessStructureRef.accessStructureId;

  @override
  State<ChatPage> createState() => _ChatPageState();
}

class _ChatPageState extends State<ChatPage> {
  final TextEditingController _messageController = TextEditingController();
  final ScrollController _scrollController = ScrollController();
  late final FocusNode _inputFocusNode;
  final List<TimelineItem> _timeline = [];
  final Map<EventId, ChatMessage> _messageById = {};
  final Map<String, GlobalKey> _timelineKeys = {};
  String? _highlightedId;
  final Map<EventId, SigningRequestState> _signingRequests = {};
  final Set<String> _seenSigningEventIds = {};
  List<PublicKey> _memberPubkeys = [];
  StreamSubscription<ChannelEvent>? _subscription;
  StreamSubscription<TxState>? _txSubscription;
  final Map<String, TxTimelineKind> _txTimelineState = {};
  NostrClient? _client;
  ConnectionState _connectionState = const ConnectionState.connecting();
  ReplyTarget? _replyingTo;
  ({AccessStructureRef asRef, String testMessage, List<DeviceId> devices})?
  _pendingSignRequest;
  ({AccessStructureRef asRef, UnsignedTx unsignedTx, List<DeviceId> devices})?
  _pendingTxSignRequest;

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
      _subscribeTxStream();
    }
  }

  void _subscribeTxStream() {
    final walletCtx = WalletContext.of(context);
    if (walletCtx == null) return;
    _txSubscription = walletCtx.txStream.listen(_handleTxState);
  }

  TxState? _pendingTxState;

  void _handleTxState(TxState state) {
    if (!mounted) return;
    if (_connectionState is! ConnectionState_Connected) {
      _pendingTxState = state;
      return;
    }
    _applyTxState(state);
  }

  void _applyTxState(TxState state) {
    bool changed = false;
    for (final tx in state.txs) {
      final txid = tx.txid;
      final lastSeen = tx.lastSeen;
      final confirmTime = tx.confirmationTime;

      final currentState = _txTimelineState[txid];

      if (lastSeen != null &&
          currentState != TxTimelineKind.mempool &&
          currentState != TxTimelineKind.confirmed) {
        _removeTxTimelineItem(txid, TxTimelineKind.needsBroadcast);
        _txTimelineState[txid] = TxTimelineKind.mempool;
        _insertTimelineItem(
          TimelineTransaction(
            tx,
            kind: TxTimelineKind.mempool,
            timestampSecs: lastSeen,
          ),
        );
        changed = true;
      }

      if (confirmTime != null &&
          currentState != TxTimelineKind.confirmed &&
          (lastSeen == null || confirmTime.time != lastSeen)) {
        _txTimelineState[txid] = TxTimelineKind.confirmed;
        _insertTimelineItem(
          TimelineTransaction(
            tx,
            kind: TxTimelineKind.confirmed,
            timestampSecs: confirmTime.time,
          ),
        );
        changed = true;
      }
    }
    if (changed) setState(() {});
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
    final stream = _client!.connectToChannel(params: widget.channelParams);
    _subscription = stream.listen(_handleEvent);
  }

  NostrProfile? _getProfile(PublicKey pubkey) {
    return _nostrContext!.getProfile(pubkey);
  }

  void _showMemberProfile(PublicKey pubkey) {
    showModalBottomSheet(
      context: context,
      isScrollControlled: true,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(16)),
      ),
      builder: (_) =>
          MemberDetailSheet(pubkey: pubkey, profile: _getProfile(pubkey)),
    );
  }

  DeviceId? _getMyDevice(SigningRequestState? state) {
    if (state == null || _myPubkey == null) return null;
    final myOffer = state.offers[_myPubkey!.toHex()];
    if (myOffer == null) return null;
    for (final idx in offerShareIndices(binonces: myOffer.binonces)) {
      final device = _deviceForShareIndex(widget.accessStructureRef, idx);
      if (device != null) return device;
    }
    return null;
  }

  DeviceId? _deviceForShareIndex(AccessStructureRef asRef, int shareIndex) {
    final accessStruct = coord.getAccessStructure(asRef: asRef);
    if (accessStruct == null) return null;
    for (final deviceId in accessStruct.devices()) {
      if (accessStruct.getDeviceShortShareIndex(deviceId: deviceId) ==
          shareIndex) {
        return deviceId;
      }
    }
    return null;
  }

  final List<ChannelEvent> _pendingEvents = [];
  bool _batchScheduled = false;

  void _handleEvent(ChannelEvent event) {
    if (!mounted) return;
    _pendingEvents.add(event);
    if (!_batchScheduled) {
      _batchScheduled = true;
      // 🪬 addPostFrameCallback (not Timer.run) is needed here for the
      // scroll-to-bottom in _flushPendingEvents to work reliably.
      // scheduleFrame() ensures the callback fires even when the window is
      // unfocused on Linux desktop.
      WidgetsBinding.instance.addPostFrameCallback(
        (_) => _flushPendingEvents(),
      );
      WidgetsBinding.instance.scheduleFrame();
    }
  }

  void _flushPendingEvents() {
    _batchScheduled = false;
    if (_pendingEvents.isEmpty || !mounted) return;

    final events = List.of(_pendingEvents);
    _pendingEvents.clear();

    final wasAtBottom =
        !_scrollController.hasClients ||
        _scrollController.position.pixels >=
            _scrollController.position.maxScrollExtent - 50;

    setState(() {
      for (final event in events) {
        _processEvent(event);
      }
    });

    if (wasAtBottom) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (!_scrollController.hasClients) return;
        final pos = _scrollController.position;
        final distance = pos.maxScrollExtent - pos.pixels;
        if (distance < 300) {
          _scrollController.animateTo(
            pos.maxScrollExtent,
            duration: const Duration(milliseconds: 200),
            curve: Curves.easeOut,
          );
        } else {
          _jumpToBottom();
        }
      });
    }
  }

  void _processEvent(ChannelEvent event) {
    switch (event) {
      case ChannelEvent_ChatMessage(
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
        final isMe = _myPubkey != null && author == _myPubkey!;
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

      case ChannelEvent_MessageSent(:final messageId):
        final msg = _messageById[messageId];
        if (msg != null) {
          msg.status = MessageStatus.sent;
        }

      case ChannelEvent_MessageSendFailed(:final messageId, :final reason):
        final msg = _messageById[messageId];
        if (msg != null) {
          msg.status = MessageStatus.failed;
          msg.failureReason = reason;
        }

      case ChannelEvent_ConnectionState(:final field0):
        _connectionState = field0;
        if (field0 is ConnectionState_Connected && _pendingTxState != null) {
          _applyTxState(_pendingTxState!);
          _pendingTxState = null;
        }

      case ChannelEvent_GroupMetadata(:final members):
        _memberPubkeys = members.map((m) => m.pubkey).toList();
        _nostrContext!.updateProfilesFromChannel(members);

      case ChannelEvent_Signing(:final event, :final pending):
        // Round decisions (RoundConfirmed, RoundAborted) are local
        // decisions derived from the settling timer; they don't carry a
        // nostr event_id and shouldn't produce chat messages. Route them
        // directly to the SigningRequestState.
        if (event is SigningEvent_RoundConfirmed) {
          final state = _signingRequests[event.requestId];
          if (state != null && _client != null) {
            final binonces = event.subset.expand((e) => e.binonces).toList();
            final sealed = _client!.sealRoundConfirmed(
              accessStructureId: widget.accessStructureId,
              requestId: event.requestId,
              signTask: event.signTask,
              binonces: binonces,
            );
            final subsetEventIds = event.subset.map((e) => e.eventId).toList();
            state.setRoundConfirmed(sealed, subsetEventIds);
          }
          break;
        }
        if (event is SigningEvent_RoundPending) {
          // Provisional snapshot: the settling timer fired but threshold
          // hasn't been met yet. Round is still collecting. UI can render
          // "your offer is likely accepted" to authors in `observed`.
          // Does NOT cancel nonce reservations — offerers stay reserved
          // until the round confirms or the requester cancels.
          final state = _signingRequests[event.requestId];
          if (state != null) {
            state.setRoundPending(event.observed, event.threshold);
          }
          break;
        }
        if (event is SigningEvent_Rejected) {
          // Validation errors (duplicate share_index, late offer, etc).
          // Log-only for now — surface later if UX requires.
          break;
        }

        final eventId = switch (event) {
          SigningEvent_Request(:final eventId) => eventId,
          SigningEvent_Offer(:final eventId) => eventId,
          SigningEvent_Partial(:final eventId) => eventId,
          SigningEvent_Cancel(:final eventId) => eventId,
          SigningEvent_RoundConfirmed() ||
          SigningEvent_RoundPending() ||
          SigningEvent_Rejected() => throw StateError('handled above'),
        };
        final idHex = eventId.toHex();
        if (!_seenSigningEventIds.add(idHex)) break;

        final (author, timestamp, content) = switch (event) {
          SigningEvent_Request(
            :final author,
            :final timestamp,
            :final signTask,
            :final message,
          ) =>
            (
              author,
              timestamp,
              message.isNotEmpty
                  ? message
                  : signingDetailsText(signingDetails(signTask: signTask)),
            ),
          SigningEvent_Offer(:final author, :final timestamp) => (
            author,
            timestamp,
            'offered to sign',
          ),
          SigningEvent_Partial(:final author, :final timestamp) => (
            author,
            timestamp,
            'signed',
          ),
          SigningEvent_Cancel(:final author, :final timestamp) => (
            author,
            timestamp,
            'cancelled the signing request',
          ),
          SigningEvent_RoundConfirmed() ||
          SigningEvent_RoundPending() ||
          SigningEvent_Rejected() => throw StateError('handled above'),
        };
        final isMe = _myPubkey != null && author == _myPubkey!;
        _messageById[eventId] = ChatMessage(
          messageId: eventId,
          author: author,
          content: content,
          timestamp: DateTime.fromMillisecondsSinceEpoch(timestamp * 1000),
          isMe: isMe,
          quoteIcon: Icons.draw,
          status: pending ? MessageStatus.pending : MessageStatus.sent,
        );

        switch (event) {
          case SigningEvent_Request():
            _signingRequests[event.eventId] = SigningRequestState(event);
            for (final item in _timeline) {
              if (item is! TimelineSigning) continue;
              final itemEvent = item.event;
              switch (itemEvent) {
                case SigningEvent_Offer(:final requestId, :final author):
                  if (requestId == event.eventId) {
                    _signingRequests[event.eventId]!.addOffer(
                      author.toHex(),
                      itemEvent,
                    );
                  }
                case SigningEvent_Partial(:final requestId, :final author):
                  if (requestId == event.eventId) {
                    _signingRequests[event.eventId]!.addPartial(
                      author.toHex(),
                      itemEvent,
                    );
                  }
                default:
                  break;
              }
            }
            _insertTimelineItem(TimelineSigning(event));
          case SigningEvent_Offer():
            final state = _signingRequests[event.requestId];
            if (state != null) {
              state.addOffer(event.author.toHex(), event);
            }
            final threshold = _getThreshold(event.requestId);
            _insertTimelineItem(
              TimelineSigning(
                event,
                progressCount: state?.offers.length,
                progressTotal: threshold > 0 ? threshold : null,
              ),
            );
          case SigningEvent_Partial():
            final state = _signingRequests[event.requestId];
            if (state != null) {
              final authorHex = event.author.toHex();
              final offer = state.offers[authorHex];
              if (offer != null && (event.timestamp - offer.timestamp) < 300) {
                _timeline.removeWhere(
                  (item) =>
                      item is TimelineSigning &&
                      item.event is SigningEvent_Offer &&
                      (item.event as SigningEvent_Offer).author.toHex() ==
                          authorHex &&
                      (item.event as SigningEvent_Offer).requestId.toHex() ==
                          event.requestId.toHex(),
                );
              }
              state.addPartial(authorHex, event);
              final threshold = _getThreshold(event.requestId);
              _insertTimelineItem(
                TimelineSigning(
                  event,
                  progressCount: state.partials.length,
                  progressTotal: threshold > 0 ? threshold : null,
                ),
              );
              if (state.partials.length >= threshold) {
                final details = signingDetails(
                  signTask: state.request.signTask,
                );
                if (details is SigningDetails_Transaction) {
                  final txid = details.transaction.txid;
                  final existing = _txTimelineState[txid];
                  if (existing == null) {
                    _txTimelineState[txid] = TxTimelineKind.needsBroadcast;
                    _insertTimelineItem(
                      TimelineTransaction(
                        details.transaction,
                        kind: TxTimelineKind.needsBroadcast,
                        timestampSecs: event.timestamp,
                        signingState: state,
                      ),
                    );
                  }
                } else {
                  _insertTimelineItem(
                    TimelineSigningComplete(
                      state,
                      completedAtSecs: event.timestamp,
                    ),
                  );
                }
              }
            }
          case SigningEvent_Cancel():
            final state = _signingRequests[event.requestId];
            if (state != null) {
              state.cancelled = true;
            }
            _insertTimelineItem(TimelineSigning(event));
            coord.cancelRemoteSignSession(
              id: RemoteSignSessionId(field0: event.requestId.field0),
            );
          case SigningEvent_RoundConfirmed() ||
              SigningEvent_RoundPending() ||
              SigningEvent_Rejected():
            // handled above
            break;
        }

      case ChannelEvent_Error(
        :final eventId,
        :final author,
        :final timestamp,
        :final reason,
      ):
        _insertTimelineItem(
          TimelineError(
            eventId: eventId,
            author: author,
            timestamp: timestamp,
            reason: reason,
          ),
        );
    }
  }

  int _getThreshold(EventId requestId) {
    final state = _signingRequests[requestId];
    if (state == null) return 0;
    final accessStruct = coord.getAccessStructure(
      asRef: widget.accessStructureRef,
    );
    return accessStruct?.threshold() ?? 0;
  }

  void _removeTxTimelineItem(String txid, TxTimelineKind kind) {
    _timeline.removeWhere(
      (item) =>
          item is TimelineTransaction &&
          item.kind == kind &&
          item.tx.txid == txid,
    );
  }

  void _jumpToBottom() {
    if (!_scrollController.hasClients) return;
    final pos = _scrollController.position;
    _scrollController.jumpTo(pos.maxScrollExtent);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!_scrollController.hasClients) return;
      final pos = _scrollController.position;
      if (pos.pixels < pos.maxScrollExtent - 1) {
        _jumpToBottom();
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

  SigningRequestState? get _activeSigningRequest {
    final frostKey = coord.getFrostKey(keyId: widget.keyId);
    final threshold = frostKey?.accessStructures()[0].threshold() ?? 0;
    SigningRequestState? best;
    for (final state in _signingRequests.values) {
      if (state.cancelled) continue;
      if (state.partials.length >= threshold) continue;
      if (state.sealedData == null) continue;
      if (best == null || state.timestamp.isAfter(best.timestamp)) {
        best = state;
      }
    }
    return best;
  }

  Widget _buildTimeline(ThemeData theme, {bool hasTaskCard = false}) {
    if (_timeline.isEmpty) {
      return Center(
        child: _connectionState is ConnectionState_Connected
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
    final topPadding = hasTaskCard
        ? 8.0
        : MediaQuery.of(context).padding.top + kToolbarHeight + 8;
    return ListView.builder(
      controller: _scrollController,
      padding: EdgeInsets.fromLTRB(16, topPadding, 16, 16),
      itemCount: _timeline.length,
      itemBuilder: (context, index) {
        final item = _timeline[index];
        final showDateDivider =
            index == 0 ||
            !_isSameDay(item.timestamp, _timeline[index - 1].timestamp);
        final child = switch (item) {
          TimelineChat(:final message) => _MessageBubble(
            key: _timelineKeys.putIfAbsent(
              message.messageId.toHex(),
              () => GlobalKey(),
            ),
            message: message,
            profile: _getProfile(message.author),
            isHighlighted: _highlightedId == message.messageId.toHex(),
            replyToMessage: message.replyTo != null
                ? _messageById[message.replyTo]
                : null,
            onTapQuote: message.replyTo != null
                ? () => _scrollToAndHighlight(message.replyTo!)
                : null,
            onTapAvatar: message.isMe
                ? null
                : () => _showMemberProfile(message.author),
            onReply: () => _startReply(
              ReplyTarget(
                eventId: message.messageId,
                author: message.author,
                preview: message.content,
                isMe: message.isMe,
              ),
            ),
            onRetry: () => _retryMessage(message),
            onCopy: () => _copyMessage(message),
          ),
          TimelineSigning(
            event: final event,
            :final progressCount,
            :final progressTotal,
          ) =>
            switch (event) {
              SigningEvent_Request() => _buildRequestCard(event),
              SigningEvent_Offer() => _buildOfferCard(
                event,
                progressCount,
                progressTotal,
              ),
              SigningEvent_Partial() => _buildPartialCard(
                event,
                progressCount,
                progressTotal,
              ),
              SigningEvent_Cancel() => _buildCancelCard(event),
              SigningEvent_RoundConfirmed() ||
              SigningEvent_RoundPending() ||
              SigningEvent_Rejected() => const SizedBox.shrink(),
            },
          TimelineSigningComplete(:final requestState) => _SigningCompleteCard(
            details: signingDetailsText(
              signingDetails(signTask: requestState.request.signTask),
              walletCtx: WalletContext.of(context),
            ),
            onShowSignature: () => _completeAndShowSignature(requestState),
          ),
          TimelineTransaction(
            :final tx,
            :final kind,
            :final timestamp,
            :final signingState,
          ) =>
            switch (kind) {
              TxTimelineKind.needsBroadcast => _TransactionCard(
                key: _timelineKeys.putIfAbsent(tx.txid, () => GlobalKey()),
                tx: tx,
                timestamp: timestamp,
                onTap: () => _showTxDetails(tx),
                onBroadcast: signingState != null
                    ? () => _broadcastTransaction(signingState)
                    : null,
              ),
              TxTimelineKind.mempool => _TransactionCard(
                key: _timelineKeys.putIfAbsent(tx.txid, () => GlobalKey()),
                tx: tx,
                timestamp: timestamp,
                isHighlighted: _highlightedId == tx.txid,
                onTap: () => _showTxDetails(tx),
              ),
              TxTimelineKind.confirmed => _TxConfirmedLine(
                tx: tx,
                timestamp: timestamp,
                onTapPill: () => _scrollToByStringId(tx.txid),
              ),
            },
          TimelineError() => SigningErrorCard(
            text: item.reason,
            author: item.author,
            profile: _getProfile(item.author),
            isMe: _myPubkey != null && item.author == _myPubkey!,
            onCopy: () => Clipboard.setData(ClipboardData(text: item.reason)),
            onReply: () => _startReply(
              ReplyTarget(
                eventId: item.eventId,
                author: item.author,
                preview: 'Error: ${item.reason}',
                isMe: _myPubkey != null && item.author == _myPubkey!,
              ),
            ),
            onTapAvatar: _myPubkey != null && item.author == _myPubkey!
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

  void _scrollToAndHighlight(EventId eventId) {
    _scrollToByStringId(eventId.toHex());
  }

  void _scrollToByStringId(String id) {
    final key = _timelineKeys[id];
    if (key?.currentContext == null) return;
    Scrollable.ensureVisible(
      key!.currentContext!,
      duration: const Duration(milliseconds: 300),
      curve: Curves.easeOut,
      alignment: 0.3,
    );
    setState(() => _highlightedId = id);
    Future.delayed(const Duration(milliseconds: 1200), () {
      if (mounted) setState(() => _highlightedId = null);
    });
  }

  Widget _buildRequestCard(SigningEvent_Request request) {
    final state = _signingRequests[request.eventId];
    if (state == null) {
      return SigningErrorCard(text: 'Unknown signing request');
    }
    final myPubkey = _myPubkey;
    final iOffered =
        myPubkey != null && state.offers.containsKey(myPubkey.toHex());
    final accessStruct = coord.getAccessStructure(
      asRef: widget.accessStructureRef,
    );
    final threshold = accessStruct?.threshold() ?? 0;
    final reqIsMe = myPubkey != null && state.request.author == myPubkey;
    final idHex = request.eventId.toHex();
    final key = _timelineKeys.putIfAbsent(idHex, () => GlobalKey());
    final isComplete = state.partials.length >= threshold;
    return SigningRequestCard(
      key: key,
      state: state,
      threshold: threshold,
      isMe: reqIsMe,
      iOffered: iOffered,
      isHighlighted: _highlightedId == idHex,
      sendStatus: _messageById[request.eventId]?.status,
      profile: _getProfile(state.request.author),
      getDisplayName: (pubkey) => getDisplayName(_getProfile(pubkey), pubkey),
      onOfferToSign: iOffered ? null : () => _onOfferToSign(state),
      onCancel: reqIsMe && !isComplete && !state.cancelled
          ? () => _onCancelRequest(state)
          : null,
      onTap: state.sealedData != null
          ? () => _showSigningProgress(state)
          : null,
      onCopy: () => _copySigningText(request),
      onReply: () => _startReply(_signingReplyTarget(request)),
      onTapAvatar: reqIsMe
          ? null
          : () => _showMemberProfile(state.request.author),
    );
  }

  void _showSigningProgress(SigningRequestState state) {
    _openSigningPage(state);
  }

  Widget _signingPillWidget(EventId requestId) {
    final state = _signingRequests[requestId];
    if (state == null) return const Text('?');
    final details = signingDetails(signTask: state.request.signTask);
    if (details is SigningDetails_Transaction) {
      final walletCtx = WalletContext.of(context);
      if (walletCtx != null) {
        final tx = details.transaction;
        final chainTipHeight = walletCtx.superWallet.height();
        final txDetails = TxDetailsModel(
          tx: tx,
          chainTipHeight: chainTipHeight,
          now: DateTime.now(),
        );
        final isSend = txDetails.isSend;
        final accentColor = isSend
            ? Colors.redAccent.harmonizeWith(
                Theme.of(context).colorScheme.primary,
              )
            : Colors.green.harmonizeWith(Theme.of(context).colorScheme.primary);
        return Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              isSend ? Icons.north_east : Icons.south_east,
              size: 14,
              color: accentColor,
            ),
            const SizedBox(width: 2),
            SatoshiText(
              value: txDetails.netValue,
              showSign: true,
              hideLeadingWhitespace: true,
              style: Theme.of(context).textTheme.bodySmall,
            ),
          ],
        );
      }
    }
    final text = signingDetailsText(details);
    final preview = text.length > 30 ? '${text.substring(0, 30)}...' : text;
    return Text(
      preview,
      maxLines: 1,
      overflow: TextOverflow.ellipsis,
      style: Theme.of(context).textTheme.bodySmall?.copyWith(
        color: Theme.of(context).colorScheme.primary,
      ),
    );
  }

  Widget _buildOfferCard(
    SigningEvent_Offer offer,
    int? progressCount,
    int? progressTotal,
  ) {
    final isMe = _myPubkey != null && offer.author == _myPubkey!;
    return _SigningEventLine(
      profile: _getProfile(offer.author),
      author: offer.author,
      isMe: isMe,
      label: 'offered to sign',
      messagePill: _signingPillWidget(offer.requestId),
      suffix: () {
        final indices = offerShareIndices(binonces: offer.binonces);
        return 'with ${indices.length == 1 ? 'key #${indices.first}' : 'keys ${indices.map((i) => '#$i').join(', ')}'}';
      }(),
      timestamp: DateTime.fromMillisecondsSinceEpoch(offer.timestamp * 1000),
      onTapPill: () => _scrollToAndHighlight(offer.requestId),
      onTapAvatar: isMe ? null : () => _showMemberProfile(offer.author),
      progress: progressTotal != null && progressTotal > 0
          ? progressCount! / progressTotal
          : null,
      progressTooltip: '$progressCount out of $progressTotal needed',
    );
  }

  Widget _buildPartialCard(
    SigningEvent_Partial partial,
    int? progressCount,
    int? progressTotal,
  ) {
    final isMe = _myPubkey != null && partial.author == _myPubkey!;
    return _SigningEventLine(
      profile: _getProfile(partial.author),
      author: partial.author,
      isMe: isMe,
      label: 'signed',
      messagePill: _signingPillWidget(partial.requestId),
      timestamp: DateTime.fromMillisecondsSinceEpoch(partial.timestamp * 1000),
      onTapPill: () => _scrollToAndHighlight(partial.requestId),
      onTapAvatar: isMe ? null : () => _showMemberProfile(partial.author),
      progress: progressTotal != null && progressTotal > 0
          ? progressCount! / progressTotal
          : null,
      progressTooltip: '$progressCount of $progressTotal signed',
      progressColor: Colors.green,
    );
  }

  Widget _buildCancelCard(SigningEvent_Cancel cancel) {
    final isMe = _myPubkey != null && cancel.author == _myPubkey!;
    return _SigningEventLine(
      profile: _getProfile(cancel.author),
      author: cancel.author,
      isMe: isMe,
      label: 'cancelled the signing request',
      messagePill: _signingPillWidget(cancel.requestId),
      timestamp: DateTime.fromMillisecondsSinceEpoch(cancel.timestamp * 1000),
      onTapPill: () => _scrollToAndHighlight(cancel.requestId),
      onTapAvatar: isMe ? null : () => _showMemberProfile(cancel.author),
    );
  }

  Future<void> _onCancelRequest(SigningRequestState state) async {
    if (_client == null) return;
    setState(() => state.cancelled = true);
    final nsec = _nostrContext!.nostrSettings.getNsec();
    try {
      await _client!.sendSignCancel(
        accessStructureId: widget.accessStructureId,
        nsec: nsec,
        requestId: state.request.eventId,
      );
    } catch (e) {
      if (mounted) {
        setState(() => state.cancelled = false);
        showErrorSnackbar(context, 'Failed to cancel signing request: $e');
      }
    }
  }

  Future<void> _onOfferToSign(SigningRequestState state) async {
    final frostKey = coord.getFrostKey(keyId: widget.keyId);
    if (frostKey == null || _client == null) return;
    final walletCtx = WalletContext.of(context);
    final threshold = frostKey.accessStructures()[0].threshold();

    await showBottomSheetOrDialog(
      context,
      title: Text(
        state.offers.length + 1 >= threshold ? 'Sign' : 'Offer to Sign',
      ),
      builder: (context, scrollController) {
        if (walletCtx != null) {
          return walletCtx.wrap(
            Builder(
              builder: (ctx) => _OfferSignSheet(
                state: state,
                accessStructureRef: widget.accessStructureRef,
                client: _client!,
                nostrContext: _nostrContext!,
                threshold: threshold,
                getProfile: _getProfile,
                deviceForShareIndex: _deviceForShareIndex,
                scrollController: scrollController,
              ),
            ),
          );
        }
        return _OfferSignSheet(
          state: state,
          accessStructureRef: widget.accessStructureRef,
          client: _client!,
          nostrContext: _nostrContext!,
          threshold: threshold,
          getProfile: _getProfile,
          deviceForShareIndex: _deviceForShareIndex,
          scrollController: scrollController,
        );
      },
    );
  }

  void _openSigningPage(SigningRequestState state) {
    final walletCtx = WalletContext.of(context);
    if (walletCtx == null) return;
    final details = signingDetails(signTask: state.request.signTask);
    if (details is! SigningDetails_Transaction) return;
    final tx = details.transaction;
    final txDetails = TxDetailsModel(
      tx: tx,
      chainTipHeight: walletCtx.superWallet.height(),
      now: DateTime.now(),
    );
    final nsec = _nostrContext!.nostrSettings.getNsec();
    showBottomSheetOrDialog(
      context,
      title: const Text('Signing'),
      builder: (ctx, scrollController) => walletCtx.wrap(
        NostrSigningPage(
          scrollController: scrollController,
          txDetails: txDetails,
          signingState: state,
          threshold: _getThreshold(state.request.eventId),
          getProfile: _getProfile,
          client: _client!,
          accessStructureRef: widget.accessStructureRef,
          nsec: nsec,
          myPubkey: _myPubkey,
        ),
      ),
    );
  }

  Future<void> _completeAndShowSignature(SigningRequestState state) async {
    final sealed = state.sealedData;
    if (sealed == null) return;

    try {
      final signatures = sealed.combineSignatures(
        allShares: state.partials.values.map((p) => p.signatureShares).toList(),
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

  void _showTxDetails(Transaction tx) {
    final walletCtx = WalletContext.of(context);
    if (walletCtx == null) return;
    final fsCtx = FrostsnapContext.of(context);
    if (fsCtx == null) return;
    final chainTipHeight = walletCtx.superWallet.height();
    final txDetails = TxDetailsModel(
      tx: tx,
      chainTipHeight: chainTipHeight,
      now: DateTime.now(),
    );
    showBottomSheetOrDialog(
      context,
      title: const Text('Transaction Details'),
      builder: (context, scrollController) => walletCtx.wrap(
        TxDetailsPage(
          scrollController: scrollController,
          txStates: walletCtx.txStream,
          txDetails: txDetails,
          psbtMan: fsCtx.psbtManager,
        ),
      ),
    );
  }

  Future<void> _broadcastTransaction(SigningRequestState state) async {
    final sealed = state.sealedData;
    if (sealed == null) return;
    final walletCtx = WalletContext.of(context);
    if (walletCtx == null) return;
    final details = signingDetails(signTask: state.request.signTask);
    if (details is! SigningDetails_Transaction) return;

    try {
      final signatures = sealed.combineSignatures(
        allShares: state.partials.values.map((p) => p.signatureShares).toList(),
      );
      final signedTx = await details.transaction.withSignatures(
        signatures: signatures,
      );
      if (!mounted) return;
      await walletCtx.superWallet.broadcastTx(
        masterAppkey: walletCtx.masterAppkey,
        tx: signedTx,
      );
      if (mounted) {
        ScaffoldMessenger.of(
          context,
        ).showSnackBar(const SnackBar(content: Text('Transaction broadcast!')));
      }
    } catch (e) {
      if (mounted) {
        showErrorSnackbar(context, 'Broadcast failed: $e');
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
            const SizedBox(height: 8),
            ListTile(
              shape: RoundedRectangleBorder(
                borderRadius: BorderRadius.circular(24),
              ),
              tileColor: theme.colorScheme.surfaceContainer,
              leading: const Icon(Icons.currency_bitcoin),
              title: const Text('Send Bitcoin'),
              onTap: () {
                Navigator.pop(context);
                _proposeSendBitcoin();
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
            decoration: const InputDecoration(labelText: 'Message to sign'),
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

    setState(() {
      _pendingSignRequest = (
        asRef: widget.accessStructureRef,
        testMessage: testMessage.trim(),
        devices: <DeviceId>[],
      );
    });
    _inputFocusNode.requestFocus();
  }

  Future<void> _proposeSendBitcoin() async {
    final walletCtx = WalletContext.of(context);
    if (walletCtx == null) return;
    final asRef = widget.accessStructureRef;

    await showBottomSheetOrDialog(
      context,
      title: const Text('Send Bitcoin'),
      builder: (context, scrollController) => walletCtx.wrap(
        WalletSendPage(
          superWallet: walletCtx.superWallet,
          masterAppkey: walletCtx.masterAppkey,
          scrollController: scrollController,
          remoteSigning: true,
          onTxReady: (unsignedTx, selectedDevices) {
            setState(() {
              _pendingTxSignRequest = (
                asRef: asRef,
                unsignedTx: unsignedTx,
                devices: selectedDevices,
              );
            });
            _inputFocusNode.requestFocus();
          },
        ),
      ),
    );
  }

  Future<void> _sendMessage() async {
    final content = _messageController.text.trim();
    final pending = _pendingSignRequest;
    final pendingTx = _pendingTxSignRequest;
    if (content.isEmpty && pending == null && pendingTx == null) return;
    if (_client == null) return;

    final nsec = _nostrContext!.nostrSettings.getNsec();
    final replyToId = _replyingTo?.eventId;
    _messageController.clear();
    setState(() {
      _replyingTo = null;
      _pendingSignRequest = null;
      _pendingTxSignRequest = null;
    });

    try {
      if (pendingTx != null) {
        final requestId = await _client!.sendSignRequest(
          accessStructureRef: pendingTx.asRef,
          nsec: nsec,
          unsignedTx: pendingTx.unsignedTx,
          message: content,
        );
        if (pendingTx.devices.isNotEmpty) {
          final signTask = wireSignTaskBitcoinTransaction(
            unsignedTx: pendingTx.unsignedTx,
          );
          await _sendOfferForRequest(
            requestId,
            pendingTx.asRef,
            pendingTx.devices,
            signTask,
          );
        }
      } else if (pending != null) {
        final requestId = await _client!.sendTestSignRequest(
          accessStructureRef: pending.asRef,
          nsec: nsec,
          testMessage: pending.testMessage,
          message: content,
        );
        if (pending.devices.isNotEmpty) {
          final signTask = wireSignTaskTest(message: pending.testMessage);
          await _sendOfferForRequest(
            requestId,
            pending.asRef,
            pending.devices,
            signTask,
          );
        }
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

  Future<void> _sendOfferForRequest(
    EventId requestId,
    AccessStructureRef asRef,
    List<DeviceId> devices,
    WireSignTask signTask,
  ) async {
    final nsec = _nostrContext!.nostrSettings.getNsec();
    final reservationId = RemoteSignSessionId(field0: requestId.field0);
    final allBinonces = <ParticipantBinonces>[];
    for (final device in devices) {
      final binonces = await coord.reserveNonces(
        id: reservationId,
        accessStructureRef: asRef,
        signTask: signTask,
        deviceId: device,
      );
      allBinonces.add(binonces);
    }
    await _client!.sendSignOffer(
      accessStructureId: widget.accessStructureId,
      nsec: nsec,
      requestId: requestId,
      binonces: allBinonces,
    );
  }

  Future<void> _retryMessage(ChatMessage message) async {
    if (message.status != MessageStatus.failed || _client == null) return;

    final nsec = _nostrContext!.nostrSettings.getNsec();
    setState(() {
      _timeline.removeWhere(
        (item) => item is TimelineChat && item.message == message,
      );
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
    if (_myPubkey != null && author == _myPubkey!) return 'You';
    return getDisplayName(_getProfile(author), author);
  }

  void _copySigningText(SigningEvent event) {
    final text = switch (event) {
      SigningEvent_Request(:final signTask, :final message) =>
        '${signingDetailsText(signingDetails(signTask: signTask))}${message.isNotEmpty ? '\n$message' : ''}',
      SigningEvent_Offer(:final binonces, :final author) =>
        '${_displayName(author)} offered to sign with ${offerShareIndices(binonces: binonces).map((i) => 'key #$i').join(', ')}',
      SigningEvent_Partial(:final author) => '${_displayName(author)} signed',
      SigningEvent_Cancel(:final author) =>
        '${_displayName(author)} cancelled the signing request',
      SigningEvent_RoundConfirmed() ||
      SigningEvent_RoundPending() ||
      SigningEvent_Rejected() => throw StateError(
        'round decisions have no chat copy text',
      ),
    };
    Clipboard.setData(ClipboardData(text: text));
  }

  ReplyTarget _signingReplyTarget(SigningEvent event) {
    final (eventId, author) = switch (event) {
      SigningEvent_Request(:final eventId, :final author) => (eventId, author),
      SigningEvent_Offer(:final eventId, :final author) => (eventId, author),
      SigningEvent_Partial(:final eventId, :final author) => (eventId, author),
      SigningEvent_Cancel(:final eventId, :final author) => (eventId, author),
      SigningEvent_RoundConfirmed() ||
      SigningEvent_RoundPending() ||
      SigningEvent_Rejected() => throw StateError(
        'round decisions cannot be replied to',
      ),
    };
    final preview = switch (event) {
      SigningEvent_Request(:final signTask) =>
        'Signing request: ${signingDetailsText(signingDetails(signTask: signTask))}',
      SigningEvent_Offer(:final binonces) =>
        'Sign offer — ${offerShareIndices(binonces: binonces).map((i) => 'key #$i').join(', ')}',
      SigningEvent_Partial() => 'Signed',
      SigningEvent_Cancel() => 'Cancelled',
      SigningEvent_RoundConfirmed() ||
      SigningEvent_RoundPending() ||
      SigningEvent_Rejected() => throw StateError(
        'round decisions cannot be replied to',
      ),
    };
    return ReplyTarget(
      eventId: eventId,
      author: author,
      preview: preview,
      isMe: _myPubkey != null && author == _myPubkey!,
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
    _txSubscription?.cancel();
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
      body: Column(
        children: [
          Expanded(child: _buildTimeline(theme)),
          _buildSigningBanner(theme),
          _buildMessageInput(),
        ],
      ),
    );
  }

  Widget _buildConnectionIndicator() {
    final (color, tooltip) = switch (_connectionState) {
      ConnectionState_Connecting() => (Colors.orange, 'Connecting...'),
      ConnectionState_Connected() => (Colors.green, 'Connected'),
      ConnectionState_Disconnected(:final reason) => (
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

  Widget _buildSigningBanner(ThemeData theme) {
    final activeRequest = _activeSigningRequest;
    if (activeRequest == null) return const SizedBox.shrink();

    final myPubkey = _myPubkey;
    final isRequester =
        myPubkey != null && activeRequest.request.author == myPubkey;
    final alreadySigned =
        myPubkey != null &&
        activeRequest.partials.containsKey(myPubkey.toHex());
    final sealed = activeRequest.sealedData;
    final myDevice = _getMyDevice(activeRequest);
    final canSign = sealed != null && myDevice != null && !alreadySigned;

    if (!canSign && !isRequester) return const SizedBox.shrink();

    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 6),
      decoration: BoxDecoration(color: theme.colorScheme.primaryContainer),
      child: Row(
        children: [
          _signingPillWidget(activeRequest.request.eventId),
          const Spacer(),
          if (isRequester)
            OutlinedButton(
              onPressed: () => _onCancelRequest(activeRequest),
              style: OutlinedButton.styleFrom(
                padding: const EdgeInsets.symmetric(horizontal: 12),
                minimumSize: const Size(0, 36),
              ),
              child: const Text('Cancel'),
            ),
          if (canSign) ...[
            if (isRequester) const SizedBox(width: 8),
            FilledButton.icon(
              onPressed: () => _openSigningPage(activeRequest),
              icon: const Icon(Icons.draw, size: 16),
              label: const Text('Sign'),
              style: FilledButton.styleFrom(
                padding: const EdgeInsets.symmetric(horizontal: 12),
                minimumSize: const Size(0, 36),
              ),
            ),
          ],
        ],
      ),
    );
  }

  Widget _buildMessageInput() {
    final theme = Theme.of(context);
    final isConnected = _connectionState is ConnectionState_Connected;

    final hasAttachment =
        _replyingTo != null ||
        _pendingSignRequest != null ||
        _pendingTxSignRequest != null;

    return Container(
      color: hasAttachment ? theme.colorScheme.surface : Colors.transparent,
      child: SafeArea(
        top: false,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            if (_replyingTo != null)
              Container(
                padding: const EdgeInsets.symmetric(
                  horizontal: 16,
                  vertical: 8,
                ),
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
                padding: const EdgeInsets.symmetric(
                  horizontal: 16,
                  vertical: 8,
                ),
                decoration: BoxDecoration(
                  color: theme.colorScheme.secondaryContainer.withValues(
                    alpha: 0.5,
                  ),
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
                      onPressed: () =>
                          setState(() => _pendingSignRequest = null),
                      padding: EdgeInsets.zero,
                      constraints: const BoxConstraints(),
                    ),
                  ],
                ),
              ),
            if (_pendingTxSignRequest != null)
              Container(
                padding: const EdgeInsets.symmetric(
                  horizontal: 16,
                  vertical: 8,
                ),
                decoration: BoxDecoration(
                  color: theme.colorScheme.secondaryContainer.withValues(
                    alpha: 0.5,
                  ),
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
                            'Send Transaction',
                            style: theme.textTheme.labelSmall?.copyWith(
                              color: theme.colorScheme.secondary,
                              fontWeight: FontWeight.w600,
                            ),
                          ),
                          Text(
                            _pendingTxSignRequest!.devices.isNotEmpty
                                ? 'Signing with ${_pendingTxSignRequest!.devices.map((d) => coord.getDeviceName(id: d) ?? "device").join(", ")}'
                                : _pendingTxSignRequest!.unsignedTx.txid(),
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
                      onPressed: () =>
                          setState(() => _pendingTxSignRequest = null),
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
  final NostrProfile? profile;
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

    return ConstrainedBox(
      constraints: BoxConstraints(
        maxWidth: MediaQuery.of(context).size.width * 0.7,
      ),
      child: AnimatedContainer(
        duration: const Duration(milliseconds: 400),
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
        decoration: BoxDecoration(
          color: widget.isHighlighted
              ? Color.lerp(baseColor, theme.colorScheme.primary, 0.2)
              : baseColor,
          borderRadius: BorderRadius.circular(16),
          boxShadow: widget.isHighlighted
              ? [
                  BoxShadow(
                    color: theme.colorScheme.primary.withValues(alpha: 0.4),
                    blurRadius: 10,
                    spreadRadius: 1,
                  ),
                ]
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
                  Icon(
                    replyTo.quoteIcon,
                    size: 12,
                    color: theme.colorScheme.primary,
                  ),
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
  final NostrProfile? profile;
  final PublicKey author;
  final bool isMe;
  final String label;
  final Widget messagePill;
  final String? suffix;
  final DateTime timestamp;
  final VoidCallback? onTapPill;
  final VoidCallback? onTapAvatar;
  final double? progress;
  final String? progressTooltip;
  final Color? progressColor;

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
    this.progress,
    this.progressTooltip,
    this.progressColor,
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
                TextSpan(
                  children: [
                    TextSpan(
                      text: '${isMe ? 'you ' : ''}$label ',
                      style: eventStyle,
                    ),
                    WidgetSpan(
                      alignment: PlaceholderAlignment.middle,
                      child: MouseRegion(
                        cursor: SystemMouseCursors.click,
                        child: GestureDetector(
                          onTap: onTapPill,
                          child: Container(
                            padding: const EdgeInsets.symmetric(
                              horizontal: 6,
                              vertical: 2,
                            ),
                            decoration: BoxDecoration(
                              color: theme.colorScheme.surfaceContainerHighest,
                              borderRadius: BorderRadius.circular(10),
                            ),
                            child: messagePill,
                          ),
                        ),
                      ),
                    ),
                    if (suffix != null)
                      TextSpan(text: ' $suffix', style: eventStyle),
                  ],
                ),
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
            if (progress != null) ...[
              const SizedBox(width: 6),
              Tooltip(
                message: progressTooltip ?? '',
                child: SizedBox(
                  width: 16,
                  height: 16,
                  child: CircularProgressIndicator(
                    value: progress!.clamp(0.0, 1.0),
                    strokeWidth: 2.5,
                    backgroundColor: theme.colorScheme.outlineVariant,
                    color: progressColor ?? theme.colorScheme.primary,
                  ),
                ),
              ),
            ],
          ],
        ),
      ),
    );
  }
}

class _SigningCompleteCard extends StatelessWidget {
  final String details;
  final VoidCallback? onShowSignature;

  const _SigningCompleteCard({required this.details, this.onShowSignature});

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

class _TransactionCard extends StatefulWidget {
  final Transaction tx;
  final DateTime timestamp;
  final bool isHighlighted;
  final VoidCallback onTap;
  final Future<void> Function()? onBroadcast;

  const _TransactionCard({
    super.key,
    required this.tx,
    required this.timestamp,
    this.isHighlighted = false,
    required this.onTap,
    this.onBroadcast,
  });

  @override
  State<_TransactionCard> createState() => _TransactionCardState();
}

class _TransactionCardState extends State<_TransactionCard> {
  bool _broadcasting = false;

  static String _formatTime(DateTime t) =>
      '${t.hour.toString().padLeft(2, '0')}:${t.minute.toString().padLeft(2, '0')}';

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final walletCtx = WalletContext.of(context);
    final chainTipHeight = walletCtx?.superWallet.height() ?? 0;
    final txDetails = TxDetailsModel(
      tx: widget.tx,
      chainTipHeight: chainTipHeight,
      now: DateTime.now(),
    );
    final isSend = txDetails.isSend;
    final accentColor = isSend
        ? Colors.redAccent.harmonizeWith(theme.colorScheme.primary)
        : Colors.green.harmonizeWith(theme.colorScheme.primary);

    final statusText = widget.onBroadcast != null
        ? 'Signed'
        : isSend
        ? 'Sending'
        : 'Receiving';

    final baseColor = theme.colorScheme.surfaceContainerHighest;
    return Align(
      alignment: Alignment.center,
      child: MouseRegion(
        cursor: SystemMouseCursors.click,
        child: GestureDetector(
          onTap: widget.onTap,
          child: Container(
            margin: const EdgeInsets.symmetric(vertical: 4),
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
            decoration: BoxDecoration(
              color: widget.isHighlighted
                  ? Color.lerp(baseColor, theme.colorScheme.primary, 0.2)
                  : baseColor,
              borderRadius: BorderRadius.circular(12),
              boxShadow: widget.isHighlighted
                  ? [
                      BoxShadow(
                        color: theme.colorScheme.primary.withValues(alpha: 0.4),
                        blurRadius: 10,
                        spreadRadius: 1,
                      ),
                    ]
                  : [],
            ),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Icon(
                  isSend ? Icons.north_east : Icons.south_east,
                  size: 18,
                  color: accentColor,
                ),
                const SizedBox(width: 8),
                Flexible(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Text(
                        statusText,
                        style: theme.textTheme.titleSmall?.copyWith(
                          fontWeight: FontWeight.w600,
                        ),
                      ),
                      Row(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          SatoshiText(
                            value: txDetails.netValue,
                            showSign: true,
                            style: theme.textTheme.bodyLarge,
                          ),
                          const SizedBox(width: 12),
                          if (widget.onBroadcast != null)
                            _broadcasting
                                ? const SizedBox(
                                    width: 20,
                                    height: 20,
                                    child: CircularProgressIndicator(
                                      strokeWidth: 2,
                                    ),
                                  )
                                : FilledButton.tonal(
                                    onPressed: () async {
                                      setState(() => _broadcasting = true);
                                      try {
                                        await widget.onBroadcast!();
                                      } finally {
                                        if (mounted)
                                          setState(() => _broadcasting = false);
                                      }
                                    },
                                    style: FilledButton.styleFrom(
                                      padding: const EdgeInsets.symmetric(
                                        horizontal: 12,
                                      ),
                                      minimumSize: const Size(0, 28),
                                    ),
                                    child: const Text('Broadcast'),
                                  )
                          else
                            Text(
                              _formatTime(widget.timestamp),
                              style: theme.textTheme.labelSmall?.copyWith(
                                fontSize: 10,
                                color: theme.colorScheme.onSurfaceVariant,
                              ),
                            ),
                        ],
                      ),
                    ],
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _TxConfirmedLine extends StatelessWidget {
  final Transaction tx;
  final DateTime timestamp;
  final VoidCallback onTapPill;

  const _TxConfirmedLine({
    required this.tx,
    required this.timestamp,
    required this.onTapPill,
  });

  static String _formatTime(DateTime t) =>
      '${t.hour.toString().padLeft(2, '0')}:${t.minute.toString().padLeft(2, '0')}';

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final walletCtx = WalletContext.of(context);
    final chainTipHeight = walletCtx?.superWallet.height() ?? 0;
    final txDetails = TxDetailsModel(
      tx: tx,
      chainTipHeight: chainTipHeight,
      now: DateTime.now(),
    );
    final isSend = txDetails.isSend;
    final accentColor = isSend
        ? Colors.redAccent.harmonizeWith(theme.colorScheme.primary)
        : Colors.green.harmonizeWith(theme.colorScheme.primary);
    final eventStyle = theme.textTheme.bodySmall?.copyWith(
      fontStyle: FontStyle.italic,
      color: theme.colorScheme.onSurfaceVariant,
    );

    return Align(
      alignment: Alignment.center,
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 2),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text('confirmed ', style: eventStyle),
            MouseRegion(
              cursor: SystemMouseCursors.click,
              child: GestureDetector(
                onTap: onTapPill,
                child: Container(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 6,
                    vertical: 2,
                  ),
                  decoration: BoxDecoration(
                    color: theme.colorScheme.surfaceContainerHighest,
                    borderRadius: BorderRadius.circular(10),
                  ),
                  child: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Icon(
                        isSend ? Icons.north_east : Icons.south_east,
                        size: 14,
                        color: accentColor,
                      ),
                      const SizedBox(width: 2),
                      SatoshiText(
                        value: txDetails.netValue,
                        showSign: true,
                        hideLeadingWhitespace: true,
                        style: theme.textTheme.bodySmall,
                      ),
                    ],
                  ),
                ),
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
      'Jan',
      'Feb',
      'Mar',
      'Apr',
      'May',
      'Jun',
      'Jul',
      'Aug',
      'Sep',
      'Oct',
      'Nov',
      'Dec',
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
            child: Divider(
              color: theme.colorScheme.outline.withValues(alpha: 0.3),
            ),
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
            child: Divider(
              color: theme.colorScheme.outline.withValues(alpha: 0.3),
            ),
          ),
        ],
      ),
    );
  }
}

enum _OfferSignPhase { offer, waiting, signing }

enum _NoDevicesReason {
  /// None of the user's devices are in this wallet's signing group at all.
  notASigner,

  /// Every eligible device has already been offered (by us or by a
  /// duplicate from someone else). Waiting on other signers.
  allAlreadyOffered,

  /// The user has devices in the group but none are currently connected /
  /// hold nonces, so there's nothing to offer right now.
  noneReady,
}

class _OfferSignSheet extends StatefulWidget {
  final SigningRequestState state;
  final AccessStructureRef accessStructureRef;
  final NostrClient client;
  final NostrContext nostrContext;
  final int threshold;
  final NostrProfile? Function(PublicKey) getProfile;
  final DeviceId? Function(AccessStructureRef, int) deviceForShareIndex;
  final ScrollController? scrollController;

  const _OfferSignSheet({
    required this.state,
    required this.accessStructureRef,
    required this.client,
    required this.nostrContext,
    required this.threshold,
    required this.getProfile,
    required this.deviceForShareIndex,
    this.scrollController,
  });

  KeyId get keyId => accessStructureRef.keyId;
  AccessStructureId get accessStructureId =>
      accessStructureRef.accessStructureId;

  @override
  State<_OfferSignSheet> createState() => _OfferSignSheetState();
}

class _OfferSignSheetState extends State<_OfferSignSheet> {
  _OfferSignPhase _phase = _OfferSignPhase.offer;
  late Set<DeviceId> _selectedDevices;
  String? _error;

  /// All devices in this access structure, regardless of state. Used by
  /// both the eligible list and the empty-state reasoner.
  List<DeviceItem> get _allDevices {
    final accessStructure = coord.getAccessStructure(
      asRef: widget.accessStructureRef,
    );
    if (accessStructure == null) return const [];
    return DeviceItem.fromAccessStructure(accessStructure);
  }

  Set<int> get _offeredShareIndices => widget.state.offers.values
      .expand((o) => offerShareIndices(binonces: o.binonces))
      .toSet();

  /// Devices the user can still offer with: enabled (has nonces) AND the
  /// share index isn't already in the offer chain.
  List<DeviceItem> get _deviceItems {
    final offered = _offeredShareIndices;
    return _allDevices
        .where((item) => item.enabled && !offered.contains(item.shareIndex))
        .toList();
  }

  /// Reason the eligible device list is empty, so we can show an
  /// informative empty state instead of a blank section header.
  _NoDevicesReason get _noDevicesReason {
    final all = _allDevices;
    if (all.isEmpty) return _NoDevicesReason.notASigner;

    final offered = _offeredShareIndices;
    final remaining = all
        .where((d) => !offered.contains(d.shareIndex))
        .toList();
    if (remaining.isEmpty) return _NoDevicesReason.allAlreadyOffered;

    // There are devices left to offer, but none of them are enabled.
    return _NoDevicesReason.noneReady;
  }

  @override
  void initState() {
    super.initState();
    _selectedDevices = _deviceItems.map((d) => d.id).toSet();
  }

  Future<void> _doOffer() async {
    if (_selectedDevices.isEmpty) return;

    final nSelectedByOthers = widget.state.offers.length;
    final willMeetThreshold =
        nSelectedByOthers + _selectedDevices.length >= widget.threshold;

    setState(() => _phase = _OfferSignPhase.waiting);

    try {
      final nsec = widget.nostrContext.nostrSettings.getNsec();
      final reservationId = RemoteSignSessionId(
        field0: widget.state.request.eventId.field0,
      );
      final signTask = widget.state.request.signTask;
      final allBinonces = <ParticipantBinonces>[];
      for (final device in _selectedDevices) {
        final binonces = await coord.reserveNonces(
          id: reservationId,
          accessStructureRef: widget.accessStructureRef,
          signTask: signTask,
          deviceId: device,
        );
        allBinonces.add(binonces);
      }
      await widget.client.sendSignOffer(
        accessStructureId: widget.accessStructureId,
        nsec: nsec,
        requestId: widget.state.request.eventId,
        binonces: allBinonces,
      );

      if (!willMeetThreshold) {
        if (mounted) Navigator.pop(context);
        return;
      }

      for (var i = 0; i < 50; i++) {
        await Future.delayed(const Duration(milliseconds: 100));
        if (widget.state.sealedData != null) break;
      }

      if (widget.state.sealedData == null) {
        if (mounted) Navigator.pop(context);
        return;
      }

      await _startSigning();
    } catch (e) {
      if (mounted) {
        setState(() {
          _error = '$e';
          _phase = _OfferSignPhase.offer;
        });
      }
    }
  }

  Future<void> _startSigning() async {
    final walletCtx = WalletContext.of(context);
    final fsCtx = FrostsnapContext.of(context);
    if (walletCtx == null || fsCtx == null) return;

    try {
      final details = signingDetails(signTask: widget.state.request.signTask);
      if (details is! SigningDetails_Transaction) return;
      final tx = details.transaction;
      final txDetails = TxDetailsModel(
        tx: tx,
        chainTipHeight: walletCtx.superWallet.height(),
        now: DateTime.now(),
      );

      setState(() => _phase = _OfferSignPhase.signing);

      final nsec = widget.nostrContext.nostrSettings.getNsec();
      await showBottomSheetOrDialog(
        context,
        title: const Text('Signing'),
        builder: (ctx, scrollController) => walletCtx.wrap(
          NostrSigningPage(
            scrollController: scrollController,
            txDetails: txDetails,
            signingState: widget.state,
            threshold: widget.threshold,
            getProfile: widget.getProfile,
            client: widget.client,
            accessStructureRef: widget.accessStructureRef,
            nsec: nsec,
            myPubkey: widget.nostrContext.myPubkey,
          ),
        ),
      );

      if (mounted) Navigator.pop(context, true);
    } catch (e) {
      if (mounted) {
        setState(() {
          _error = '$e';
          _phase = _OfferSignPhase.offer;
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    if (_phase == _OfferSignPhase.waiting ||
        _phase == _OfferSignPhase.signing) {
      return Padding(
        padding: const EdgeInsets.all(32),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const CircularProgressIndicator(),
            const SizedBox(height: 16),
            Text(
              _phase == _OfferSignPhase.waiting
                  ? 'Broadcasting offer...'
                  : 'Signing...',
              style: theme.textTheme.bodyMedium,
            ),
          ],
        ),
      );
    }

    final details = signingDetails(signTask: widget.state.request.signTask);
    final walletCtx = WalletContext.of(context);

    return SingleChildScrollView(
      controller: widget.scrollController,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (details is SigningDetails_Transaction && walletCtx != null) ...[
            buildDetailsColumn(
              context,
              txDetails: TxDetailsModel(
                tx: details.transaction,
                chainTipHeight: walletCtx.superWallet.height(),
                now: DateTime.now(),
              ),
              showConfirmations: false,
            ),
            const Divider(),
          ] else if (details is SigningDetails_Message) ...[
            ListTile(
              leading: const Icon(Icons.message),
              title: Text(details.message),
            ),
            const Divider(),
          ],
          if (_deviceItems.isEmpty)
            _NoDevicesCard(reason: _noDevicesReason)
          else
            DeviceSelectorList(
              title: 'Offer to sign with',
              devices: _deviceItems,
              selected: _selectedDevices,
              onToggle: (id) => setState(() {
                if (_selectedDevices.contains(id)) {
                  _selectedDevices.remove(id);
                } else {
                  _selectedDevices.add(id);
                }
              }),
            ),
          if (_error != null)
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
              child: Text(
                _error!,
                style: TextStyle(color: theme.colorScheme.error),
              ),
            ),
          if (_deviceItems.isNotEmpty)
            Padding(
              padding: const EdgeInsets.all(16),
              child: FilledButton(
                onPressed: _selectedDevices.isNotEmpty ? _doOffer : null,
                child: Text(
                  widget.state.offers.length + _selectedDevices.length >=
                          widget.threshold
                      ? 'Sign'
                      : 'Offer to Sign',
                ),
              ),
            ),
        ],
      ),
    );
  }
}

/// Empty-state card shown in place of the device selector when the user
/// can't offer to sign — because they're not in the signing group, because
/// every eligible device has already been offered, or because no devices
/// are ready right now. Each reason gets its own icon and copy so the user
/// knows whether to wait, act, or simply close the sheet.
class _NoDevicesCard extends StatelessWidget {
  final _NoDevicesReason reason;

  const _NoDevicesCard({required this.reason});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final (icon, headline, body) = _copyFor(reason);

    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 8, 16, 24),
      child: Container(
        padding: const EdgeInsets.fromLTRB(24, 28, 24, 28),
        decoration: BoxDecoration(
          color: theme.colorScheme.surfaceContainerHighest,
          borderRadius: BorderRadius.circular(20),
          border: Border.all(
            color: theme.colorScheme.outlineVariant.withValues(alpha: 0.6),
          ),
        ),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Container(
              width: 56,
              height: 56,
              decoration: BoxDecoration(
                color: theme.colorScheme.secondaryContainer,
                shape: BoxShape.circle,
              ),
              child: Icon(
                icon,
                size: 28,
                color: theme.colorScheme.onSecondaryContainer,
              ),
            ),
            const SizedBox(height: 16),
            Text(
              headline,
              style: theme.textTheme.titleMedium?.copyWith(
                fontWeight: FontWeight.w600,
              ),
              textAlign: TextAlign.center,
            ),
            const SizedBox(height: 6),
            Text(
              body,
              textAlign: TextAlign.center,
              style: theme.textTheme.bodyMedium?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
                height: 1.4,
              ),
            ),
          ],
        ),
      ),
    );
  }

  (IconData, String, String) _copyFor(_NoDevicesReason reason) {
    switch (reason) {
      case _NoDevicesReason.notASigner:
        return (
          Icons.key_off_outlined,
          "You're not a signer here",
          "None of your devices belong to this wallet's signing group, so you can't contribute a signature.",
        );
      case _NoDevicesReason.allAlreadyOffered:
        return (
          Icons.check_circle_outline,
          "You've already offered",
          "Your share of this signing round is in. Waiting on the rest of the signers.",
        );
      case _NoDevicesReason.noneReady:
        return (
          Icons.usb_outlined,
          "No signing device ready",
          "Plug in one of your devices for this wallet to offer a signature.",
        );
    }
  }
}
