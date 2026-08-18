import 'package:flutter/material.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/src/rust/api/bitcoin.dart';
import 'package:frostsnap/src/rust/api/super_wallet.dart';
import 'package:frostsnap/theme.dart';
import 'package:frostsnap/wallet_receive.dart';

class CheckAddressPage extends StatefulWidget {
  const CheckAddressPage({super.key});

  @override
  State<CheckAddressPage> createState() => _CheckAddressPageState();
}

class _CheckAddressPageState extends State<CheckAddressPage> {
  late final TextEditingController textInputController;
  Future<SearchResult>? searchFuture;
  int currentDepth = 0;
  int searchSize = 100;
  bool _isValidAddress = false;

  @override
  void initState() {
    super.initState();
    textInputController = TextEditingController();
    textInputController.addListener(_onTextChanged);
  }

  @override
  void dispose() {
    textInputController.dispose();
    super.dispose();
  }

  // Rebuilds on every change rather than on validity transitions: the error text
  // reads both the field's emptiness and its validity, and `_isValidAddress` is
  // false for both, so a validity-only trigger misses empty<->invalid entirely.
  void _onTextChanged() {
    final walletCtx = WalletContext.of(context);
    final text = textInputController.text.trim();
    setState(() {
      _isValidAddress =
          text.isNotEmpty &&
          walletCtx != null &&
          Address.fromString(s: text, network: walletCtx.network) != null;
    });
  }

  Future<SearchResult> searchAddress() async {
    final walletContext = WalletContext.of(context)!;
    if (currentDepth >= 1000) {
      searchSize = 1000;
    }

    final address = await walletContext.wallet.superWallet.searchForAddress(
      masterAppkey: walletContext.wallet.masterAppkey,
      addressStr: textInputController.text,
      start: currentDepth,
      stop: currentDepth + searchSize,
    );

    currentDepth += searchSize;

    return SearchResult(depth: currentDepth, address: address);
  }

  Widget _buildSearchResults(SearchResult? result) {
    if (result == null) return const SizedBox.shrink();
    final walletCtx = WalletContext.of(context)!;

    final children = <Widget>[
      Text(
        result.address != null
            ? "Found!"
            : "Address not found in first ${result.depth} addresses.",
      ),
      const SizedBox(height: 8),
    ];

    if (result.address != null) {
      children.addAll([
        Text(
          "This address belongs to us at ${result.address?.derivationPath ?? ""}",
        ),
        const SizedBox(height: 16),
        FilledButton.tonal(
          onPressed: () => showBottomSheetOrDialog(
            context,
            title: Text('Receive'),
            builder: (context, scrollController) => walletCtx.wrap(
              ReceivePage(
                wallet: walletCtx.wallet,
                txStream: walletCtx.txStream,
                scrollController: scrollController,
                derivationIndex: result.address?.index,
              ),
            ),
          ),
          child: const Text("Address info"),
        ),
      ]);
    } else {
      if (result.depth < 10000) {
        if (result.depth >= 1000) {
          children.addAll([
            const Text(
              "This address almost certainly doesn't belong to this wallet under any normal usage.",
            ),
            const SizedBox(height: 8),
            const Text("It's not yours or check another wallet."),
          ]);
        }
      } else {
        children.add(const Text("Look elsewhere... "));
      }
      children.addAll([
        const SizedBox(height: 16),
        FilledButton.tonal(
          onPressed: () {
            setState(() {
              searchFuture = searchAddress();
            });
          },
          child: const Text('Search deeper'),
        ),
      ]);
    }

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: children,
    );
  }

  @override
  Widget build(BuildContext context) {
    final body = Padding(
      padding: const EdgeInsets.all(16.0),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const Text('Check whether an address belongs to this wallet'),
          const SizedBox(height: 16),
          SizedBox(
            width: 400,
            child: TextFormField(
              controller: textInputController,
              minLines: 2,
              maxLines: 6,
              decoration: InputDecoration(
                counterText: '',
                errorText:
                    textInputController.text.isNotEmpty && !_isValidAddress
                    ? 'Invalid address for this network'
                    : null,
              ),
            ),
          ),
          const SizedBox(height: 32),
          FilledButton(
            onPressed: _isValidAddress
                ? () {
                    currentDepth = 0;
                    searchSize = 100;
                    setState(() {
                      searchFuture = searchAddress();
                    });
                  }
                : null,
            child: const Text('Look for address'),
          ),
          const SizedBox(height: 16),
          FutureBuilder<SearchResult>(
            future: searchFuture,
            builder: (context, snapshot) {
              if (snapshot.connectionState == ConnectionState.waiting) {
                return const Center(
                  child: Padding(
                    padding: EdgeInsets.all(16.0),
                    child: CircularProgressIndicator(),
                  ),
                );
              }

              if (snapshot.hasError) {
                return Text('Error: ${snapshot.error}');
              }

              return _buildSearchResults(snapshot.data);
            },
          ),
        ],
      ),
    );

    final scrollView = CustomScrollView(
      shrinkWrap: true,
      slivers: [
        TopBarSliver(
          title: Text('Check address'),
          leading: IconButton(
            icon: Icon(Icons.arrow_back),
            onPressed: () => Navigator.pop(context),
          ),
          showClose: false,
        ),
        SliverToBoxAdapter(child: body),
        SliverToBoxAdapter(child: SizedBox(height: 16)),
      ],
    );
    return SafeArea(child: scrollView);
  }
}

class SearchResult {
  final int depth;
  final AddressInfo? address;

  const SearchResult({required this.depth, required this.address});
}
