import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnap/nostr_chat/group_info_page.dart';
import 'package:frostsnap/nostr_chat/nostr_profile.dart';
import 'package:frostsnap/nostr_chat/nostr_state.dart';
import 'package:frostsnap/nostr_chat/setup_dialog.dart';
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
  List<PublicKey> _memberPubkeys = [];
  StreamSubscription<FfiChannelEvent>? _subscription;
  NostrClient? _client;
  FfiConnectionState _connectionState = const FfiConnectionState.connecting();
  ChatMessage? _replyingTo;

  NostrContext? _nostrContext;
  PublicKey? get _myPubkey => _nostrContext?.myPubkey;
  bool _didConnect = false;

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
    _nostrContext = NostrContext.of(context);
    if (!_didConnect) {
      _didConnect = true;
      _loadPubkeyAndConnect();
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

  Future<void> _loadPubkeyAndConnect() async {
    try {
      final hasIdentity = _nostrContext!.nostrSettings.hasIdentity();
      if (!hasIdentity) {
        if (!mounted) return;
        final result = await showNostrSetupDialog(context);
        if (result == NostrSetupResult.cancelled || !mounted) {
          Navigator.of(context).pop();
          return;
        }
      }
      _connect();
    } catch (e) {
      debugPrint('Error loading nostr identity: $e');
      if (!mounted) return;
      final result = await showNostrSetupDialog(context);
      if (result == NostrSetupResult.cancelled || !mounted) {
        Navigator.of(context).pop();
        return;
      }
      _connect();
    }
  }

  Future<void> _connect() async {
    _client = await NostrClient.connect();
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
      }
    });
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
    if (content.isEmpty || _client == null) return;

    final nsec = _nostrContext!.nostrSettings.getNsec();
    final replyToId = _replyingTo?.messageId;
    _messageController.clear();
    setState(() => _replyingTo = null);

    await _client!.sendMessage(
      keyId: widget.keyId,
      nsec: nsec,
      content: content,
      replyTo: replyToId,
    );
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
          Expanded(
            child: _messages.isEmpty
                ? Center(
                    child: _connectionState is FfiConnectionState_Connected
                        ? Text(
                            'No messages yet.\nSend a message to start the conversation.',
                            textAlign: TextAlign.center,
                            style: theme.textTheme.bodyMedium?.copyWith(
                              color: theme.colorScheme.onSurfaceVariant,
                            ),
                          )
                        : const CircularProgressIndicator(),
                  )
                : ListView.builder(
                    controller: _scrollController,
                    padding: const EdgeInsets.all(16),
                    itemCount: _messages.length,
                    itemBuilder: (context, index) {
                      final message = _messages[index];
                      return _MessageBubble(
                        message: message,
                        profile: _getProfile(message.author),
                        replyToMessage: message.replyTo != null
                            ? _messageById[message.replyTo]
                            : null,
                        onReply: () => _startReply(message),
                        onRetry: () => _retryMessage(message),
                        onCopy: () => _copyMessage(message),
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
