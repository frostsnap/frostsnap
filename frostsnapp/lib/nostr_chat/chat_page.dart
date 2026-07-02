import 'dart:async';
import 'dart:math' as math;
import 'package:flutter/material.dart' hide ConnectionState;
import 'package:flutter/services.dart';
import 'package:frostsnap/copy_feedback.dart';
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
import 'package:frostsnap/src/rust/lib.dart' show WireSignTask;
import 'package:frostsnap/wallet_send.dart';
import 'package:frostsnap/src/rust/api/super_wallet.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/theme.dart';
import 'package:frostsnap/wallet.dart';
import 'package:frostsnap/wallet_receive.dart';
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

/// Safety cap on how far a peer's claimed receive index may be ahead
/// of this wallet's local cursor before we auto-advance via
/// `mark_address_shared`. Bigger jumps require explicit user
/// confirmation. BDK's default gap-limit is 20; 100 covers normal
/// usage without letting a malicious peer push the cursor far ahead.
const int _receiveIndexLookahead = 100;

sealed class TimelineItem {
  DateTime get timestamp;
}

/// Timeline variants that render as canonical chat bubbles. The
/// abstract `buildBubble` enforces — at compile time — that anything
/// claiming to be a chat message produces a [ChatBubble]. Free-form
/// system cards (signing, transaction, error) stay direct siblings
/// of [TimelineItem].
sealed class ChatBubbleItem extends TimelineItem {
  ChatBubble buildBubble(BuildContext context, ChatBubbleHandlers h);
}

class TimelineChat extends ChatBubbleItem {
  final ChatMessage message;
  @override
  DateTime get timestamp => message.timestamp;
  TimelineChat(this.message);

  @override
  ChatBubble buildBubble(BuildContext context, ChatBubbleHandlers h) {
    final replyTarget = h.getReplyTarget(message.replyTo);
    return ChatBubble(
      key: h.timelineKeys.putIfAbsent(
        message.messageId.toHex(),
        () => GlobalKey(),
      ),
      author: message.author,
      authorProfile: h.getProfile(message.author),
      isMe: message.isMe,
      timestamp: message.timestamp,
      status: message.status,
      failureReason: message.failureReason,
      replyQuote: replyTarget == null
          ? null
          : _ChatReplyQuote(
              target: replyTarget,
              targetProfile: h.getProfile(replyTarget.author),
              onTap: () => h.onScrollToHighlight(replyTarget.messageId),
            ),
      text: message.content,
      onTapAvatar: message.isMe
          ? null
          : () => h.onShowMemberProfile(message.author),
      onReply: () => h.onReplyChat(message),
      onCopy: () => h.onCopyChat(message),
      onRetry: () => h.onRetryChat(message),
      onTapQuote: replyTarget == null
          ? null
          : () => h.onScrollToHighlight(replyTarget.messageId),
      isHighlighted: h.highlightedId == message.messageId.toHex(),
    );
  }
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

class ReceiveAddressCardModel {
  final EventId messageId;
  final PublicKey author;
  final DateTime timestamp;
  final bool isMe;
  final int derivationIndex;
  final String memo;
  MessageStatus status;
  String? failureReason;
  bool markedShared;

  ReceiveAddressCardModel({
    required this.messageId,
    required this.author,
    required this.timestamp,
    required this.isMe,
    required this.derivationIndex,
    required this.memo,
    required this.status,
    this.failureReason,
    this.markedShared = false,
  });
}

class TimelineReceiveAddress extends ChatBubbleItem {
  final ReceiveAddressCardModel card;
  @override
  DateTime get timestamp => card.timestamp;
  TimelineReceiveAddress(this.card);

  @override
  ChatBubble buildBubble(BuildContext context, ChatBubbleHandlers h) {
    // The address is derived locally from the receiver's own
    // descriptor — see `_ReceiveAttachment`. Sender and receiver
    // render an IDENTICAL attachment; no verification step exists
    // because there's no claim to verify.
    final canOpen = card.status == MessageStatus.sent;
    return ChatBubble(
      key: h.timelineKeys.putIfAbsent(
        card.messageId.toHex(),
        () => GlobalKey(),
      ),
      author: card.author,
      authorProfile: h.getProfile(card.author),
      isMe: card.isMe,
      timestamp: card.timestamp,
      status: card.status,
      failureReason: card.failureReason,
      attachment: _ReceiveAttachment(derivationIndex: card.derivationIndex),
      text: card.memo,
      onTap: canOpen ? () => h.onOpenReceive(card) : null,
      onRetry: card.isMe ? () => h.onRetryReceive(card) : null,
      onTapAvatar: card.isMe ? null : () => h.onShowMemberProfile(card.author),
    );
  }
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

enum TxTimelineKind { needsBroadcast }

/// Locally-signed tx awaiting broadcast. Owned entirely by the Dart
/// signing flow — the wallet hasn't observed this tx yet so the
/// runner can't emit a `TxObservation` for it. Removed from the
/// timeline once the first `Mempool` `TxObservation` arrives for
/// the txid.
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

/// A wallet-observed tx (in mempool or just confirmed) emitted by the
/// channel runner. Carries identifiers + chat-side correlations only;
/// the renderer fetches the opaque `Transaction` via
/// `walletCtx.superWallet.getTx(txid)` at build time.
class TimelineTxObservation extends TimelineItem {
  final String txid;
  final ObservationKind kind;
  @override
  final DateTime timestamp;
  final EventId? addressRevealEvent;
  final EventId? signingStartEvent;

  TimelineTxObservation({
    required this.txid,
    required this.kind,
    required int timestampSecs,
    this.addressRevealEvent,
    this.signingStartEvent,
  }) : timestamp = DateTime.fromMillisecondsSinceEpoch(timestampSecs * 1000);
}

/// Body of the chat surface — owns the channel connection, timeline,
/// composer, and signing-request state. Returns a body widget (no
/// Scaffold/AppBar) so it embeds cleanly inside the remote-wallet
/// shell's TabBarView.
///
/// `chrome` (optional) lets a host AppBar render the connection
/// indicator + group-info action without reaching into this state.
/// `autofocus` controls whether the composer pops the keyboard at
/// mount time — defaults to `false`, which is what the embedded use
/// wants. (Reintroduce a Scaffold wrapper if a future deep-link or
/// debug screen needs to push chat as its own route.)
class ChatPageBody extends StatefulWidget {
  final AccessStructureRef accessStructureRef;
  final String walletName;
  final ChannelConnectionParams channelParams;
  final ChatChromeController? chrome;
  final bool autofocus;

  const ChatPageBody({
    super.key,
    required this.accessStructureRef,
    required this.walletName,
    required this.channelParams,
    this.chrome,
    this.autofocus = false,
  });

  KeyId get keyId => accessStructureRef.keyId;
  AccessStructureId get accessStructureId =>
      accessStructureRef.accessStructureId;

  @override
  State<ChatPageBody> createState() => _ChatPageBodyState();
}

/// Read-only handle for a host (Scaffold/AppBar) to observe a chat
/// body's connection state and trigger its group-info sheet without
/// reaching into widget state.
class ChatChromeController extends ChangeNotifier {
  ConnectionState _connectionState = const ConnectionState.connecting();
  ConnectionState get connectionState => _connectionState;

  VoidCallback? _openGroupInfo;

  /// Null until the embedded `ChatPageBody` initialises and binds its
  /// handler. Hosts should pass this directly to `IconButton.onPressed`
  /// — `null` disables the button until the body is ready.
  VoidCallback? get openGroupInfo => _openGroupInfo;

  void updateConnectionState(ConnectionState state) {
    _connectionState = state;
    notifyListeners();
  }

  void bindOpenGroupInfo(VoidCallback handler) {
    _openGroupInfo = handler;
    notifyListeners();
  }
}

class _ChatPageBodyState extends State<ChatPageBody> {
  final TextEditingController _messageController = TextEditingController();
  final ScrollController _scrollController = ScrollController();
  late final FocusNode _inputFocusNode;
  final List<TimelineItem> _timeline = [];
  final Map<EventId, ChatMessage> _messageById = {};
  final Map<EventId, ReceiveAddressCardModel> _receiveCardById = {};
  final Map<String, GlobalKey> _timelineKeys = {};
  String? _highlightedId;
  final Map<EventId, SigningRequestState> _signingRequests = {};
  final Set<String> _seenSigningEventIds = {};
  late List<PublicKey> _memberPubkeys;
  StreamSubscription<ChannelEvent>? _subscription;
  StreamSubscription<TxState>? _txSubscription;

  /// Tracks the `TxTimelineKind.needsBroadcast` entries inserted
  /// Dart-side after signing completion. Replaced by a
  /// `TimelineTxObservation` once the wallet observes the tx.
  final Map<String, TxTimelineKind> _txTimelineState = {};

  /// Upsert key for runner-emitted observations. `(txid, kind)` →
  /// the currently-inserted timeline item. Lets repeat
  /// notifications no-op and lets the kind transition coexist
  /// (mempool item stays after a separate confirmed pill is added).
  final Map<(String, ObservationKind), TimelineTxObservation> _txItemByKey = {};
  NostrClient? _client;
  ChannelHandle? _handle;
  ConnectionState _connectionState = const ConnectionState.connecting();
  ReplyTarget? _replyingTo;
  ({AccessStructureRef asRef, String testMessage, List<DeviceId> devices})?
  _pendingSignRequest;
  ({AccessStructureRef asRef, UnsignedTx unsignedTx, List<DeviceId> devices})?
  _pendingTxSignRequest;
  ({int index, String address})? _pendingReceiveAttachment;

  NostrContext? _nostrContext;
  PublicKey get _myPubkey => _nostrContext!.myPubkey;

  @override
  void initState() {
    super.initState();
    _inputFocusNode = FocusNode(onKeyEvent: _handleKeyEvent);
    if (widget.autofocus) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        _inputFocusNode.requestFocus();
      });
    }
    // Bind chrome callbacks AFTER the current build completes — these
    // notify listeners on the ChatChromeController, and a host AppBar's
    // ListenableBuilder higher up the tree would otherwise be dirtied
    // mid-build (this widget is constructed during the host's build).
    final chrome = widget.chrome;
    if (chrome != null) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (!mounted) return;
        chrome.bindOpenGroupInfo(_openGroupInfo);
        chrome.updateConnectionState(_connectionState);
      });
    }
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final nostr = NostrContext.of(context);
    if (_nostrContext == null) {
      _nostrContext = nostr;
      _memberPubkeys = [nostr.myPubkey];
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
    final handle = _handle;
    if (handle == null) return;
    for (final tx in state.txs) {
      // Local-only pump. Runner folds correlations against the
      // chat-side maps it maintains and emits TxObservation events
      // that drive timeline insertion.
      handle.notifyTxObserved(tx: tx);
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
    final nostr = _nostrContext!;
    _client = await nostr.nostrClient;
    final identity = nostr.nostrSettings.currentIdentity();
    if (identity == null) {
      throw StateError('nostr identity not configured');
    }
    _handle = await _client!.connectToChannel(
      identity: identity,
      params: widget.channelParams,
    );
    final stream = _handle!.events().watch();
    _subscription = stream.listen(_handleEvent);
    // listen-then-start: attaches the broadcast sink before the runner
    // emits Connecting/Connected and replays the cache.
    await _handle!.start();
  }

  NostrProfile? _getProfile(PublicKey pubkey) {
    return _nostrContext!.getProfile(pubkey);
  }

  List<int> _shareIndicesForPubkey(PublicKey pubkey) {
    final hex = pubkey.toHex();
    for (final p in _participantShares) {
      if (p.pubkey.toHex() == hex) return p.shareIndices.toList();
    }
    return const [];
  }

  void _showMemberProfile(PublicKey pubkey) {
    final profile = _getProfile(pubkey);
    final hasName =
        (profile?.displayName?.isNotEmpty ?? false) ||
        (profile?.name?.isNotEmpty ?? false);
    showBottomSheetOrDialog(
      context,
      title: hasName ? Text(getDisplayName(profile, pubkey)) : null,
      builder: (sheetContext, scrollController) => MemberDetailSheet(
        pubkey: pubkey,
        profile: profile,
        keyIndices: _shareIndicesForPubkey(pubkey),
        scrollController: scrollController,
      ),
    );
  }

  DeviceId? _getMyDevice(SigningRequestState? state) {
    if (state == null) return null;
    final myOffer = state.offers[_myPubkey.toHex()];
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
        final isMe = author == _myPubkey;
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
        final card = _receiveCardById[messageId];
        if (card != null && card.status != MessageStatus.sent) {
          card.status = MessageStatus.sent;
          unawaited(_markAddressSharedFor(card));
        }

      case ChannelEvent_MessageSendFailed(:final messageId, :final reason):
        final msg = _messageById[messageId];
        if (msg != null) {
          msg.status = MessageStatus.failed;
          msg.failureReason = reason;
        }

      case ChannelEvent_ReceiveAddress(
        :final messageId,
        :final author,
        :final timestamp,
        :final pending,
        :final derivationIndex,
        :final memo,
      ):
        final existing = _receiveCardById[messageId];
        if (existing != null) return;
        final isMe = author == _myPubkey;
        final card = ReceiveAddressCardModel(
          messageId: messageId,
          author: author,
          timestamp: DateTime.fromMillisecondsSinceEpoch(timestamp * 1000),
          isMe: isMe,
          derivationIndex: derivationIndex,
          memo: memo,
          status: pending ? MessageStatus.pending : MessageStatus.sent,
        );
        _receiveCardById[messageId] = card;
        _insertTimelineItem(TimelineReceiveAddress(card));
        if (!isMe && card.status == MessageStatus.sent) {
          _maybeMarkAddressSharedForPeer(card);
        }

      case ChannelEvent_ReceiveAddressSendFailed(
        :final messageId,
        :final reason,
      ):
        final card = _receiveCardById[messageId];
        if (card != null) {
          card.status = MessageStatus.failed;
          card.failureReason = reason;
        }

      case ChannelEvent_TxObservation(
        :final txid,
        :final kind,
        :final timestamp,
        :final addressRevealEvent,
        :final signingStartEvent,
      ):
        final key = (txid, kind);
        final existing = _txItemByKey[key];
        if (existing != null) break;
        if (kind == ObservationKind.mempool) {
          // First wallet observation for this txid — drop any
          // pre-broadcast (locally-signed, awaiting broadcast) card.
          _timeline.removeWhere(
            (item) =>
                item is TimelineTransaction &&
                item.tx.txid == txid &&
                item.kind == TxTimelineKind.needsBroadcast,
          );
        }
        final item = TimelineTxObservation(
          txid: txid,
          kind: kind,
          timestampSecs: timestamp.toInt(),
          addressRevealEvent: addressRevealEvent,
          signingStartEvent: signingStartEvent,
        );
        _txItemByKey[key] = item;
        _insertTimelineItem(item);

      case ChannelEvent_ConnectionState(:final field0):
        _connectionState = field0;
        widget.chrome?.updateConnectionState(field0);
        if (field0 is ConnectionState_Connected && _pendingTxState != null) {
          _applyTxState(_pendingTxState!);
          _pendingTxState = null;
        }

      case ChannelEvent_GroupMetadata(:final members):
        _memberPubkeys = members.map((m) => m.pubkey).toList();
        if (!_memberPubkeys.any((m) => m == _myPubkey)) {
          _memberPubkeys.add(_myPubkey);
        }
        _nostrContext!.updateProfilesFromChannel(members);

      case ChannelEvent_Signing(:final event, :final pending):
        // Round decisions (RoundConfirmed, RoundAborted) are local
        // decisions derived from the settling timer; they don't carry a
        // nostr event_id and shouldn't produce chat messages. Route them
        // directly to the SigningRequestState.
        if (event is SigningEvent_RoundConfirmed) {
          final state = _signingRequests[event.requestId];
          if (state != null && _handle != null) {
            final binonces = event.subset.expand((e) => e.binonces).toList();
            final sealed = _handle!.sealRoundConfirmed(
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
        final isMe = author == _myPubkey;
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

      case ChannelEvent_ChannelState(:final participants):
        _participantShares = participants;
    }
  }

  List<ChannelParticipant> _participantShares = [];

  int _getThreshold(EventId requestId) {
    final state = _signingRequests[requestId];
    if (state == null) return 0;
    final accessStruct = coord.getAccessStructure(
      asRef: widget.accessStructureRef,
    );
    return accessStruct?.threshold() ?? 0;
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
        final bubbleHandlers = ChatBubbleHandlers(
          getProfile: _getProfile,
          getReplyTarget: (id) => id == null ? null : _messageById[id],
          myPubkey: _myPubkey,
          highlightedId: _highlightedId,
          timelineKeys: _timelineKeys,
          onShowMemberProfile: _showMemberProfile,
          onScrollToHighlight: _scrollToAndHighlight,
          onReplyChat: (message) => _startReply(
            ReplyTarget(
              eventId: message.messageId,
              author: message.author,
              preview: message.content,
              isMe: message.isMe,
            ),
          ),
          onRetryChat: _retryMessage,
          onCopyChat: _copyMessage,
          onOpenReceive: (card) => unawaited(_openReceivePage(card)),
          onRetryReceive: (card) => unawaited(_retryReceiveSend(card)),
        );
        final child = switch (item) {
          ChatBubbleItem b => b.buildBubble(context, bubbleHandlers),
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
            :final timestamp,
            :final signingState,
          ) =>
            _TransactionCard(
              key: _timelineKeys.putIfAbsent(tx.txid, () => GlobalKey()),
              tx: tx,
              timestamp: timestamp,
              onTap: () => _showTxDetails(tx),
              onBroadcast: signingState != null
                  ? () => _broadcastTransaction(signingState)
                  : null,
            ),
          TimelineTxObservation(
            :final txid,
            :final kind,
            :final timestamp,
            :final addressRevealEvent,
          ) =>
            _buildTxObservationCard(
              txid: txid,
              kind: kind,
              timestamp: timestamp,
              addressRevealEvent: addressRevealEvent,
            ),
          TimelineError() => SigningErrorCard(
            text: item.reason,
            author: item.author,
            profile: _getProfile(item.author),
            isMe: item.author == _myPubkey,
            onCopy: () => copyToClipboard(item.reason),
            onReply: () => _startReply(
              ReplyTarget(
                eventId: item.eventId,
                author: item.author,
                preview: 'Error: ${item.reason}',
                isMe: item.author == _myPubkey,
              ),
            ),
            onTapAvatar: item.author == _myPubkey
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

  /// Render a runner-emitted `TimelineTxObservation` item. Fetches
  /// the opaque `Transaction` from the wallet via `getTx(txid)` at
  /// build time. If the wallet doesn't know about it yet (race or
  /// chain-sync gap), renders a minimal placeholder card so the
  /// timeline doesn't jump on the next snapshot.
  Widget _buildTxObservationCard({
    required String txid,
    required ObservationKind kind,
    required DateTime timestamp,
    required EventId? addressRevealEvent,
  }) {
    final walletCtx = WalletContext.of(context);
    final tx = walletCtx?.superWallet.getTx(
      masterAppkey: walletCtx.masterAppkey,
      txid: txid,
    );
    if (tx == null) {
      return _TxLookupPlaceholder(txid: txid, timestamp: timestamp);
    }
    switch (kind) {
      case ObservationKind.mempool:
        return _TransactionCard(
          key: _timelineKeys.putIfAbsent(txid, () => GlobalKey()),
          tx: tx,
          timestamp: timestamp,
          isHighlighted: _highlightedId == txid,
          onTap: () => _showTxDetails(tx),
          quote: _buildReceiveQuoteFor(addressRevealEvent),
        );
      case ObservationKind.confirmed:
        return _TxConfirmedLine(
          tx: tx,
          timestamp: timestamp,
          onTapPill: () => _scrollToByStringId(txid),
        );
    }
  }

  /// Build a `_QuoteHeader` for a tx-observation card that links back
  /// to the receive-address share message announcing the address the
  /// tx pays to. Returns null when the runner didn't resolve a quote
  /// or the receive bubble is no longer in `_receiveCardById` (rare).
  Widget? _buildReceiveQuoteFor(EventId? id) {
    if (id == null) return null;
    final card = _receiveCardById[id];
    if (card == null) return null;
    final label = card.isMe
        ? 'You'
        : getDisplayName(_getProfile(card.author), card.author);
    final body = card.memo.isEmpty
        ? 'Address #${card.derivationIndex}'
        : card.memo;
    return _QuoteHeader(
      label: label,
      body: body,
      bodyIcon: Icons.call_received_rounded,
      onTap: () => _scrollToAndHighlight(id),
    );
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
    final iOffered = state.offers.containsKey(myPubkey.toHex());
    final accessStruct = coord.getAccessStructure(
      asRef: widget.accessStructureRef,
    );
    final threshold = accessStruct?.threshold() ?? 0;
    final reqIsMe = state.request.author == myPubkey;
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
    final isMe = offer.author == _myPubkey;
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
    final isMe = partial.author == _myPubkey;
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
    final isMe = cancel.author == _myPubkey;
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
    final nsec = await NostrContext.of(context).ensureIdentity(context);
    if (nsec == null || !mounted) return;
    setState(() => state.cancelled = true);
    try {
      await _handle!.sendSignCancel(requestId: state.request.eventId);
    } catch (e) {
      if (mounted) {
        setState(() => state.cancelled = false);
        showErrorSnackbar(context, 'Failed to cancel signing request: $e');
      }
    }
  }

  Future<void> _onOfferToSign(SigningRequestState state) async {
    final frostKey = coord.getFrostKey(keyId: widget.keyId);
    if (frostKey == null || _handle == null) return;
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
                handle: _handle!,
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
          handle: _handle!,
          nostrContext: _nostrContext!,
          threshold: threshold,
          getProfile: _getProfile,
          deviceForShareIndex: _deviceForShareIndex,
          scrollController: scrollController,
        );
      },
    );
  }

  void _openSigningPage(SigningRequestState state) async {
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
    final nsec = await NostrContext.of(context).ensureIdentity(context);
    if (nsec == null || !mounted) return;
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
          handle: _handle!,
          accessStructureRef: widget.accessStructureRef,
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
            const SizedBox(height: 8),
            ListTile(
              shape: RoundedRectangleBorder(
                borderRadius: BorderRadius.circular(24),
              ),
              tileColor: theme.colorScheme.surfaceContainer,
              leading: const Icon(Icons.call_received_rounded),
              title: const Text('Receive Bitcoin'),
              onTap: () {
                Navigator.pop(context);
                _proposeReceiveAddress();
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
      _pendingTxSignRequest = null;
      _pendingReceiveAttachment = null;
      _pendingSignRequest = (
        asRef: widget.accessStructureRef,
        testMessage: testMessage.trim(),
        devices: <DeviceId>[],
      );
    });
    _inputFocusNode.requestFocus();
  }

  void _proposeReceiveAddress() {
    final walletCtx = WalletContext.of(context);
    if (walletCtx == null || _client == null) return;

    final info = walletCtx.superWallet.nextAddress(
      masterAppkey: walletCtx.masterAppkey,
    );
    setState(() {
      _pendingSignRequest = null;
      _pendingTxSignRequest = null;
      _pendingReceiveAttachment = (
        index: info.index,
        address: info.address.toString(),
      );
    });
    _inputFocusNode.requestFocus();
  }

  /// Deep-link from a sent + verified receive card into the canonical
  /// `ReceivePage` for that address index. Reuses local-receive UX
  /// (copy, verify on device, tx history) instead of duplicating it.
  Future<void> _openReceivePage(ReceiveAddressCardModel card) async {
    final walletCtx = WalletContext.of(context);
    if (walletCtx == null) return;
    await showBottomSheetOrDialog(
      context,
      title: const Text('Receive'),
      builder: (_, scrollController) => walletCtx.wrap(
        ReceivePage(
          wallet: walletCtx.wallet,
          txStream: walletCtx.txStream,
          derivationIndex: card.derivationIndex,
          scrollController: scrollController,
        ),
      ),
    );
  }

  Future<void> _markAddressSharedFor(ReceiveAddressCardModel card) async {
    if (card.markedShared) return;
    final walletCtx = WalletContext.of(context);
    if (walletCtx == null) return;
    try {
      await walletCtx.superWallet.markAddressShared(
        masterAppkey: walletCtx.masterAppkey,
        derivationIndex: card.derivationIndex,
      );
      if (!mounted) return;
      setState(() {
        card.markedShared = true;
      });
    } catch (e) {
      debugPrint('markAddressShared failed: $e');
    }
  }

  Future<void> _retryReceiveSend(ReceiveAddressCardModel card) async {
    if (_client == null) return;
    final nsec = await NostrContext.of(context).ensureIdentity(context);
    if (nsec == null || !mounted) return;
    try {
      await _handle!.sendReceiveAddress(
        derivationIndex: card.derivationIndex,
        memo: card.memo,
      );
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text('Retry failed: $e')));
    }
  }

  /// Receiver-side: advance the local wallet's reveal cursor when
  /// a peer publishes a usable index. Out-of-window indices (a
  /// malicious or buggy peer pushing very far) are silently
  /// ignored — the bubble still renders the derived address; we
  /// just don't advance our cursor.
  void _maybeMarkAddressSharedForPeer(ReceiveAddressCardModel card) {
    if (card.markedShared) return;
    final walletCtx = WalletContext.of(context);
    if (walletCtx == null) return;
    final localNext = walletCtx.superWallet
        .nextAddress(masterAppkey: walletCtx.masterAppkey)
        .index;
    if (card.derivationIndex > localNext + _receiveIndexLookahead) return;
    unawaited(_markAddressSharedFor(card));
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
              _pendingSignRequest = null;
              _pendingReceiveAttachment = null;
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
    // Single-attachment invariant: at most one of these is non-null;
    // producers enforce mutual exclusion when they set their own.
    final pending = _pendingSignRequest;
    final pendingTx = _pendingTxSignRequest;
    final pendingReceive = _pendingReceiveAttachment;
    if (content.isEmpty &&
        pending == null &&
        pendingTx == null &&
        pendingReceive == null) {
      return;
    }
    if (_client == null) return;

    final nsec = await NostrContext.of(context).ensureIdentity(context);
    if (nsec == null || !mounted) return;
    final replyToId = _replyingTo?.eventId;
    _messageController.clear();
    setState(() {
      _replyingTo = null;
      _pendingSignRequest = null;
      _pendingTxSignRequest = null;
      _pendingReceiveAttachment = null;
    });

    try {
      if (pendingTx != null) {
        final requestId = await _handle!.sendSignRequest(
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
        final requestId = await _handle!.sendTestSignRequest(
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
      } else if (pendingReceive != null) {
        await _handle!.sendReceiveAddress(
          derivationIndex: pendingReceive.index,
          memo: content,
        );
      } else {
        await _handle!.sendMessage(content: content, replyTo: replyToId);
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
    await _handle!.sendSignOffer(requestId: requestId, binonces: allBinonces);
  }

  Future<void> _retryMessage(ChatMessage message) async {
    if (message.status != MessageStatus.failed || _handle == null) return;

    final nsec = await NostrContext.of(context).ensureIdentity(context);
    if (nsec == null || !mounted) return;
    setState(() {
      _timeline.removeWhere(
        (item) => item is TimelineChat && item.message == message,
      );
      _messageById.remove(message.messageId);
    });

    await _handle!.sendMessage(
      content: message.content,
      replyTo: message.replyTo,
    );
  }

  void _copyMessage(ChatMessage message) {
    Clipboard.setData(ClipboardData(text: message.content));
  }

  String _displayName(PublicKey author) {
    if (author == _myPubkey) return 'You';
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
      isMe: author == _myPubkey,
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
    final walletCtx = WalletContext.of(context);
    final page = GroupInfoPage(
      walletName: widget.walletName,
      members: _memberPubkeys,
      accessStructureId: widget.accessStructureId,
      participantShares: _participantShares,
    );
    Navigator.of(context).push(
      MaterialPageRoute(
        builder: (_) => walletCtx != null ? walletCtx.wrap(page) : page,
      ),
    );
  }

  @override
  void dispose() {
    _subscription?.cancel();
    _txSubscription?.cancel();
    _handle?.close();
    _messageController.dispose();
    _scrollController.dispose();
    _inputFocusNode.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      children: [
        Expanded(child: _buildTimeline(theme)),
        _buildSigningBanner(theme),
        _buildMessageInput(),
      ],
    );
  }

  Widget _buildSigningBanner(ThemeData theme) {
    final activeRequest = _activeSigningRequest;
    if (activeRequest == null) return const SizedBox.shrink();

    final myPubkey = _myPubkey;
    final isRequester = activeRequest.request.author == myPubkey;
    final alreadySigned = activeRequest.partials.containsKey(myPubkey.toHex());
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
        _pendingTxSignRequest != null ||
        _pendingReceiveAttachment != null;

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
            if (_pendingReceiveAttachment != null)
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
                    const Icon(Icons.call_received_rounded, size: 18),
                    const SizedBox(width: 8),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            'Receive address',
                            style: theme.textTheme.labelSmall?.copyWith(
                              color: theme.colorScheme.secondary,
                              fontWeight: FontWeight.w600,
                            ),
                          ),
                          Text(
                            '#${_pendingReceiveAttachment!.index} · '
                            '${_pendingReceiveAttachment!.address}',
                            softWrap: true,
                            style: theme.textTheme.bodySmall?.copyWith(
                              color: theme.colorScheme.onSurfaceVariant,
                              fontFamily: 'monospace',
                            ),
                          ),
                        ],
                      ),
                    ),
                    IconButton(
                      icon: const Icon(Icons.close, size: 18),
                      onPressed: () =>
                          setState(() => _pendingReceiveAttachment = null),
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

  /// Optional quote header rendered above the tx body — used to
  /// link an incoming tx back to the receive-address share message
  /// that announced it. The card max-width is capped so a 2-line
  /// memo doesn't balloon the card.
  final Widget? quote;

  const _TransactionCard({
    super.key,
    required this.tx,
    required this.timestamp,
    this.isHighlighted = false,
    required this.onTap,
    this.onBroadcast,
    this.quote,
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
    final body = Row(
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
                            child: CircularProgressIndicator(strokeWidth: 2),
                          )
                        : FilledButton.tonal(
                            onPressed: () async {
                              setState(() => _broadcasting = true);
                              try {
                                await widget.onBroadcast!();
                              } finally {
                                if (mounted) {
                                  setState(() => _broadcasting = false);
                                }
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
    );

    return Align(
      alignment: Alignment.center,
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 360),
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
                          color: theme.colorScheme.primary.withValues(
                            alpha: 0.4,
                          ),
                          blurRadius: 10,
                          spreadRadius: 1,
                        ),
                      ]
                    : [],
              ),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                mainAxisSize: MainAxisSize.min,
                children: [
                  if (widget.quote != null) ...[
                    widget.quote!,
                    const SizedBox(height: 6),
                  ],
                  body,
                ],
              ),
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
  final ChannelHandle handle;
  final NostrContext nostrContext;
  final int threshold;
  final NostrProfile? Function(PublicKey) getProfile;
  final DeviceId? Function(AccessStructureRef, int) deviceForShareIndex;
  final ScrollController? scrollController;

  const _OfferSignSheet({
    required this.state,
    required this.accessStructureRef,
    required this.handle,
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

    final nsec = await NostrContext.of(context).ensureIdentity(context);
    if (nsec == null || !mounted) return;

    final nSelectedByOthers = widget.state.offers.length;
    final willMeetThreshold =
        nSelectedByOthers + _selectedDevices.length >= widget.threshold;

    setState(() => _phase = _OfferSignPhase.waiting);

    try {
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
      await widget.handle.sendSignOffer(
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
            handle: widget.handle,
            accessStructureRef: widget.accessStructureRef,
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

// =====================================================================
// Canonical chat-bubble shape
// =====================================================================

/// Per-timeline-item callbacks the host provides to all chat-bubble
/// variants. Each [ChatBubbleItem] subclass uses the subset it needs.
class ChatBubbleHandlers {
  /// Profile lookup for the bubble author / reply target avatar.
  final NostrProfile? Function(PublicKey) getProfile;

  /// Reply-target lookup so a bubble can render the quoted message
  /// inline.
  final ChatMessage? Function(EventId?) getReplyTarget;

  /// The local user's pubkey — used to colour and align bubbles.
  final PublicKey myPubkey;

  /// IDs (hex) currently being scroll-highlighted.
  final String? highlightedId;

  /// Per-timeline-item GlobalKeys for scrollTo + highlight.
  final Map<String, GlobalKey> timelineKeys;

  /// Open another member's profile sheet.
  final void Function(PublicKey) onShowMemberProfile;

  /// Jump the timeline to a quoted message by id.
  final void Function(EventId) onScrollToHighlight;

  // ----- chat-text-specific -----
  final void Function(ChatMessage) onReplyChat;
  final void Function(ChatMessage) onRetryChat;
  final void Function(ChatMessage) onCopyChat;

  // ----- receive-specific -----
  final void Function(ReceiveAddressCardModel) onOpenReceive;
  final void Function(ReceiveAddressCardModel) onRetryReceive;

  const ChatBubbleHandlers({
    required this.getProfile,
    required this.getReplyTarget,
    required this.myPubkey,
    required this.highlightedId,
    required this.timelineKeys,
    required this.onShowMemberProfile,
    required this.onScrollToHighlight,
    required this.onReplyChat,
    required this.onRetryChat,
    required this.onCopyChat,
    required this.onOpenReceive,
    required this.onRetryReceive,
  });
}

/// Canonical chat-bubble widget. Owns ALL the bubble chrome —
/// colors, alignment, max-width, author label, optional reply
/// quote, optional attachment panel, body text + timestamp +
/// status tick, optional action footer, hover/long-press actions
/// rail, failed-state retry.
///
/// Content-agnostic: callers fill the `attachment` / `text` /
/// `actions` slots; this widget never inspects them.
class ChatBubble extends StatefulWidget {
  final PublicKey author;
  final NostrProfile? authorProfile;
  final bool isMe;
  final DateTime timestamp;
  final MessageStatus status;
  final String? failureReason;

  /// Inline quoted message (chat-text only today; future variants
  /// can use it too).
  final Widget? replyQuote;

  /// Optional attachment panel rendered above the text — nested
  /// rounded container with `surface` color, distinct from the
  /// bubble background. Caller decides the height/content.
  final Widget? attachment;

  /// Body text. Empty string is valid — timestamp + status still
  /// render in their bottom-right slot.
  final String text;

  /// Optional action buttons rendered as a footer row inside the
  /// bubble (e.g. "Apply anyway", "Verify on device").
  final List<Widget> actions;

  /// Bubble-level tap. Falls back to `onRetry` for failed bubbles
  /// when null.
  final VoidCallback? onTap;

  // ----- standard hover / long-press actions -----
  final VoidCallback? onReply;
  final VoidCallback? onCopy;
  final VoidCallback? onRetry;
  final VoidCallback? onTapAvatar;
  final VoidCallback? onTapQuote;

  final bool isHighlighted;

  const ChatBubble({
    super.key,
    required this.author,
    required this.authorProfile,
    required this.isMe,
    required this.timestamp,
    required this.status,
    this.failureReason,
    this.replyQuote,
    this.attachment,
    required this.text,
    this.actions = const [],
    this.onTap,
    this.onReply,
    this.onCopy,
    this.onRetry,
    this.onTapAvatar,
    this.onTapQuote,
    this.isHighlighted = false,
  });

  @override
  State<ChatBubble> createState() => _ChatBubbleState();
}

class _ChatBubbleState extends State<ChatBubble> {
  bool _isHovered = false;

  bool _isMobile(BuildContext context) =>
      MediaQuery.of(context).size.width < 600;

  String _formatTime(DateTime time) {
    return '${time.hour.toString().padLeft(2, '0')}:${time.minute.toString().padLeft(2, '0')}';
  }

  Widget _buildBubbleContent(BuildContext context, ThemeData theme) {
    final isMe = widget.isMe;
    final isFailed = widget.status == MessageStatus.failed;

    final baseColor = isFailed
        ? theme.colorScheme.errorContainer
        : isMe
        ? theme.colorScheme.primaryContainer
        : theme.colorScheme.surfaceContainerHighest;

    return LayoutBuilder(
      builder: (context, constraints) {
        // Cap at 70% of the chat's available width (not the whole
        // screen — on desktop with a sidebar/tray, screen width is
        // much wider than what the chat actually owns), and never
        // exceed a comfortable reading width.
        final available = constraints.maxWidth.isFinite
            ? constraints.maxWidth
            : MediaQuery.of(context).size.width;
        final maxBubble = math.min(available * 0.7, 560.0);
        return ConstrainedBox(
          constraints: BoxConstraints(maxWidth: maxBubble),
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
                    getDisplayName(widget.authorProfile, widget.author),
                    style: theme.textTheme.labelSmall?.copyWith(
                      color: theme.colorScheme.primary,
                    ),
                  ),
                if (widget.replyQuote != null)
                  Padding(
                    padding: const EdgeInsets.only(bottom: 6),
                    child: widget.replyQuote!,
                  ),
                if (widget.attachment != null) ...[
                  Container(
                    margin: const EdgeInsets.only(top: 2, bottom: 6),
                    decoration: BoxDecoration(
                      color: theme.colorScheme.surface,
                      borderRadius: BorderRadius.circular(12),
                    ),
                    clipBehavior: Clip.antiAlias,
                    child: widget.attachment!,
                  ),
                ],
                // Body row: text + timestamp + status tick. Matches the
                // original _MessageBubble layout so single-line text
                // hugs the timestamp on its right.
                Row(
                  mainAxisSize: MainAxisSize.min,
                  crossAxisAlignment: CrossAxisAlignment.end,
                  children: [
                    if (widget.text.isNotEmpty)
                      Flexible(child: Text(widget.text)),
                    if (widget.text.isNotEmpty) const SizedBox(width: 8),
                    Padding(
                      padding: const EdgeInsets.only(bottom: 1),
                      child: Row(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Text(
                            _formatTime(widget.timestamp),
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
                if (widget.actions.isNotEmpty)
                  Padding(
                    padding: const EdgeInsets.only(top: 6),
                    child: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: widget.actions,
                    ),
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
      },
    );
  }

  Widget _buildStatusIndicator(ThemeData theme) {
    return switch (widget.status) {
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

  void _showMobileActions(BuildContext context) {
    final theme = Theme.of(context);
    final isFailed = widget.status == MessageStatus.failed;

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
                            if (isFailed && widget.onRetry != null)
                              _buildActionTile(
                                icon: Icons.refresh,
                                label: 'Retry',
                                color: theme.colorScheme.error,
                                onTap: () {
                                  Navigator.of(dialogContext).pop();
                                  widget.onRetry!();
                                },
                              ),
                            if (widget.onCopy != null)
                              _buildActionTile(
                                icon: Icons.copy,
                                label: 'Copy',
                                onTap: () {
                                  Navigator.of(dialogContext).pop();
                                  widget.onCopy!();
                                },
                              ),
                            if (widget.onReply != null)
                              _buildActionTile(
                                icon: Icons.reply,
                                label: 'Reply',
                                onTap: () {
                                  Navigator.of(dialogContext).pop();
                                  widget.onReply!();
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

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final isMe = widget.isMe;
    final isFailed = widget.status == MessageStatus.failed;
    final isMobile = _isMobile(context);
    final bubble = _buildBubbleContent(context, theme);

    final hoverActions = AnimatedOpacity(
      opacity: _isHovered ? 1.0 : 0.0,
      duration: const Duration(milliseconds: 150),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (isFailed && widget.onRetry != null)
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
          if (widget.onCopy != null)
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
          if (widget.onReply != null)
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
                profile: widget.authorProfile,
                pubkey: widget.author,
              ),
            ),
          )
        : null;

    final tap = isFailed ? (widget.onRetry ?? widget.onTap) : widget.onTap;
    // Only enable long-press when there's at least one action the
    // menu would show — otherwise the user gets an empty popup.
    final hasLongPressAction =
        widget.onCopy != null ||
        widget.onReply != null ||
        (isFailed && widget.onRetry != null);

    return Align(
      alignment: isMe ? Alignment.centerRight : Alignment.centerLeft,
      child: MouseRegion(
        onEnter: isMobile ? null : (_) => setState(() => _isHovered = true),
        onExit: isMobile ? null : (_) => setState(() => _isHovered = false),
        child: GestureDetector(
          onTap: tap,
          onLongPress: isMobile && hasLongPressAction
              ? () => _showMobileActions(context)
              : null,
          child: Padding(
            padding: const EdgeInsets.symmetric(vertical: 4),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: isMe
                  ? [
                      if (!isMobile) hoverActions,
                      if (!isMobile) const SizedBox(width: 4),
                      Flexible(child: bubble),
                    ]
                  : [
                      if (avatar != null) avatar,
                      Flexible(child: bubble),
                      if (!isMobile) const SizedBox(width: 4),
                      if (!isMobile) hoverActions,
                    ],
            ),
          ),
        ),
      ),
    );
  }
}

/// Attachment panel for a [TimelineReceiveAddress] bubble. Renders
/// inside [ChatBubble.attachment]'s rounded container — its job is
/// only the content: index, truncated address, verification badge.
/// Attachment panel for [TimelineReceiveAddress]. Renders the
/// derivation index and the locally-derived address — sender and
/// receiver both see the same content. The address is computed
/// from this wallet's descriptor; no "verified" claim is made
/// (verifying an address is fundamentally an out-of-band action).
class _ReceiveAttachment extends StatelessWidget {
  final int derivationIndex;

  const _ReceiveAttachment({required this.derivationIndex});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final walletCtx = WalletContext.of(context);
    final info = walletCtx?.superWallet.getAddressInfo(
      masterAppkey: walletCtx.masterAppkey,
      index: derivationIndex,
    );
    final address = info?.address.toString();

    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          Row(
            children: [
              Icon(
                Icons.call_received_rounded,
                size: 16,
                color: theme.colorScheme.onSurfaceVariant,
              ),
              const SizedBox(width: 6),
              Expanded(
                child: Text(
                  'Receive address',
                  style: theme.textTheme.labelSmall?.copyWith(
                    color: theme.colorScheme.onSurfaceVariant,
                    fontWeight: FontWeight.w600,
                  ),
                ),
              ),
              Icon(
                Icons.open_in_new_rounded,
                size: 14,
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ],
          ),
          const SizedBox(height: 6),
          Text(
            '#$derivationIndex',
            style: theme.textTheme.titleMedium?.copyWith(
              color: theme.colorScheme.primary,
              fontWeight: FontWeight.w600,
            ),
          ),
          const SizedBox(height: 4),
          // Full address, allowed to wrap.
          Text(
            address ?? '…',
            style: theme.textTheme.bodyMedium?.copyWith(
              fontFamily: 'monospace',
            ),
            softWrap: true,
          ),
        ],
      ),
    );
  }
}

/// Inline quoted reply for a chat-text bubble. Mirrors the original
/// `_MessageBubble._buildReplyQuote` so the shape is preserved.
/// Canonical "quoted-thing" header — left-bar accent, author label,
/// 2-line body with optional leading icon. Used both for chat reply
/// quotes and tx-card → receive-share quotes.
class _QuoteHeader extends StatelessWidget {
  final String label;
  final String body;
  final IconData? bodyIcon;
  final VoidCallback onTap;

  const _QuoteHeader({
    required this.label,
    required this.body,
    this.bodyIcon,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return GestureDetector(
      onTap: onTap,
      child: Container(
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
              label,
              style: theme.textTheme.labelSmall?.copyWith(
                color: theme.colorScheme.primary,
                fontWeight: FontWeight.w600,
              ),
            ),
            const SizedBox(height: 2),
            Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                if (bodyIcon != null) ...[
                  Icon(
                    bodyIcon,
                    size: 14,
                    color: theme.colorScheme.onSurfaceVariant,
                  ),
                  const SizedBox(width: 4),
                ],
                Flexible(
                  child: Text(
                    body,
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                    style: theme.textTheme.bodySmall?.copyWith(
                      color: theme.colorScheme.onSurfaceVariant,
                    ),
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

class _ChatReplyQuote extends StatelessWidget {
  final ChatMessage target;
  final NostrProfile? targetProfile;
  final VoidCallback onTap;

  const _ChatReplyQuote({
    required this.target,
    required this.targetProfile,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    return _QuoteHeader(
      label: target.isMe ? 'You' : getDisplayName(targetProfile, target.author),
      body: target.content,
      bodyIcon: target.quoteIcon,
      onTap: onTap,
    );
  }
}

class _TxLookupPlaceholder extends StatelessWidget {
  final String txid;
  final DateTime timestamp;
  const _TxLookupPlaceholder({required this.txid, required this.timestamp});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final short = txid.length > 12 ? '${txid.substring(0, 12)}…' : txid;
    return Align(
      alignment: Alignment.center,
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 360),
        child: Container(
          margin: const EdgeInsets.symmetric(vertical: 4),
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
          decoration: BoxDecoration(
            color: theme.colorScheme.surfaceContainerHighest,
            borderRadius: BorderRadius.circular(12),
          ),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(
                Icons.history,
                size: 18,
                color: theme.colorScheme.onSurfaceVariant,
              ),
              const SizedBox(width: 8),
              Text(
                'Syncing tx $short…',
                style: theme.textTheme.bodySmall?.copyWith(
                  color: theme.colorScheme.onSurfaceVariant,
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
