import 'dart:collection';

import 'package:collection/collection.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:frostsnapp/bridge_definitions.dart';
import 'package:frostsnapp/contexts.dart';
import 'package:frostsnapp/address.dart';
import 'package:frostsnapp/global.dart';
import 'package:frostsnapp/id_ext.dart';
import 'package:frostsnapp/theme.dart';
import 'package:frostsnapp/wallet.dart';
import 'package:frostsnapp/wallet_tx_details.dart';
import 'package:pretty_qr_code/pretty_qr_code.dart';

class AddressList extends StatefulWidget {
  final bool showUsed;
  final Function(BuildContext, Address) onTap;
  final int? scrollToDerivationIndex;

  const AddressList({
    super.key,
    required this.onTap,
    this.showUsed = false,
    this.scrollToDerivationIndex,
  });

  @override
  State<AddressList> createState() => _AddressListState();
}

class _AddressListState extends State<AddressList> {
  late bool _showUsed;
  List<Address> _addresses = [];
  List<Address> _freshAddresses = [];
  List<Address> get addresses => _showUsed ? _addresses : _freshAddresses;

  final _firstAddrKey = GlobalKey();
  final _scrollController = ScrollController();

  void update(BuildContext context, {void Function()? andSetState}) {
    final walletCtx = WalletContext.of(context);
    if (walletCtx != null) {
      final addresses = walletCtx.superWallet.addressesState(
        masterAppkey: walletCtx.masterAppkey,
      );
      final freshAddresses = addresses.where((a) => !a.used).toList();
      if (mounted) {
        setState(() {
          _addresses = addresses;
          _freshAddresses = freshAddresses;
          if (andSetState != null) andSetState();
        });
      }
    }
  }

  @override
  void initState() {
    super.initState();
    _showUsed = widget.showUsed;
    update(context);

    // Scroll to the given derivation index (if requested).
    final startIndex = widget.scrollToDerivationIndex;
    if (startIndex != null) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        late final double addrItemHeight;
        final addrItemCtx = _firstAddrKey.currentContext;
        if (addrItemCtx != null) {
          final render = addrItemCtx.findRenderObject() as RenderBox;
          addrItemHeight = render.size.height;
        } else {
          addrItemHeight = 0;
        }
        final targetIndex =
            addresses.indexed
                .firstWhereOrNull((ia) => ia.$2.index == startIndex)
                ?.$1 ??
            0;
        final targetOffset =
            _scrollController.offset + targetIndex * addrItemHeight;
        _scrollController.animateTo(
          targetOffset,
          duration: Durations.long4,
          curve: Curves.easeInOutCubicEmphasized,
        );
      });
    }
  }

  Widget buildAddressItem(BuildContext context, Address addr, {Key? key}) {
    final theme = Theme.of(context);
    return Card.filled(
      key: key,
      clipBehavior: Clip.hardEdge,
      color:
          addr.used
              ? Colors.transparent
              : theme.colorScheme.surfaceContainerHighest,
      margin: EdgeInsets.only(bottom: 16.0),
      child: ListTile(
        onTap: () {
          Navigator.pop(context);
          widget.onTap(context, addr);
        },
        leading: Text(
          '#${addr.index}',
          style: theme.textTheme.labelLarge?.copyWith(
            decoration: addr.used ? TextDecoration.lineThrough : null,
            color:
                addr.used
                    ? theme.colorScheme.onSurfaceVariant
                    : theme.colorScheme.primary,
            fontFamily: monospaceTextStyle.fontFamily,
          ),
        ),
        trailing: Icon(Icons.chevron_right),
        title: Text(
          spacedHex(addr.addressString),
          style: monospaceTextStyle.copyWith(
            color: addr.used ? theme.colorScheme.onSurfaceVariant : null,
          ),
          overflow: TextOverflow.ellipsis,
          maxLines: 2,
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    final appBar = SliverAppBar(
      title: Text('Pick address'),
      titleTextStyle: theme.textTheme.titleMedium,
      centerTitle: true,
      backgroundColor: theme.colorScheme.surfaceContainerLow,
      pinned: true,
      leading: IconButton(
        onPressed: () => Navigator.pop(context),
        icon: Icon(Icons.close),
      ),
      bottom: PreferredSize(
        preferredSize: Size.fromHeight(76.0),
        child: Card.filled(
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(24.0),
          ),
          color: theme.colorScheme.secondaryContainer,
          margin: const EdgeInsets.symmetric(
            horizontal: 16.0,
          ).copyWith(bottom: 12.0),
          clipBehavior: Clip.hardEdge,
          child: SwitchListTile(
            value: _showUsed,
            onChanged: (v) => update(context, andSetState: () => _showUsed = v),
            title: Text('Show used'),
            contentPadding: EdgeInsets.only(
              left: 20.0,
              right: 16.0,
              top: 4.0,
              bottom: 4.0,
            ),
          ),
        ),
      ),
    );

    var first = true;
    return CustomScrollView(
      controller: _scrollController,
      shrinkWrap: true,
      physics: ClampingScrollPhysics(),
      slivers: [
        appBar,
        SliverSafeArea(
          sliver: SliverPadding(
            padding: const EdgeInsets.symmetric(
              horizontal: 16.0,
              vertical: 8.0,
            ),
            sliver: SliverList.list(
              children:
                  addresses
                      .map(
                        (addr) => buildAddressItem(
                          context,
                          addr,
                          key: () {
                            if (first) {
                              first = false;
                              return _firstAddrKey;
                            } else {
                              return null;
                            }
                          }(),
                        ),
                      )
                      .toList(),
            ),
          ),
        ),
      ],
    );
  }
}

class ReceivePage extends StatefulWidget {
  final Wallet wallet;
  final int? derivationIndex;

  const ReceivePage({super.key, required this.wallet, this.derivationIndex});

  @override
  State<ReceivePage> createState() => _ReceiverPageState();
}

class _ReceiverPageState extends State<ReceivePage> {
  Address? _address;
  bool get isReady => _address != null;
  Wallet get wallet => widget.wallet;

  QrImage addressQrImage(Address address) {
    final qrCode = QrCode(8, QrErrorCorrectLevel.L);
    qrCode.addData(address.addressString);
    return QrImage(qrCode);
  }

  @override
  void initState() {
    super.initState();

    final index = widget.derivationIndex;
    (index != null) ? updateToIndex(index) : updateToNextUnused();
  }

  void updateToIndex(int index) {
    final addr = wallet.superWallet.addressState(
      masterAppkey: wallet.masterAppkey,
      index: index,
    );
    if (mounted) setState(() => _address = addr);
  }

  void updateToNextUnused() async {
    final addr = await wallet.superWallet.nextUnusedAddress(
      masterAppkey: wallet.masterAppkey,
    );
    if (mounted) setState(() => _address = addr);
  }

  Widget buildStep1(BuildContext context, Address address) {
    final theme = Theme.of(context);
    return Card.filled(
      clipBehavior: Clip.hardEdge,
      color: theme.colorScheme.surfaceContainerHigh,
      margin: EdgeInsets.symmetric(horizontal: 16.0).copyWith(bottom: 16.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          ListTile(
            dense: true,
            leading: CircleAvatar(
              radius: 12.0,
              backgroundColor: theme.colorScheme.surfaceContainerLowest,
              child: Text('1.', style: theme.textTheme.titleSmall),
            ),
            title: Text('Share address', textAlign: TextAlign.start),
            subtitle: Text(
              'Derivation path: ${address.derivationPath}',
              style: monospaceTextStyle,
            ),
          ),
          Padding(
            padding: const EdgeInsets.symmetric(
              horizontal: 16.0,
            ).copyWith(bottom: 16.0, top: 4.0),
            child: Row(
              spacing: 16.0,
              children: [
                Expanded(
                  child: AspectRatio(
                    aspectRatio: 1.0,
                    child: Card.filled(
                      color: Colors.white,
                      margin: EdgeInsets.all(0.0),
                      clipBehavior: Clip.hardEdge,
                      child: InkWell(
                        onTap: () {
                          print('aaa');
                        },
                        child: Padding(
                          padding: const EdgeInsets.all(8.0),
                          child: PrettyQrView(
                            qrImage: addressQrImage(address),
                            decoration: const PrettyQrDecoration(
                              shape: PrettyQrSmoothSymbol(color: Colors.black),
                            ),
                          ),
                        ),
                      ),
                    ),
                  ),
                ),
                Expanded(
                  child: AspectRatio(
                    aspectRatio: 1.0,
                    child: InkWell(
                      borderRadius: BorderRadius.circular(8.0),
                      onTap:
                          () => copyAction(
                            context,
                            'Address',
                            address.addressString,
                          ),
                      child: Center(
                        child: Padding(
                          padding: const EdgeInsets.all(8.0),
                          child: FittedBox(
                            child: Column(
                              mainAxisSize: MainAxisSize.min,
                              crossAxisAlignment: CrossAxisAlignment.center,
                              spacing: 12.0,
                              children: [
                                Text(
                                  spacedHex(
                                    address.addressString,
                                    groupsPerLine: 3,
                                  ),
                                  style: monospaceTextStyle,
                                  softWrap: false,
                                  textAlign: TextAlign.end,
                                ),
                                Icon(Icons.copy, size: 20.0),
                              ],
                            ),
                          ),
                        ),
                      ),
                    ),
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }

  Widget buildStep2(BuildContext context, Address address) {
    final Map<DeviceId, String> devices = LinkedHashMap(
      equals: deviceIdEquals,
      hashCode: (a) => Object.hashAll(a.field0),
    );

    final frostKey = wallet.frostKey();
    if (frostKey != null) {
      for (final access in frostKey.accessStructures()) {
        devices.addEntries(
          access.devices().map(
            (id) => MapEntry(id, coord.getDeviceName(id: id) ?? '<no-name>'),
          ),
        );
      }
    }

    deviceListSubject;

    final theme = Theme.of(context);
    return Card.filled(
      color: theme.colorScheme.surfaceContainerHigh,
      clipBehavior: Clip.hardEdge,
      margin: EdgeInsets.symmetric(horizontal: 16.0).copyWith(bottom: 12.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          ListTile(
            dense: true,
            leading: CircleAvatar(
              radius: 12.0,
              backgroundColor: theme.colorScheme.surfaceContainerLowest,
              child: Text('2.', style: theme.textTheme.titleSmall),
            ),
            title: Text('Verify address', textAlign: TextAlign.start),
            subtitle: Text('Plug in any of these devices'),
          ),
          StreamBuilder(
            stream: deviceListSubject,
            builder: (context, snapshot) {
              final data = snapshot.data;
              if (data == null) {
                return SizedBox();
              }
              final connected = deviceIdSet(
                data.state.devices.map((d) => d.id).toList(),
              );
              return Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                mainAxisSize: MainAxisSize.min,
                children:
                    devices.entries.map((entry) {
                      final isConnected = connected.contains(entry.key);
                      return ListTile(
                        dense: true,
                        enabled: isConnected,
                        title: Text(entry.value, style: monospaceTextStyle),
                        trailing: Text(
                          isConnected ? 'Connected' : 'Disconnected',
                          style: TextStyle(
                            color:
                                isConnected
                                    ? Theme.of(context).colorScheme.primary
                                    : null,
                          ),
                        ),
                      );
                    }).toList(),
              );
            },
          ),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    final appBar = SliverAppBar(
      title: Text('Receive bitcoin'),
      titleTextStyle: theme.textTheme.titleMedium,
      centerTitle: true,
      backgroundColor: theme.colorScheme.surfaceContainerLow,
      pinned: true,
      leading: IconButton(
        onPressed: () => Navigator.pop(context),
        icon: Icon(Icons.close),
      ),
      actions: [
        TextButton.icon(
          onPressed:
              isReady ? () => openAddressPicker(context, _address!) : null,
          label: Text(
            '#${_address?.index}',
            textAlign: TextAlign.end,
            style: monospaceTextStyle,
          ),
          icon: Icon(Icons.arrow_drop_down),
        ),
      ],
      actionsPadding: EdgeInsets.symmetric(horizontal: 12.0),
    );

    return CustomScrollView(
      shrinkWrap: true,
      physics: ClampingScrollPhysics(),
      slivers: [
        appBar,
        if (isReady) SliverToBoxAdapter(child: buildStep1(context, _address!)),
        if (isReady) SliverToBoxAdapter(child: buildStep2(context, _address!)),
        if (isReady)
          SliverSafeArea(
            sliver: SliverPadding(
              padding: EdgeInsets.symmetric(horizontal: 16.0),
              sliver: SliverToBoxAdapter(
                child: TextButton.icon(
                  onPressed: () {},
                  icon: Icon(Icons.done),
                  label: Text('Mark as used'),
                ),
              ),
            ),
          ),
      ],
    );
  }

  void openAddressPicker(BuildContext context, Address address) {
    final walletCtx = WalletContext.of(context)!;
    showBottomSheetOrDialog(
      context,
      builder: (context) {
        return walletCtx.wrap(
          AddressList(
            onTap: (context, addr) => updateToIndex(addr.index),
            showUsed: address.used,
            scrollToDerivationIndex: address.index,
          ),
        );
      },
    );
  }
}

class WalletReceivePage extends StatefulWidget {
  const WalletReceivePage({super.key});

  @override
  State<WalletReceivePage> createState() => _WalletReceivePageState();
}

class _WalletReceivePageState extends State<WalletReceivePage> {
  final GlobalKey<AnimatedListState> _listKey = GlobalKey<AnimatedListState>();
  late List<Address> _addresses = [];
  final ScrollController scrollController = ScrollController(
    keepScrollOffset: false,
  );
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

    final nextAddressInfo = await walletCtx.superWallet.nextAddress(
      masterAppkey: walletCtx.masterAppkey,
    );
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
    _addresses = walletCtx.superWallet.addressesState(
      masterAppkey: walletCtx.masterAppkey,
    );

    final freshAddrs = _addresses.where((addr) => !addr.used).toList();

    final body = CustomScrollView(
      controller: scrollController,
      reverse: true,
      slivers: [
        SliverToBoxAdapter(child: SizedBox(height: 80)),
        SliverSafeArea(
          sliver: SliverList.builder(
            key: _listKey,
            itemCount: _addresses.length,
            itemBuilder:
                (context, index) =>
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
            builder: (context) => walletCtx.wrap(AddressPage(address: address)),
          ),
        );
      }
    }

    final theme = Theme.of(context);

    final appBar = SliverAppBar(
      flexibleSpace: FlexibleSpaceBar(
        title: Text('Receive', style: theme.textTheme.titleMedium),
        centerTitle: true,
        background: Container(
          decoration: BoxDecoration(
            borderRadius: BorderRadiusDirectional.only(
              topStart: Radius.circular(24.0),
              topEnd: Radius.circular(24.0),
            ),
            //borderRadius: BorderRadius.circular(24.0),
            color: theme.colorScheme.surfaceContainerLow,
          ),
        ),
      ),
      pinned: true,
      forceMaterialTransparency: true,
      automaticallyImplyLeading: false,
      leading: IconButton(
        onPressed: () => Navigator.pop(context),
        icon: Icon(Icons.close),
      ),
    );

    final scrollView = CustomScrollView(
      controller: scrollController,
      shrinkWrap: true,
      slivers: [
        appBar,
        PinnedHeaderSliver(
          child: Padding(
            padding: const EdgeInsets.only(
              left: 16.0,
              right: 16.0,
              bottom: 12.0,
            ),
            child: OutlinedButton(
              onPressed: () async => await _addAddress(context),
              child: Text('New Address'),
            ),
          ),
        ),
        SliverSafeArea(
          sliver: SliverList.builder(
            itemCount: freshAddrs.length,
            itemBuilder: (context, index) {
              final addr = freshAddrs[index];
              return Card.filled(
                clipBehavior: Clip.hardEdge,
                margin: EdgeInsets.only(left: 16.0, right: 16.0, bottom: 12.0),
                child: ListTile(
                  onTap: () {},
                  leading: Text(
                    '#${addr.index}',
                    style: theme.textTheme.labelLarge?.copyWith(
                      color: theme.colorScheme.primary,
                      fontFamily: monospaceTextStyle.fontFamily,
                    ),
                  ),
                  trailing: Icon(Icons.chevron_right),
                  title: Text(
                    spacedHex(addr.addressString),
                    style: monospaceTextStyle,
                    overflow: TextOverflow.ellipsis,
                    maxLines: 2,
                  ),
                ),
              );
            },
          ),
        ),
      ],
    );
    //final a = SwitchListTile(value: value, onChanged: onChanged);

    return scrollView;

    //return Scaffold(
    //  appBar: AppBar(title: const Text('Receive Bitcoin'), centerTitle: true),
    //  body: body,
    //  floatingActionButton: FloatingActionButton.extended(
    //    extendedIconLabelSpacing: fabIsExtended ? 8 : 0,
    //    extendedPadding: fabIsExtended ? null : const EdgeInsets.all(16),
    //    icon: Icon(Icons.add),
    //    label: AnimatedSize(
    //      curve: Curves.easeInOutCubicEmphasized,
    //      duration: Durations.long1,
    //      child: Text(fabIsExtended ? 'New Address' : ''),
    //    ),
    //    onPressed: newAddressAction,
    //  ),
    //);
  }

  Widget _buildAddressItem(BuildContext context, Address address) {
    final walletCtx = WalletContext.of(context)!;
    final theme = Theme.of(context);

    openAddressPage() async {
      Navigator.push(
        context,
        MaterialPageRoute(
          builder: (context) => walletCtx.wrap(AddressPage(address: address)),
        ),
      );
    }

    copyAddress() async {
      Clipboard.setData(ClipboardData(text: address.addressString));
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text('Address copied to clipboard')));
    }

    return Card.filled(
      color: ElevationOverlay.applySurfaceTint(
        theme.colorScheme.surface,
        theme.colorScheme.surfaceTint,
        address.used ? 0.0 : 6.0,
      ),
      child: ListTile(
        isThreeLine: true,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(16.0),
        ),
        title: Text(
          address.addressString,
          maxLines: 2,
          overflow: TextOverflow.ellipsis,
          style: monospaceTextStyle,
        ),
        subtitle: Text('# ${address.index}${address.used ? ' (Used)' : ''}'),
        onLongPress: copyAddress,
        onTap: openAddressPage,
        trailing: Icon(Icons.policy),
      ),
    );
  }
}
