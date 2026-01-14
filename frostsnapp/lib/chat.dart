import 'dart:async';
import 'package:flutter/material.dart';
import 'package:frostsnap/src/rust/api/nostr.dart';
import 'package:frostsnap/src/rust/api.dart';

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
  final String nsec;

  const ChatPage({
    super.key,
    required this.keyId,
    required this.walletName,
    required this.nsec,
  });

  @override
  State<ChatPage> createState() => _ChatPageState();
}

class _ChatPageState extends State<ChatPage> {
  final TextEditingController _messageController = TextEditingController();
  final ScrollController _scrollController = ScrollController();
  final FocusNode _inputFocusNode = FocusNode();
  final List<ChatMessage> _messages = [];
  final Map<NostrEventId, ChatMessage> _messageById = {};
  StreamSubscription<FfiChannelEvent>? _subscription;
  NostrChannelHandle? _handle;
  FfiConnectionState _connectionState = const FfiConnectionState.connecting();
  String? _channelName;
  late final Nsec _nsec;
  late final PublicKey _myPubkey;
  ChatMessage? _replyingTo;

  @override
  void initState() {
    super.initState();
    _nsec = Nsec.parse(s: widget.nsec);
    _myPubkey = _nsec.publicKey();
    _connect();
  }

  Future<void> _connect() async {
    final relays = defaultRelayUrls();
    final stream = connectToChannel(
      keyId: widget.keyId,
      nsec: _nsec,
      relayUrls: relays,
    );

    _subscription = stream.listen(_handleEvent);
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
          final isMe = author.equals(other: _myPubkey);
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
          WidgetsBinding.instance.addPostFrameCallback((_) => _scrollToBottom());

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

        case FfiChannelEvent_ChannelMetadata(:final name, about: _):
          _channelName = name;

        case FfiChannelEvent_ConnectionState(:final field0):
          _connectionState = field0;
          if (field0 is FfiConnectionState_Connected) {
            _initializeChannel();
          }
      }
    });
  }

  Future<void> _initializeChannel() async {
    _handle = await getChannelHandle(keyId: widget.keyId);
    await _handle?.initializeChannel(walletName: widget.walletName);
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
    if (content.isEmpty) return;

    _handle ??= await getChannelHandle(keyId: widget.keyId);
    if (_handle == null) return;

    final replyToId = _replyingTo?.messageId;
    _messageController.clear();
    setState(() => _replyingTo = null);

    await _handle!.sendMessage(content: content, replyTo: replyToId);
    _inputFocusNode.requestFocus();
  }

  Future<void> _retryMessage(ChatMessage message) async {
    if (message.status != MessageStatus.failed) return;

    _handle ??= await getChannelHandle(keyId: widget.keyId);
    if (_handle == null) return;

    setState(() {
      _messages.remove(message);
      _messageById.remove(message.messageId);
    });

    await _handle!.sendMessage(
      content: message.content,
      replyTo: message.replyTo,
    );
  }

  void _startReply(ChatMessage message) {
    setState(() => _replyingTo = message);
    _inputFocusNode.requestFocus();
  }

  void _cancelReply() {
    setState(() => _replyingTo = null);
  }

  @override
  void dispose() {
    _subscription?.cancel();
    disconnectChannel(keyId: widget.keyId);
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
        title: Text(_channelName ?? widget.walletName),
        actions: [
          _buildConnectionIndicator(),
          const SizedBox(width: 8),
        ],
      ),
      body: Column(
        children: [
          Expanded(
            child: _messages.isEmpty
                ? Center(
                    child: Text(
                      'No messages yet.\nSend a message to start the conversation.',
                      textAlign: TextAlign.center,
                      style: theme.textTheme.bodyMedium?.copyWith(
                        color: theme.colorScheme.onSurfaceVariant,
                      ),
                    ),
                  )
                : ListView.builder(
                    controller: _scrollController,
                    padding: const EdgeInsets.all(16),
                    itemCount: _messages.length,
                    itemBuilder: (context, index) {
                      final message = _messages[index];
                      return _MessageBubble(
                        message: message,
                        replyToMessage: message.replyTo != null
                            ? _messageById[message.replyTo]
                            : null,
                        onReply: () => _startReply(message),
                        onRetry: () => _retryMessage(message),
                      );
                    },
                  ),
          ),
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
          'Disconnected${reason != null ? ': $reason' : ''}'
        ),
    };

    return Tooltip(
      message: tooltip,
      child: Container(
        width: 12,
        height: 12,
        decoration: BoxDecoration(
          color: color,
          shape: BoxShape.circle,
        ),
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
                  top: BorderSide(color: theme.colorScheme.outline.withValues(alpha: 0.2)),
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
                              : 'Replying to ${_shortenPubkey(_replyingTo!.author.toHex())}',
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
          Padding(
            padding: const EdgeInsets.all(8),
            child: Row(
              children: [
                Expanded(
                  child: TextField(
                    controller: _messageController,
                    focusNode: _inputFocusNode,
                    autofocus: true,
                    enabled: isConnected,
                    decoration: InputDecoration(
                      hintText: isConnected ? 'Type a message...' : 'Connecting...',
                      border: OutlineInputBorder(
                        borderRadius: BorderRadius.circular(24),
                      ),
                      contentPadding: const EdgeInsets.symmetric(
                        horizontal: 16,
                        vertical: 8,
                      ),
                    ),
                    textInputAction: TextInputAction.send,
                    onSubmitted: (_) => _sendMessage(),
                  ),
                ),
                const SizedBox(width: 8),
                IconButton(
                  onPressed: isConnected ? _sendMessage : null,
                  icon: const Icon(Icons.send),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }

  String _shortenPubkey(String pubkey) {
    if (pubkey.length <= 12) return pubkey;
    return '${pubkey.substring(0, 6)}...${pubkey.substring(pubkey.length - 6)}';
  }
}

class _MessageBubble extends StatefulWidget {
  final ChatMessage message;
  final ChatMessage? replyToMessage;
  final VoidCallback onReply;
  final VoidCallback onRetry;

  const _MessageBubble({
    required this.message,
    required this.replyToMessage,
    required this.onReply,
    required this.onRetry,
  });

  @override
  State<_MessageBubble> createState() => _MessageBubbleState();
}

class _MessageBubbleState extends State<_MessageBubble> {
  bool _isHovered = false;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final isMe = widget.message.isMe;
    final isFailed = widget.message.status == MessageStatus.failed;

    final bubble = Container(
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
              _shortenPubkey(widget.message.author.toHex()),
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

    final hoverActions = AnimatedOpacity(
      opacity: _isHovered ? 1.0 : 0.0,
      duration: const Duration(milliseconds: 150),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (isFailed)
            IconButton(
              icon: Icon(Icons.refresh, size: 18, color: theme.colorScheme.error),
              onPressed: widget.onRetry,
              padding: EdgeInsets.zero,
              constraints: const BoxConstraints(),
              splashRadius: 14,
              tooltip: 'Retry',
            ),
          IconButton(
            icon: Icon(Icons.reply, size: 18, color: theme.colorScheme.onSurfaceVariant),
            onPressed: widget.onReply,
            padding: EdgeInsets.zero,
            constraints: const BoxConstraints(),
            splashRadius: 14,
            tooltip: 'Reply',
          ),
        ],
      ),
    );

    return Align(
      alignment: isMe ? Alignment.centerRight : Alignment.centerLeft,
      child: MouseRegion(
        onEnter: (_) => setState(() => _isHovered = true),
        onExit: (_) => setState(() => _isHovered = false),
        child: GestureDetector(
          onTap: isFailed ? widget.onRetry : null,
          child: Padding(
            padding: const EdgeInsets.symmetric(vertical: 4),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: isMe
                  ? [hoverActions, const SizedBox(width: 4), bubble]
                  : [bubble, const SizedBox(width: 4), hoverActions],
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
          left: BorderSide(
            color: theme.colorScheme.primary,
            width: 2,
          ),
        ),
        color: theme.colorScheme.surface.withValues(alpha: 0.5),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            replyTo.isMe ? 'You' : _shortenPubkey(replyTo.author.toHex()),
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

  String _shortenPubkey(String pubkey) {
    if (pubkey.length <= 12) return pubkey;
    return '${pubkey.substring(0, 6)}...${pubkey.substring(pubkey.length - 6)}';
  }

  String _formatTime(DateTime time) {
    return '${time.hour.toString().padLeft(2, '0')}:${time.minute.toString().padLeft(2, '0')}';
  }
}
