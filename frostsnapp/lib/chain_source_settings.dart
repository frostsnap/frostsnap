import 'package:flutter/material.dart';
import 'package:frostsnap/bitcoin_network_ext.dart';
import 'package:frostsnap/settings.dart';
import 'package:frostsnap/src/rust/api/bitcoin.dart';
import 'package:frostsnap/src/rust/api/settings.dart';

/// Per-network chain source picker.
class ChainSourceSettingsPage extends StatelessWidget {
  const ChainSourceSettingsPage({super.key});

  @override
  Widget build(BuildContext context) {
    final settings = SettingsContext.of(context)!;

    return Padding(
      padding: const EdgeInsets.all(16.0),
      child: StreamBuilder(
        stream: settings.developerSettings,
        builder: (context, snap) {
          final developerMode = snap.data?.developerMode ?? false;

          return ListView(
            children: [
              if (!settings.settings.compactFiltersAvailable())
                const Card.outlined(
                  child: ListTile(
                    leading: Icon(Icons.phone_android),
                    title: Text('Compact filters need a desktop'),
                    subtitle: Text(
                      'Running a filter node keeps connections to several peers and downloads '
                      'every filter on the chain, which has not been measured against phone '
                      'battery and data limits.',
                    ),
                  ),
                ),
              for (final network in BitcoinNetwork.supportedNetworks())
                if (network.isMainnet() || developerMode)
                  _ChainSourceCard(network: network),
            ],
          );
        },
      ),
    );
  }
}

class _ChainSourceCard extends StatelessWidget {
  final BitcoinNetwork network;

  const _ChainSourceCard({required this.network});

  @override
  Widget build(BuildContext context) {
    final settingsCtx = SettingsContext.of(context)!;
    final settings = settingsCtx.settings;
    final selected = settings.getChainSource(network: network);

    Future<void> choose(ChainSource? source) async {
      if (source == null || source == selected) return;
      try {
        await settings.setChainSource(network: network, source: source);
        if (context.mounted) {
          ScaffoldMessenger.of(context).showSnackBar(
            const SnackBar(
              content: Text('Restart Frostsnap to use the new chain source'),
            ),
          );
        }
      } catch (error) {
        if (context.mounted) {
          ScaffoldMessenger.of(
            context,
          ).showSnackBar(SnackBar(content: Text('$error')));
        }
      }
    }

    return Card.outlined(
      margin: const EdgeInsets.only(bottom: 12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 16, 16, 0),
            child: Text(
              network.displayName,
              style: Theme.of(context).textTheme.titleMedium,
            ),
          ),
          RadioGroup<ChainSource>(
            groupValue: selected,
            onChanged: choose,
            child: const Column(
              children: [
                RadioListTile<ChainSource>(
                  value: ChainSource.electrum,
                  title: Text('Electrum server'),
                  subtitle: Text(
                    'Fast, and light on data. The server is told which addresses belong to this '
                    'wallet.',
                  ),
                ),
                _CompactFiltersTile(),
              ],
            ),
          ),
          const Padding(
            padding: EdgeInsets.fromLTRB(16, 0, 16, 12),
            child: Text(
              'Takes effect when Frostsnap restarts.',
              style: TextStyle(fontStyle: FontStyle.italic),
            ),
          ),
        ],
      ),
    );
  }
}

/// Compact filters tile, split out so it can read `compactFiltersAvailable` at build time.
class _CompactFiltersTile extends StatelessWidget {
  const _CompactFiltersTile();

  @override
  Widget build(BuildContext context) {
    final available = SettingsContext.of(
      context,
    )!.settings.compactFiltersAvailable();

    return RadioListTile<ChainSource>(
      value: ChainSource.compactFilters,
      enabled: available,
      title: const Text('Compact block filters'),
      subtitle: Text(
        available
            ? 'No server learns which addresses are yours. Slower to sync and uses more data.'
            : 'Not available on this platform.',
      ),
    );
  }
}
