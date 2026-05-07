import 'dart:typed_data';
import 'package:flutter/material.dart';
import 'package:frostsnap/animated_check.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/fullscreen_dialog_scaffold.dart';
import 'package:frostsnap/maybe_fullscreen_dialog.dart';
import 'package:frostsnap/restoration.dart';
import 'package:frostsnap/secure_key_provider.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/nostr.dart';
import 'package:frostsnap/src/rust/lib.dart';
import 'package:frostsnap/org_keygen_page.dart';
import 'package:frostsnap/wallet_create.dart';

enum AddType { newWallet, recoverWallet, joinFromLink }

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
          isThreeLine: true,
          icon: Icon(Icons.restore_rounded, size: iconSize),
          title: 'Restore wallet',
          subtitle: 'Use an existing device key or load a physical backup',
        ),
        buildCard(
          context,
          action: () => onPressed(AddType.joinFromLink),
          isThreeLine: true,
          icon: Icon(Icons.link_rounded, size: iconSize),
          title: 'Join wallet from link',
          subtitle: 'Join an existing wallet using a shared nostr link',
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

    // Step 1 of the org-keygen redesign mockup: Personal vs Organisation.
    // Organisation continues inside OrgKeygenPage (sessionRole → join |
    // name → lobby → review → TODO start_keygen); Personal pops back
    // here with `personal`, and we fall through to the existing local
    // keygen dialog.
    final nostrClient = await NostrClient.connect();
    if (!context.mounted) return;
    final choice = await MaybeFullscreenDialog.show<WalletTypeChoice>(
      context: context,
      // Dismissing by tap-outside would silently drop an in-progress
      // lobby without telling peers. Force the user through the
      // explicit back / cancel paths, which publish `CancelLobby` for
      // the host so joiners get evicted too.
      barrierDismissible: false,
      child: OrgKeygenPage(nostrClient: nostrClient),
    );
    if (!context.mounted || choice != WalletTypeChoice.personal) return;

    final asRef = await MaybeFullscreenDialog.show<AccessStructureRef>(
      context: context,
      barrierDismissible: false,
      child: WalletCreatePage(),
    );

    if (!context.mounted || asRef == null) return;
    // Wallet-create page has popped. Wait for the user to unplug the
    // devices (no-op if nothing is connected) before opening the new wallet.
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

  static void showJoinFromLinkDialog(
    BuildContext context, {
    String? initialLink,
  }) async {
    final homeCtx = HomeContext.of(context)!;
    final keyId = await MaybeFullscreenDialog.show<KeyId>(
      context: context,
      child: JoinFromLinkPage(initialLink: initialLink),
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
      case AddType.joinFromLink:
        WalletAddColumn.showJoinFromLinkDialog(context);
    }
  };
}

enum _JoinState { input, loading, success, error }

class JoinFromLinkPage extends StatefulWidget {
  final String? initialLink;

  const JoinFromLinkPage({super.key, this.initialLink});

  @override
  State<JoinFromLinkPage> createState() => _JoinFromLinkPageState();
}

class _JoinFromLinkPageState extends State<JoinFromLinkPage> {
  late final TextEditingController _controller;
  _JoinState _state = _JoinState.input;
  String? _error;
  KeyId? _keyId;
  String? _walletName;

  @override
  void initState() {
    super.initState();
    _controller = TextEditingController(text: widget.initialLink ?? '');
    if (widget.initialLink != null) {
      WidgetsBinding.instance.addPostFrameCallback((_) => _join());
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  ChannelSecret _parseLink(String link) {
    final hexStr = link.replaceFirst('frostsnap://channel/', '');
    if (hexStr.length != 32) throw 'Invalid link: expected 32 hex characters';
    final bytes = Uint8List(16);
    for (var i = 0; i < 16; i++) {
      bytes[i] = int.parse(hexStr.substring(i * 2, i * 2 + 2), radix: 16);
    }
    return ChannelSecret(field0: U8Array16(bytes));
  }

  Future<void> _join() async {
    final link = _controller.text.trim();
    if (link.isEmpty) return;

    setState(() {
      _state = _JoinState.loading;
      _error = null;
    });

    try {
      final channelSecret = _parseLink(link);
      final encryptionKey = await SecureKeyProvider.getEncryptionKey();
      final client = await NostrClient.connect();
      final keyId = await client.joinFromLink(
        coord: coord,
        channelSecret: channelSecret,
        encryptionKey: encryptionKey,
      );
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

  @override
  Widget build(BuildContext context) {
    return MultiStepDialogScaffold(
      body: KeyedSubtree(
        key: ValueKey(_state),
        child: switch (_state) {
          _JoinState.input => _buildInput(context),
          _JoinState.loading => _buildLoading(context),
          _JoinState.success => _buildSuccess(context),
          _JoinState.error => _buildError(context),
        },
      ),
      footer: _buildFooter(context),
    );
  }

  Widget? _buildFooter(BuildContext context) {
    final theme = Theme.of(context);
    return switch (_state) {
      _JoinState.input => Align(
        alignment: Alignment.centerRight,
        child: FilledButton.icon(
          onPressed: _join,
          icon: const Icon(Icons.login_rounded),
          label: const Text('Join'),
        ),
      ),
      _JoinState.loading => null,
      _JoinState.success => null,
      _JoinState.error => Align(
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
    };
  }

  Widget _buildInput(BuildContext context) {
    final theme = Theme.of(context);
    return FullscreenDialogBody(
      title: const Text('Join Wallet'),
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
              'Paste an invite link to join an existing wallet.',
              style: theme.textTheme.bodyLarge,
              textAlign: TextAlign.center,
            ),
            const SizedBox(height: 24),
            TextField(
              controller: _controller,
              decoration: const InputDecoration(
                hintText: 'frostsnap://channel/...',
                border: OutlineInputBorder(),
                prefixIcon: Icon(Icons.link),
              ),
              autofocus: true,
              onSubmitted: (_) => _join(),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildLoading(BuildContext context) {
    final theme = Theme.of(context);
    return FullscreenDialogBody(
      title: const Text('Joining…'),
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
              'Joining wallet…',
              style: theme.textTheme.headlineSmall,
              textAlign: TextAlign.center,
            ),
            const SizedBox(height: 8),
            const Text(
              'Fetching wallet data from relays',
              textAlign: TextAlign.center,
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildSuccess(BuildContext context) {
    final theme = Theme.of(context);
    return FullscreenDialogBody(
      title: const Text('Joined'),
      showClose: false,
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
            const Text(
              'Wallet joined successfully',
              textAlign: TextAlign.center,
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildError(BuildContext context) {
    final theme = Theme.of(context);
    return FullscreenDialogBody(
      title: const Text('Error'),
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
                'Failed to join wallet',
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
    );
  }
}
