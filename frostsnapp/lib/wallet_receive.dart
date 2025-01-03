import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:frostsnapp/bridge_definitions.dart';
import 'package:frostsnapp/contexts.dart';
import 'package:frostsnapp/address.dart';
import 'package:frostsnapp/theme.dart';

class WalletReceivePage extends StatefulWidget {
  const WalletReceivePage({super.key});

  @override
  State<WalletReceivePage> createState() => _WalletReceivePageState();
}

class _WalletReceivePageState extends State<WalletReceivePage> {
  final GlobalKey<AnimatedListState> _listKey = GlobalKey<AnimatedListState>();
  late List<Address> _addresses = [];
  final ScrollController scrollController =
      ScrollController(keepScrollOffset: false);
  bool fabIsExtended = true;

  @override
  void initState() {
    super.initState();
    scrollController.addListener(() {
      if (scrollController.offset < 6.0) {
        setState(() => fabIsExtended = true);
      } else if (scrollController.position.userScrollDirection ==
              ScrollDirection.reverse &&
          fabIsExtended) {
        // Shrink FAB when scrolling down
        setState(() => fabIsExtended = false);
      } else if (scrollController.position.userScrollDirection ==
              ScrollDirection.forward &&
          !fabIsExtended) {
        // Extend FAB when scrolling up
        setState(() => fabIsExtended = true);
      }
    });
  }

  @override
  void dispose() {
    scrollController.dispose();
    super.dispose();
  }

  Future<Address> _addAddress(BuildContext context) async {
    final walletCtx = WalletContext.of(context)!;

    final nextAddressInfo = await walletCtx.superWallet
        .nextAddress(masterAppkey: walletCtx.masterAppkey);
    final Address newAddress = nextAddressInfo;

    if (context.mounted) {
      if (context.mounted) {
        setState(() {
          _addresses.insert(0, newAddress);
          _listKey.currentState?.insertItem(0);
        });
      }
    }
    return nextAddressInfo;
  }

  @override
  Widget build(BuildContext context) {
    final walletCtx = WalletContext.of(context)!;
    _addresses = walletCtx.superWallet
        .addressesState(masterAppkey: walletCtx.masterAppkey);

    final body = CustomScrollView(
      controller: scrollController,
      reverse: true,
      slivers: [
        SliverToBoxAdapter(child: SizedBox(height: 80)),
        SliverSafeArea(
          sliver: SliverList.builder(
            key: _listKey,
            itemCount: _addresses.length,
            itemBuilder: (context, index) =>
                _buildAddressItem(context, _addresses[index]),
          ),
        ),
      ],
    );

    newAddressAction() async {
      final address = await _addAddress(context);
      if (context.mounted) {
        await scrollController.animateTo(
          0.0,
          duration: Durations.long1,
          curve: Curves.easeInOutCubicEmphasized,
        );
      }
      if (context.mounted) {
        Navigator.push(
          context,
          MaterialPageRoute(
            builder: (context) => walletCtx.wrap(
              AddressPage(
                address: address,
              ),
            ),
          ),
        );
      }
    }

    return Scaffold(
      appBar: AppBar(title: const Text('Receive Bitcoin'), centerTitle: true),
      body: body,
      floatingActionButton: FloatingActionButton.extended(
        extendedIconLabelSpacing: fabIsExtended ? 8 : 0,
        extendedPadding: fabIsExtended ? null : const EdgeInsets.all(16),
        icon: Icon(Icons.add),
        label: AnimatedSize(
          curve: Curves.easeInOutCubicEmphasized,
          duration: Durations.long1,
          child: Text(fabIsExtended ? 'New Address' : ''),
        ),
        onPressed: newAddressAction,
      ),
    );
  }

  Widget _buildAddressItem(BuildContext context, Address address) {
    final walletCtx = WalletContext.of(context)!;
    final theme = Theme.of(context);

    openAddressPage() async {
      Navigator.push(
          context,
          MaterialPageRoute(
            builder: (context) => walletCtx.wrap(AddressPage(address: address)),
          ));
    }

    copyAddress() async {
      Clipboard.setData(ClipboardData(text: address.addressString));
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text('Address copied to clipboard')));
    }

    return Card.filled(
      color: ElevationOverlay.applySurfaceTint(
        theme.colorScheme.surface,
        theme.colorScheme.surfaceTint,
        address.used ? 0.0 : 6.0,
      ),
      child: ListTile(
        isThreeLine: true,
        shape:
            RoundedRectangleBorder(borderRadius: BorderRadius.circular(16.0)),
        title: Text(
          address.addressString,
          maxLines: 2,
          overflow: TextOverflow.ellipsis,
          style: addressTextStyle,
        ),
        subtitle: Text('# ${address.index}${address.used ? ' (Used)' : ''}'),
        onLongPress: copyAddress,
        onTap: openAddressPage,
        trailing: Icon(Icons.policy),
      ),
    );
  }
}
