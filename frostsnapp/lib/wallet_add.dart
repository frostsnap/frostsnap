import 'dart:typed_data';
import 'package:flutter/material.dart';
import 'package:frostsnap/animated_check.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/fullscreen_dialog_scaffold.dart';
import 'package:frostsnap/maybe_fullscreen_dialog.dart';
import 'package:frostsnap/invite_link_input.dart';
import 'package:frostsnap/join_link.dart';
import 'package:frostsnap/nostr_chat/nostr_state.dart';
import 'package:frostsnap/recovery/remote_recovery_create_page.dart';
import 'package:frostsnap/restoration.dart';
import 'package:frostsnap/secure_key_provider.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/nostr.dart';
import 'package:frostsnap/src/rust/lib.dart';
import 'package:frostsnap/org_keygen_page.dart';
import 'package:frostsnap/wallet_create.dart';

enum AddType { newWallet, recoverWallet, remoteRecoverWallet, joinFromLink }

enum VerticalButtonGroupPosition { top, bottom, middle, single }

class WalletAddColumn extends StatelessWidget {
  static const iconSize = 24.0;
  static const cardMargin = EdgeInsets.fromLTRB(16, 4, 16, 4);
  static const cardBorder = BorderRadius.all(Radius.circular(28));
  static const cardBorderTop = BorderRadius.only(
    topLeft: Radius.circular(28),
    topRight: Radius.circular(28),
    bottomLeft: Radius.circular(8),
    bottomRight: Radius.circular(8),
  );
  static const cardBorderBottom = BorderRadius.only(
    topLeft: Radius.circular(8),
    topRight: Radius.circular(8),
    bottomLeft: Radius.circular(28),
    bottomRight: Radius.circular(28),
  );
  static const cardBorderMiddle = BorderRadius.all(Radius.circular(8));
  static const contentPadding = EdgeInsets.symmetric(horizontal: 16);

  final bool showNewToFrostsnap;
  final Function(AddType) onPressed;

  WalletAddColumn({
    super.key,
    this.showNewToFrostsnap = true,
    required this.onPressed,
  });

  @override
  Widget build(BuildContext context) {
    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        if (showNewToFrostsnap) buildTitle(context, text: 'Create wallet'),
        buildCard(
          context,
          action: () => onPressed(AddType.newWallet),
          emphasize: true,
          isThreeLine: true,
          icon: Icon(Icons.add_rounded, size: iconSize),
          title: 'Create a multi-sig wallet',
          subtitle: 'Set up a secure wallet using multiple Frostsnap devices',
        ),
        buildTitle(context, text: 'Restore wallet'),
        buildCard(
          context,
          action: () => onPressed(AddType.recoverWallet),
          groupPosition: VerticalButtonGroupPosition.top,
          isThreeLine: true,
          icon: Icon(Icons.restore_rounded, size: iconSize),
          title: 'Restore wallet',
          subtitle: 'Use an existing device key or load a physical backup',
        ),
        buildCard(
          context,
          action: () => onPressed(AddType.remoteRecoverWallet),
          groupPosition: VerticalButtonGroupPosition.bottom,
          isThreeLine: true,
          icon: Icon(Icons.groups_rounded, size: iconSize),
          title: 'Start a recovery lobby',
          subtitle: 'Invite share holders to help recover a wallet over nostr',
        ),
        buildCard(
          context,
          action: () => onPressed(AddType.joinFromLink),
          isThreeLine: true,
          icon: Icon(Icons.link_rounded, size: iconSize),
          title: 'Join with invite link',
          subtitle: 'Wallet, keygen, or recovery — the link tells us which',
        ),
      ],
    );
  }

  static Widget buildTitle(
    BuildContext context, {
    required String text,
    String? subText,
    bool showInfoIcon = false,
    Widget? trailing,
  }) {
    final theme = Theme.of(context);
    return ListTile(
      dense: true,
      contentPadding: EdgeInsets.symmetric(horizontal: 16).copyWith(top: 12),
      title: Text.rich(
        TextSpan(
          text: text,
          children: showInfoIcon
              ? [
                  TextSpan(text: ' '),
                  WidgetSpan(
                    child: Icon(
                      Icons.info_outline_rounded,
                      size: 16,
                      color: theme.colorScheme.secondary,
                    ),
                  ),
                ]
              : null,
          style: TextStyle(
            color: theme.colorScheme.secondary,
            fontWeight: FontWeight.bold,
          ),
        ),
      ),
      subtitle: subText == null ? null : Text(subText),
      trailing: trailing,
      subtitleTextStyle: theme.textTheme.labelSmall?.copyWith(
        color: theme.colorScheme.secondary,
      ),
    );
  }

  static Widget buildCard(
    BuildContext context, {
    required Widget icon,
    required String title,
    required String subtitle,
    VerticalButtonGroupPosition? groupPosition,
    String? subsubtitle,
    bool emphasize = false,
    bool? isThreeLine,
    Function()? action,
  }) {
    final theme = Theme.of(context);
    final Color? emphasisColor = theme.colorScheme.secondary;
    final Color? onEmphasisColor = theme.colorScheme.onSecondary;
    final Color? color = theme.colorScheme.secondaryContainer;
    final Color? onColor = theme.colorScheme.onSecondaryContainer;

    final listTile = ListTile(
      textColor: emphasize ? onEmphasisColor : onColor,
      iconColor: emphasize ? onEmphasisColor : onColor,
      onTap: action,
      contentPadding: EdgeInsets.symmetric(horizontal: 16, vertical: 8),
      leading: icon,
      trailing: Icon(Icons.chevron_right_rounded),
      title: Text(title),
      isThreeLine: isThreeLine,
      subtitle: Text.rich(
        TextSpan(
          text: subtitle,
          children: subsubtitle == null
              ? null
              : [
                  TextSpan(text: '\n'),
                  TextSpan(
                    text: subsubtitle,
                    style: TextStyle(
                      fontStyle: FontStyle.italic,
                      color: theme.colorScheme.outline,
                      fontSize: 13,
                    ),
                  ),
                ],
        ),
      ),
    );

    return Card.filled(
      color: emphasize ? emphasisColor : color,
      shape: RoundedRectangleBorder(
        borderRadius: switch (groupPosition) {
          null => cardBorder,
          VerticalButtonGroupPosition.top => cardBorderTop,
          VerticalButtonGroupPosition.bottom => cardBorderBottom,
          VerticalButtonGroupPosition.middle => cardBorderMiddle,
          VerticalButtonGroupPosition.single => cardBorder,
        },
      ),
      clipBehavior: Clip.hardEdge,
      margin:
          (groupPosition == VerticalButtonGroupPosition.top ||
              groupPosition == VerticalButtonGroupPosition.middle)
          ? cardMargin.copyWith(bottom: 0)
          : cardMargin,
      child: listTile,
    );
  }

  static void showWalletCreateDialog(BuildContext context) async {
    final homeCtx = HomeContext.of(context)!;

    // Use the shared NostrContext client so OrgKeygenPage's lobby
    // create/join paths reach the same `local_publish` snapshot that
    // identity mutations push to — without this, the lobby's
    // auto-publish-on-connect always sees None on a freshly-built
    // per-page client.
    final nostrClient = await NostrContext.of(context).nostrClient;
    if (!context.mounted) return;
    final choice = await MaybeFullscreenDialog.show<WalletTypeChoice>(
      context: context,
      // tap-outside would silently drop an in-progress lobby without
      // telling peers; force users through explicit back/cancel.
      barrierDismissible: false,
      child: OrgKeygenPage(nostrClient: nostrClient),
    );
    if (!context.mounted || choice == null) return;

    AccessStructureRef? asRef;
    switch (choice) {
      case WalletTypeChoicePersonal():
        asRef = await MaybeFullscreenDialog.show<AccessStructureRef>(
          context: context,
          barrierDismissible: false,
          child: WalletCreatePage(),
        );
      case WalletTypeChoiceOrganisation(:final accessStructureRef):
        asRef = accessStructureRef;
    }

    if (!context.mounted || asRef == null) return;
    // Wait for the user to unplug devices before opening the wallet
    // (no-op if nothing's connected).
    await showUnplugDevicesDialog(context);
    if (!context.mounted) return;
    homeCtx.openNewlyCreatedWallet(asRef.keyId);
  }

  static void showWalletRecoverDialog(BuildContext context) async {
    final homeCtx = HomeContext.of(context)!;

    final restorationId = await MaybeFullscreenDialog.show<RestorationId>(
      context: context,
      barrierDismissible: true,
      child: RecoveryFlowWithDiscovery(
        recoveryContext: RecoveryContext.newRestoration(),
      ),
    );

    await coord.cancelProtocol();
    if (restorationId == null) return;
    homeCtx.walletListController.selectRecoveringWallet(restorationId);
  }

  static Future<void> showRemoteRecoveryDialog(BuildContext context) async {
    final homeCtx = HomeContext.of(context)!;
    final nostrClient = await NostrContext.of(context).nostrClient;
    if (!context.mounted) return;
    final asRef = await MaybeFullscreenDialog.show<AccessStructureRef>(
      context: context,
      barrierDismissible: false,
      child: RemoteRecoveryCreatePage(coord: coord, nostrClient: nostrClient),
    );
    if (asRef == null || !context.mounted) return;
    await showUnplugDevicesDialog(context);
    if (!context.mounted) return;
    homeCtx.openNewlyCreatedWallet(asRef.keyId);
  }

  static void showJoinFromLinkDialog(
    BuildContext context, {
    String? initialLink,
  }) async {
    final homeCtx = HomeContext.of(context)!;
    final keyId = await MaybeFullscreenDialog.show<KeyId>(
      context: context,
      child: JoinLinkPage(initialLink: initialLink),
    );
    if (keyId != null && context.mounted) {
      homeCtx.openNewlyCreatedWallet(keyId);
    }
  }

  static void showAddKeyDialog(
    BuildContext context,
    AccessStructureRef accessStructureRef,
  ) async {
    await MaybeFullscreenDialog.show<RestorationId>(
      context: context,
      child: RecoveryFlowWithDiscovery(
        recoveryContext: RecoveryContext.addingToWallet(
          accessStructureRef: accessStructureRef,
        ),
      ),
    );
    await coord.cancelProtocol();
  }
}

Function(AddType) makeOnPressed(BuildContext context) {
  return (addType) {
    switch (addType) {
      case AddType.newWallet:
        WalletAddColumn.showWalletCreateDialog(context);
      case AddType.recoverWallet:
        WalletAddColumn.showWalletRecoverDialog(context);
      case AddType.remoteRecoverWallet:
        WalletAddColumn.showRemoteRecoveryDialog(context);
      case AddType.joinFromLink:
        WalletAddColumn.showJoinFromLinkDialog(context);
    }
  };
}

enum _JoinState { input, loading, success, error }

/// Universal join-via-link entry point. Accepts any `frostsnap://…`
/// invite URL — wallet channel, remote keygen lobby, or remote
/// recovery lobby — and dispatches to the matching downstream flow
/// via the classifier in `join_link.dart`.
class JoinLinkPage extends StatefulWidget {
  final String? initialLink;

  const JoinLinkPage({super.key, this.initialLink});

  @override
  State<JoinLinkPage> createState() => _JoinLinkPageState();
}

class _JoinLinkPageState extends State<JoinLinkPage> {
  late final TextEditingController _controller;
  _JoinState _state = _JoinState.input;
  String? _error;
  String? _walletName;
  KeyId? _keyId;
  String? _invalidLinkError;

  @override
  void initState() {
    super.initState();
    _controller = TextEditingController(text: widget.initialLink ?? '');
    if (widget.initialLink != null) {
      WidgetsBinding.instance.addPostFrameCallback((_) => _submit());
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  ChannelSecret _parseChannelSecret(String link) {
    final hexStr = link.replaceFirst('frostsnap://channel/', '');
    if (hexStr.length != 32) throw 'Invalid link: expected 32 hex characters';
    final bytes = Uint8List(16);
    for (var i = 0; i < 16; i++) {
      bytes[i] = int.parse(hexStr.substring(i * 2, i * 2 + 2), radix: 16);
    }
    return ChannelSecret(field0: U8Array16(bytes));
  }

  Future<void> _submit() async {
    final link = _controller.text.trim();
    if (link.isEmpty) return;

    final kind = classifyJoinLink(link);
    if (kind == LinkKind.unknown) {
      setState(
        () => _invalidLinkError = 'Not a valid frostsnap:// invite link',
      );
      return;
    }

    setState(() {
      _state = _JoinState.loading;
      _error = null;
      _invalidLinkError = null;
    });

    try {
      final KeyId? keyId;
      switch (kind) {
        case LinkKind.channel:
          keyId = await _joinChannel(link);
        case LinkKind.keygen:
          keyId = await _joinKeygen(link);
        case LinkKind.recovery:
          keyId = await _joinRecovery(link);
        case LinkKind.unknown:
          throw StateError('unreachable — filtered above');
      }
      if (keyId == null) {
        // User cancelled mid-dispatch (recovery/keygen returned null).
        // Return to input so they can try a different link or exit.
        if (mounted) setState(() => _state = _JoinState.input);
        return;
      }
      _keyId = keyId;
      _walletName = coord.getFrostKey(keyId: keyId)?.keyName() ?? 'Wallet';
      if (!mounted) return;
      setState(() => _state = _JoinState.success);
      await Future.delayed(const Duration(milliseconds: 1200));
      if (mounted) Navigator.pop(context, _keyId);
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _state = _JoinState.error;
        _error = e.toString();
      });
    }
  }

  Future<KeyId?> _joinChannel(String link) async {
    final encryptionKey = await SecureKeyProvider.getEncryptionKey();
    if (!mounted) return null;
    final client = await NostrContext.of(context).nostrClient;
    if (!mounted) return null;
    final secret = _parseChannelSecret(link);
    return client.joinFromLink(
      coord: coord,
      channelSecret: secret,
      encryptionKey: encryptionKey,
    );
  }

  Future<KeyId?> _joinKeygen(String link) async {
    final client = await NostrContext.of(context).nostrClient;
    if (!mounted) return null;
    final asRef = await OrgKeygenPage.dispatchKeygenJoin(
      context: context,
      nostrClient: client,
      link: link,
    );
    return asRef?.keyId;
  }

  Future<KeyId?> _joinRecovery(String link) async {
    final client = await NostrContext.of(context).nostrClient;
    if (!mounted) return null;
    final asRef = await RemoteRecoveryCreatePage.dispatchJoin(
      context: context,
      coord: coord,
      nostrClient: client,
      link: link,
    );
    return asRef?.keyId;
  }

  @override
  Widget build(BuildContext context) {
    return switch (_state) {
      _JoinState.input => _buildInputStep(context),
      _JoinState.loading => _buildLoadingStep(context),
      _JoinState.success => _buildSuccessStep(context),
      _JoinState.error => _buildErrorStep(context),
    };
  }

  MultiStepDialogScaffold _buildInputStep(BuildContext context) {
    final theme = Theme.of(context);
    return MultiStepDialogScaffold(
      stepKey: _JoinState.input,
      title: const Text('Join with invite link'),
      showClose: true,
      body: SliverToBoxAdapter(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              Icons.link_rounded,
              size: 64,
              color: theme.colorScheme.primary,
            ),
            const SizedBox(height: 24),
            Text(
              'Paste or scan any frostsnap invite — a wallet, a keygen '
              'session, or a recovery lobby.',
              style: theme.textTheme.bodyLarge,
              textAlign: TextAlign.center,
            ),
            const SizedBox(height: 24),
            InviteLinkInput(
              controller: _controller,
              onSubmit: _submit,
              errorText: _invalidLinkError,
              hintText: 'frostsnap://…',
            ),
          ],
        ),
      ),
      footer: Align(
        alignment: Alignment.centerRight,
        child: FilledButton.icon(
          onPressed: _submit,
          icon: const Icon(Icons.login_rounded),
          label: const Text('Join'),
        ),
      ),
    );
  }

  MultiStepDialogScaffold _buildLoadingStep(BuildContext context) {
    final theme = Theme.of(context);
    return MultiStepDialogScaffold(
      stepKey: _JoinState.loading,
      title: const Text('Joining…'),
      showClose: true,
      body: SliverToBoxAdapter(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const SizedBox(
              width: 64,
              height: 64,
              child: CircularProgressIndicator(strokeWidth: 3),
            ),
            const SizedBox(height: 24),
            Text(
              'Joining session…',
              style: theme.textTheme.headlineSmall,
              textAlign: TextAlign.center,
            ),
            const SizedBox(height: 8),
            const Text('Contacting nostr relays', textAlign: TextAlign.center),
          ],
        ),
      ),
    );
  }

  MultiStepDialogScaffold _buildSuccessStep(BuildContext context) {
    final theme = Theme.of(context);
    return MultiStepDialogScaffold(
      stepKey: _JoinState.success,
      title: const Text('Joined'),
      body: SliverToBoxAdapter(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const AnimatedCheckCircle(size: 64),
            const SizedBox(height: 24),
            Text(
              _walletName ?? 'Wallet',
              style: theme.textTheme.headlineSmall,
              textAlign: TextAlign.center,
            ),
            const SizedBox(height: 8),
            const Text('Joined successfully', textAlign: TextAlign.center),
          ],
        ),
      ),
    );
  }

  MultiStepDialogScaffold _buildErrorStep(BuildContext context) {
    final theme = Theme.of(context);
    return MultiStepDialogScaffold(
      stepKey: _JoinState.error,
      title: const Text('Error'),
      showClose: true,
      body: SliverToBoxAdapter(
        child: Container(
          decoration: BoxDecoration(
            color: theme.colorScheme.errorContainer,
            borderRadius: BorderRadius.circular(12),
          ),
          padding: const EdgeInsets.all(24),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(
                Icons.error_outline_rounded,
                size: 64,
                color: theme.colorScheme.onErrorContainer,
              ),
              const SizedBox(height: 24),
              Text(
                'Failed to join',
                style: theme.textTheme.headlineSmall?.copyWith(
                  color: theme.colorScheme.onErrorContainer,
                ),
                textAlign: TextAlign.center,
              ),
              const SizedBox(height: 16),
              Text(
                _error ?? 'Unknown error',
                style: TextStyle(color: theme.colorScheme.onErrorContainer),
                textAlign: TextAlign.center,
              ),
            ],
          ),
        ),
      ),
      footer: Align(
        alignment: Alignment.centerRight,
        child: FilledButton.icon(
          onPressed: () => setState(() => _state = _JoinState.input),
          icon: const Icon(Icons.refresh),
          label: const Text('Try Again'),
          style: FilledButton.styleFrom(
            backgroundColor: theme.colorScheme.error,
            foregroundColor: theme.colorScheme.onError,
          ),
        ),
      ),
    );
  }
}
