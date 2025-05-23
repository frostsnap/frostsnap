import 'package:flutter/material.dart';
import 'package:frostsnapp/contexts.dart';
import 'package:frostsnapp/src/rust/api/super_wallet.dart';
import 'package:frostsnapp/theme.dart';
import 'package:frostsnapp/wallet_receive.dart';

class CheckAddressPage extends StatefulWidget {
  const CheckAddressPage({Key? key}) : super(key: key);

  @override
  State<CheckAddressPage> createState() => _CheckAddressPageState();
}

class _CheckAddressPageState extends State<CheckAddressPage> {
  late final TextEditingController textInputController;
  Future<SearchResult>? searchFuture;
  int currentDepth = 0;
  int searchSize = 100;

  @override
  void initState() {
    super.initState();
    textInputController = TextEditingController();
  }

  @override
  void dispose() {
    textInputController.dispose();
    super.dispose();
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
    final walletCtx = WalletContext.of(context)!;
    if (result == null) return const SizedBox.shrink();

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
        ElevatedButton(
          onPressed:
              () => showBottomSheetOrDialog(
                context,
                titleText: 'Receive',
                builder:
                    (context, scrollController) => walletCtx.wrap(
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
        ElevatedButton(
          onPressed: () {
            setState(() {
              searchFuture = searchAddress();
            });
          },
          child: const Text('Search deeper'),
        ),
      ]);
    }

    return Column(children: children);
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: Center(
        child: SingleChildScrollView(
          child: Padding(
            padding: const EdgeInsets.all(16.0),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                const Text('Check whether an address belongs to this wallet'),
                const SizedBox(height: 16),
                SizedBox(
                  width: 400,
                  child: TextFormField(
                    controller: textInputController,
                    minLines: 2,
                    maxLines: 6,
                    decoration: const InputDecoration(counterText: ''),
                  ),
                ),
                const SizedBox(height: 32),
                ElevatedButton(
                  onPressed: () {
                    currentDepth = 0;
                    searchSize = 100;
                    setState(() {
                      searchFuture = searchAddress();
                    });
                  },
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
          ),
        ),
      ),
    );
  }
}

class SearchResult {
  final int depth;
  final AddressInfo? address;

  const SearchResult({required this.depth, required this.address});
}
