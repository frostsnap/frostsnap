import 'package:flutter/material.dart';
import 'package:frostsnap/camera/camera.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/maybe_fullscreen_dialog.dart';
import 'package:frostsnap/src/rust/api/address_search.dart';
import 'package:frostsnap/src/rust/api/bitcoin.dart';
import 'package:frostsnap/src/rust/api/super_wallet.dart';
import 'package:frostsnap/theme.dart';
import 'package:frostsnap/wallet_receive.dart';

/// Searches one [addressSearchStep] of receive and change indices from `start`.
typedef AddressSearchStep =
    Future<AddressInfo?> Function(Address address, int start);

class CheckAddressPage extends StatelessWidget {
  const CheckAddressPage({super.key});

  @override
  Widget build(BuildContext context) {
    final walletCtx = WalletContext.of(context)!;
    final wallet = walletCtx.wallet;
    return AddressLookup(
      network: walletCtx.network,
      searchStep: (address, start) => wallet.superWallet.searchForAddress(
        masterAppkey: wallet.masterAppkey,
        address: address,
        start: start,
      ),
      onShowAddress: (address) => showBottomSheetOrDialog(
        context,
        title: Text(address.external ? 'Receive' : 'Change'),
        builder: (context, scrollController) => walletCtx.wrap(
          ReceivePage(
            wallet: wallet,
            txStream: walletCtx.txStream,
            scrollController: scrollController,
            address: address,
          ),
        ),
      ),
      onScan: (context) => MaybeFullscreenDialog.show<String>(
        context: context,
        child: AddressScanner(),
      ),
    );
  }
}

class AddressLookup extends StatefulWidget {
  final BitcoinNetwork network;
  final AddressSearchStep searchStep;
  final void Function(AddressInfo address) onShowAddress;
  final Future<String?> Function(BuildContext context) onScan;

  const AddressLookup({
    super.key,
    required this.network,
    required this.searchStep,
    required this.onShowAddress,
    required this.onScan,
  });

  @override
  State<AddressLookup> createState() => _AddressLookupState();
}

sealed class _Search {}

class _Searching extends _Search {
  final int start;
  _Searching(this.start);
}

class _Found extends _Search {
  final AddressInfo address;
  _Found(this.address);
}

class _NotFound extends _Search {
  final Address address;

  /// Every index below this was checked, on both keychains.
  final int checkedTo;
  _NotFound(this.address, this.checkedTo);
}

class _Failed extends _Search {
  final Object error;
  _Failed(this.error);
}

class _AddressLookupState extends State<AddressLookup> {
  final controller = _HighlightingController();
  late AddressInput input = widget.network.checkAddressInput(text: '');
  String checkedText = '';
  _Search? search;

  @override
  void initState() {
    super.initState();
    controller.addListener(_onTextChanged);
  }

  @override
  void dispose() {
    controller.dispose();
    super.dispose();
  }

  void _onTextChanged() {
    final text = controller.text;
    if (text == checkedText) return;
    checkedText = text;
    final checked = widget.network.checkAddressInput(text: text);
    controller.badChars = checked.badChars.toSet();
    setState(() {
      input = checked;
      search = null;
    });
    if (checked.status case AddressInputStatus_Valid(:final field0)) {
      _search(field0, 0);
    }
  }

  Future<void> _search(Address address, int start) async {
    final searching = _Searching(start);
    setState(() => search = searching);
    _Search result;
    try {
      final found = await widget.searchStep(address, start);
      result = found == null
          ? _NotFound(address, start + addressSearchStep())
          : _Found(found);
    } catch (error) {
      result = _Failed(error);
    }
    // An edit while this step ran replaced `search`; its result is for an address no longer shown.
    if (!mounted || search != searching) return;
    setState(() => search = result);
  }

  Future<void> _scan() async {
    final scanned = await widget.onScan(context);
    if (!mounted || scanned == null) return;
    controller.text = scanned;
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final search = this.search;
    final problem = _addressProblem(input, widget.network);

    final field = TextField(
      key: const Key('address-input'),
      controller: controller,
      minLines: 2,
      maxLines: 4,
      autocorrect: false,
      enableSuggestions: false,
      style: monospaceTextStyle.copyWith(fontSize: 15, height: 1.5),
      decoration: InputDecoration(
        filled: true,
        labelText: 'Address',
        alignLabelWithHint: true,
        helperText: input.status is AddressInputStatus_Empty
            ? 'Paste, type or scan an address'
            : null,
        errorText: problem,
        errorMaxLines: 3,
        suffixIcon: IconButton(
          tooltip: 'Scan QR code',
          icon: const Icon(Icons.qr_code_scanner),
          onPressed: _scan,
        ),
      ),
    );

    final body = Padding(
      padding: const EdgeInsets.symmetric(horizontal: 16),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            'Check whether an address belongs to this wallet.',
            style: theme.textTheme.bodyLarge,
          ),
          const SizedBox(height: 20),
          field,
          const SizedBox(height: 20),
          AnimatedSize(
            duration: Durations.medium2,
            curve: Curves.easeOutCubic,
            alignment: Alignment.topCenter,
            child: switch (search) {
              null => const SizedBox(width: double.infinity),
              _Searching(:final start) => Padding(
                key: const Key('searching'),
                padding: const EdgeInsets.only(top: 4),
                child: Row(
                  children: [
                    const SizedBox.square(
                      dimension: 20,
                      child: CircularProgressIndicator(strokeWidth: 2.5),
                    ),
                    const SizedBox(width: 16),
                    Expanded(
                      child: Text(
                        'Searching '
                        '${_indexRange(start, start + addressSearchStep())}',
                        style: _secondary(theme),
                      ),
                    ),
                  ],
                ),
              ),
              _Found(:final address) => _ResultPanel(
                key: const Key('search-found'),
                icon: Icons.check_circle,
                color: theme.colorScheme.primary,
                title:
                    'This is your ${_keychain(address)} address '
                    '#${_groupThousands(address.index)}',
                children: [
                  if (address.derivationPath.isNotEmpty)
                    Text.rich(
                      TextSpan(
                        text: 'Derivation path  ',
                        children: [
                          TextSpan(
                            text: address.derivationPath.join('/'),
                            style: monospaceTextStyle,
                          ),
                        ],
                      ),
                      style: _secondary(theme),
                    ),
                ],
                action: FilledButton(
                  key: const Key('show-found-address'),
                  onPressed: () => widget.onShowAddress(address),
                  child: Text('Show ${_keychain(address)} address'),
                ),
              ),
              _NotFound(:final address, :final checkedTo) => _ResultPanel(
                key: const Key('search-not-found'),
                icon: Icons.block,
                color: theme.colorScheme.onSurfaceVariant,
                title: 'Not in this wallet',
                children: [
                  Text('Not among ${_indexRange(0, checkedTo)}.'),
                  const SizedBox(height: 8),
                  Text(
                    'A wallet uses its addresses in order, so an address it '
                    'has given out is almost always near the start.',
                    style: _secondary(theme),
                  ),
                ],
                action: TextButton(
                  key: const Key('keep-looking'),
                  onPressed: () => _search(address, checkedTo),
                  child: const Text('Keep looking'),
                ),
              ),
              _Failed(:final error) => _ResultPanel(
                icon: Icons.error_outline,
                color: theme.colorScheme.error,
                title: "Couldn't search",
                children: [Text(displayExceptionMessage(error))],
              ),
            },
          ),
        ],
      ),
    );

    return SafeArea(
      child: CustomScrollView(
        shrinkWrap: true,
        slivers: [
          TopBarSliver(
            title: const Text('Check address'),
            leading: IconButton(
              icon: const Icon(Icons.arrow_back),
              onPressed: () => Navigator.pop(context),
            ),
            showClose: false,
          ),
          SliverToBoxAdapter(child: body),
          const SliverToBoxAdapter(child: SizedBox(height: 24)),
        ],
      ),
    );
  }
}

String? _addressProblem(AddressInput input, BitcoinNetwork network) {
  final highlighted = input.badChars.isNotEmpty;
  final plural = input.badChars.length > 1;
  return switch (input.status) {
    AddressInputStatus_Empty() || AddressInputStatus_Valid() => null,
    AddressInputStatus_NotTaproot() =>
      "Not from this wallet. Frostsnap only uses Taproot addresses, which "
          "start with ${_taprootPrefix(network)}.",
    AddressInputStatus_WrongNetwork(:final addressNetwork) =>
      "This is a ${_addressNetworkName(addressNetwork)} address, but this "
          "wallet is on ${network.isMainnet() ? 'mainnet' : network.name()}.",
    AddressInputStatus_InvalidCharacters() =>
      plural
          ? "The highlighted characters can't appear in an address."
          : "The highlighted character can't appear in an address.",
    AddressInputStatus_MixedCase() =>
      "An address is all lower case or all upper case. The highlighted "
          "letters don't match the rest.",
    AddressInputStatus_WrongLength() =>
      "Too short or too long for an address. Check for missing or extra "
          "characters.",
    AddressInputStatus_BadChecksum() =>
      highlighted
          ? "Checksum doesn't match. The highlighted character looks wrong."
          : "Checksum doesn't match. A character is wrong, missing or extra.",
    AddressInputStatus_NotAnAddress() => "This isn't a bitcoin address.",
  };
}

String _taprootPrefix(BitcoinNetwork network) => switch (network) {
  BitcoinNetwork.bitcoin => 'bc1p',
  BitcoinNetwork.regtest => 'bcrt1p',
  _ => 'tb1p',
};

String _addressNetworkName(AddressNetwork network) => switch (network) {
  AddressNetwork.mainnet => 'mainnet',
  AddressNetwork.testnet => 'testnet or signet',
  AddressNetwork.regtest => 'regtest',
};

String _groupThousands(int n) {
  final digits = n.toString();
  final buffer = StringBuffer();
  for (var i = 0; i < digits.length; i++) {
    if (i > 0 && (digits.length - i) % 3 == 0) buffer.write(',');
    buffer.write(digits[i]);
  }
  return buffer.toString();
}

String _keychain(AddressInfo address) =>
    address.external ? 'receive' : 'change';

TextStyle? _secondary(ThemeData theme) => theme.textTheme.bodyMedium?.copyWith(
  color: theme.colorScheme.onSurfaceVariant,
);

String _indexRange(int start, int end) =>
    'receive and change addresses #${_groupThousands(start)} to '
    '#${_groupThousands(end - 1)}';

class _ResultPanel extends StatelessWidget {
  final IconData icon;
  final Color color;
  final String title;
  final List<Widget> children;

  /// Spans the page's content width, like the address field, rather than the text column indented
  /// past the icon.
  final Widget? action;

  const _ResultPanel({
    super.key,
    required this.icon,
    required this.color,
    required this.title,
    required this.children,
    this.action,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final action = this.action;
    return Padding(
      padding: const EdgeInsets.only(top: 20),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Icon(icon, color: color, size: 28),
              const SizedBox(width: 16),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    Text(title, style: theme.textTheme.titleMedium),
                    const SizedBox(height: 6),
                    ...children,
                  ],
                ),
              ),
            ],
          ),
          if (action != null) ...[const SizedBox(height: 20), action],
        ],
      ),
    );
  }
}

class _HighlightingController extends TextEditingController {
  /// UTF-16 offsets, the unit of both Dart strings and the Rust check's output.
  Set<int> badChars = {};

  @override
  TextSpan buildTextSpan({
    required BuildContext context,
    TextStyle? style,
    required bool withComposing,
  }) {
    if (badChars.isEmpty) {
      return super.buildTextSpan(
        context: context,
        style: style,
        withComposing: withComposing,
      );
    }
    final colors = Theme.of(context).colorScheme;
    final badStyle = TextStyle(
      color: colors.onError,
      backgroundColor: colors.error,
      fontWeight: FontWeight.bold,
    );
    final spans = <TextSpan>[];
    var runStart = 0;
    for (var i = 0; i <= text.length; i++) {
      final boundary =
          i == text.length ||
          (i > runStart && badChars.contains(i) != badChars.contains(runStart));
      if (!boundary) continue;
      spans.add(
        TextSpan(
          text: text.substring(runStart, i),
          style: badChars.contains(runStart) ? badStyle : null,
        ),
      );
      runStart = i;
    }
    return TextSpan(style: style, children: spans);
  }
}
