import 'package:flutter/material.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/settings.dart';
import 'package:frostsnap/src/rust/api/bitcoin.dart';
import 'package:frostsnap/src/rust/api/name.dart';

/// Parent triggers submission via a [GlobalKey<EnterWalletNameViewState>]
/// + `currentState!.submit()`, keeping form state local to the view.
class EnterWalletNameView extends StatefulWidget {
  final String? initialWalletName;
  final BitcoinNetwork? initialBitcoinNetwork;
  final void Function(bool canSubmit)? onChanged;
  final void Function(String walletName, BitcoinNetwork network) onSubmit;

  const EnterWalletNameView({
    super.key,
    required this.onSubmit,
    this.onChanged,
    this.initialWalletName,
    this.initialBitcoinNetwork,
  });

  @override
  State<EnterWalletNameView> createState() => EnterWalletNameViewState();
}

class EnterWalletNameViewState extends State<EnterWalletNameView> {
  final _formKey = GlobalKey<FormState>();
  final _walletNameController = TextEditingController();
  BitcoinNetwork bitcoinNetwork = BitcoinNetwork.bitcoin;
  bool _isButtonEnabled = false;

  @override
  void initState() {
    super.initState();
    _walletNameController.addListener(_updateButtonState);
    final initialWalletName = widget.initialWalletName;
    if (initialWalletName != null) {
      _walletNameController.text = initialWalletName;
      _isButtonEnabled = initialWalletName.isNotEmpty;
    }
    final initialBitcoinNetwork = widget.initialBitcoinNetwork;
    if (initialBitcoinNetwork != null) {
      bitcoinNetwork = initialBitcoinNetwork;
    }
  }

  void _updateButtonState() {
    final enabled = _walletNameController.text.isNotEmpty;
    if (enabled != _isButtonEnabled) {
      setState(() => _isButtonEnabled = enabled);
    }
    widget.onChanged?.call(enabled);
  }

  @override
  void dispose() {
    _walletNameController.removeListener(_updateButtonState);
    _walletNameController.dispose();
    super.dispose();
  }

  void submit() {
    if (_isButtonEnabled && _formKey.currentState!.validate()) {
      widget.onSubmit(_walletNameController.text.trim(), bitcoinNetwork);
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final developerMode = SettingsContext.of(
      context,
    )!.settings.isInDeveloperMode();

    return Form(
      key: _formKey,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            "Enter the wallet name from your physical backup. If it's missing "
            "or unreadable, choose another name.",
            style: theme.textTheme.bodyMedium,
          ),
          const SizedBox(height: 24),
          TextFormField(
            controller: _walletNameController,
            maxLength: keyNameMaxLength(),
            inputFormatters: [nameInputFormatter],
            autofocus: true,
            decoration: const InputDecoration(
              labelText: 'Wallet Name',
              border: OutlineInputBorder(),
              hintText: 'The name of the wallet being restored',
            ),
            onFieldSubmitted: (_) => submit(),
            validator: (value) {
              if (value == null || value.isEmpty) {
                return 'Please enter a wallet name';
              }
              return null;
            },
          ),
          if (developerMode) ...[
            const SizedBox(height: 16),
            BitcoinNetworkChooser(
              value: bitcoinNetwork,
              onChanged: (network) {
                setState(() => bitcoinNetwork = network);
              },
            ),
          ],
        ],
      ),
    );
  }
}
