import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge.dart';
import 'package:frostsnap/animated_check.dart';
import 'package:frostsnap/copy_feedback.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/snackbar.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/src/rust/api/signing.dart';
import 'package:frostsnap/theme.dart';
import 'package:frostsnap/wallet_key_mismatch.dart';

/// Why [name] cannot be a new contact's name, or null if it can. An empty name has nothing to
/// report yet.
String? contactNameError(String name, Iterable<String> takenNames) {
  final trimmed = name.trim();
  if (trimmed.isEmpty) return null;
  if (trimmed.runes.any((rune) => rune < 0x20 || rune >= 0x7f)) {
    return 'Names can only use plain letters, numbers and symbols';
  }
  final lower = trimmed.toLowerCase();
  if (takenNames.any((taken) => taken.toLowerCase() == lower)) {
    return 'This wallet already has a contact with this name';
  }
  return null;
}

/// Whether a contact belongs in the results for what the user typed.
bool contactMatches(
  String query, {
  required String name,
  required String address,
}) {
  final lower = query.trim().toLowerCase();
  return name.toLowerCase().contains(lower) ||
      address.toLowerCase().contains(lower);
}

/// Whether what the user typed names exactly this contact, so that submitting it can only mean them.
bool contactIsExactly(
  String query, {
  required String name,
  required String address,
}) {
  final lower = query.trim().toLowerCase();
  return name.toLowerCase() == lower || address.toLowerCase() == lower;
}

/// What to tell the user about a failure, without the backtrace Rust errors carry.
String contactErrorMessage(Object error) {
  if (error is! AnyhowException) return '$error';
  return error.message.split('Stack backtrace:').first.trim();
}

String _shortAddress(String address) => address.length <= 24
    ? address
    : '${address.substring(0, 12)}...${address.substring(address.length - 8)}';

/// The contacts matching what is typed in [query], and the way to add one.
class SpContactSuggestions extends StatelessWidget {
  static const maxShown = 4;
  static const minQueryLength = 2;

  final KeyId keyId;
  final TextEditingController query;
  final bool Function(String) isSilentPaymentAddress;
  final void Function(SpContactEntry) onSelected;
  final void Function(String address) onAdd;

  const SpContactSuggestions({
    super.key,
    required this.keyId,
    required this.query,
    required this.isSilentPaymentAddress,
    required this.onSelected,
    required this.onAdd,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return ListenableBuilder(
      listenable: query,
      builder: (context, _) {
        final text = query.text.trim();
        final matches = text.length < minQueryLength
            ? <SpContactEntry>[]
            : coord
                  .spContacts(keyId: keyId)
                  .where(
                    (contact) => contactMatches(
                      text,
                      name: contact.name(),
                      address: contact.address(),
                    ),
                  )
                  .toList();
        final newAddress = matches.isEmpty && isSilentPaymentAddress(text);
        return Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          mainAxisSize: MainAxisSize.min,
          children: [
            for (final contact in matches.take(maxShown))
              ListTile(
                dense: true,
                contentPadding: const EdgeInsets.symmetric(horizontal: 8),
                leading: const Icon(Icons.person_outline),
                title: Text(contact.name()),
                subtitle: Text(
                  _shortAddress(contact.address()),
                  style: monospaceTextStyle,
                ),
                onTap: () => onSelected(contact),
              ),
            if (matches.length > maxShown)
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 8),
                child: Text(
                  '${matches.length - maxShown} more, keep typing to narrow it down',
                  style: theme.textTheme.bodySmall,
                ),
              ),
            Align(
              alignment: AlignmentDirectional.centerStart,
              child: TextButton.icon(
                onPressed: () => onAdd(newAddress ? text : ''),
                label: Text(newAddress ? 'Add as contact' : 'Add contact'),
                icon: const Icon(Icons.person_add_alt),
              ),
            ),
          ],
        );
      },
    );
  }
}

/// The wallet's contacts: find, add, check and remove them.
class SpContactsPage extends StatefulWidget {
  final AccessStructureRef accessStructureRef;

  const SpContactsPage({super.key, required this.accessStructureRef});

  @override
  State<SpContactsPage> createState() => _SpContactsPageState();
}

class _SpContactsPageState extends State<SpContactsPage> {
  final _search = TextEditingController();

  KeyId get _keyId => widget.accessStructureRef.keyId;

  @override
  void dispose() {
    _search.dispose();
    super.dispose();
  }

  Future<void> _add() async {
    await showAddSpContact(
      context,
      accessStructureRef: widget.accessStructureRef,
    );
    if (mounted) setState(() {});
  }

  Future<void> _open(SpContactEntry contact) async {
    await showBottomSheetOrDialog<void>(
      context,
      title: Text(contact.name()),
      builder: (context, scrollController) => ListView(
        controller: scrollController,
        shrinkWrap: true,
        padding: const EdgeInsets.all(16),
        children: [
          SelectableText(
            spacedHex(contact.address(), groupSize: 4),
            style: monospaceTextStyle,
          ),
          const SizedBox(height: 16),
          Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              TextButton.icon(
                onPressed: () => _remove(context, contact),
                style: TextButton.styleFrom(
                  foregroundColor: Theme.of(context).colorScheme.error,
                ),
                label: const Text('Remove from this app'),
                icon: const Icon(Icons.delete_outline),
              ),
              TextButton.icon(
                onPressed: () => copyToClipboard(contact.address()),
                label: const Text('Copy'),
                icon: const Icon(Icons.copy),
              ),
            ],
          ),
        ],
      ),
    );
    if (mounted) setState(() {});
  }

  Future<void> _remove(BuildContext sheet, SpContactEntry contact) async {
    final name = contact.name();
    final confirmed = await showDialog<bool>(
      context: sheet,
      builder: (context) => AlertDialog(
        title: Text('Remove $name?'),
        content: const Text(
          'Adding this contact back means confirming the address on a threshold of devices again.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('Cancel'),
          ),
          TextButton(
            onPressed: () => Navigator.pop(context, true),
            child: const Text('Remove'),
          ),
        ],
      ),
    );
    if (confirmed != true) return;
    await coord.removeSpContact(keyId: _keyId, name: name);
    if (sheet.mounted) Navigator.pop(sheet);
  }

  @override
  Widget build(BuildContext context) {
    final contacts = coord.spContacts(keyId: _keyId);
    return Scaffold(
      appBar: AppBar(title: const Text('Contacts')),
      floatingActionButton: FloatingActionButton.extended(
        onPressed: _add,
        label: const Text('Add contact'),
        icon: const Icon(Icons.person_add_alt),
      ),
      body: contacts.isEmpty
          ? const Center(
              child: Padding(
                padding: EdgeInsets.all(32),
                child: Text(
                  'Silent payments go to contacts your devices have confirmed. '
                  'Add one to send a silent payment.',
                  textAlign: TextAlign.center,
                ),
              ),
            )
          : Column(
              children: [
                Padding(
                  padding: const EdgeInsets.all(16),
                  child: TextField(
                    controller: _search,
                    onChanged: (_) => setState(() {}),
                    decoration: const InputDecoration(
                      hintText: 'Search name or address',
                      prefixIcon: Icon(Icons.search),
                    ),
                  ),
                ),
                Expanded(child: _results(contacts)),
              ],
            ),
    );
  }

  Widget _results(List<SpContactEntry> contacts) {
    final matches = contacts
        .where(
          (contact) => contactMatches(
            _search.text,
            name: contact.name(),
            address: contact.address(),
          ),
        )
        .toList();
    if (matches.isEmpty) {
      return const Center(child: Text('No matching contacts'));
    }
    return ListView.builder(
      itemCount: matches.length,
      itemBuilder: (context, index) {
        final contact = matches[index];
        return ListTile(
          leading: const Icon(Icons.person_outline),
          title: Text(contact.name()),
          subtitle: Text(
            _shortAddress(contact.address()),
            style: monospaceTextStyle,
          ),
          onTap: () => _open(contact),
        );
      },
    );
  }
}

/// Asks for a contact's name and address, then has a threshold of devices confirm and sign it.
/// The name of the contact, if one was added.
Future<String?> showAddSpContact(
  BuildContext context, {
  required AccessStructureRef accessStructureRef,
  String address = '',
}) async {
  final encryptionKey = await existingWalletKey(
    context: context.mounted ? context : null,
    accessStructureRef: accessStructureRef,
    action: 'add a contact',
  );
  if (encryptionKey == null || !context.mounted) return null;
  return showBottomSheetOrDialog<String>(
    context,
    title: const Text('Add contact'),
    builder: (context, scrollController) => _AddSpContact(
      accessStructureRef: accessStructureRef,
      encryptionKey: encryptionKey,
      address: address,
      scrollController: scrollController,
    ),
  );
}

class _AddSpContact extends StatefulWidget {
  final AccessStructureRef accessStructureRef;
  final SymmetricKey encryptionKey;
  final String address;
  final ScrollController scrollController;

  const _AddSpContact({
    required this.accessStructureRef,
    required this.encryptionKey,
    required this.address,
    required this.scrollController,
  });

  @override
  State<_AddSpContact> createState() => _AddSpContactState();
}

class _AddSpContactState extends State<_AddSpContact> {
  final _name = TextEditingController();
  late final _address = TextEditingController(text: widget.address);
  StreamSubscription<SpContactState>? _ecdhSub;
  StreamSubscription<SigningState>? _signSub;
  StreamSubscription<DeviceListUpdate>? _devicesSub;
  SpContactState? _state;
  final Set<DeviceId> _connected = {};

  /// Set once the devices that answered have been asked to sign.
  SignSessionId? _signSession;
  bool _opening = false;
  bool _finished = false;
  String? _error;
  String _submittedAddress = '';

  late final _accessStructure = coord.getAccessStructure(
    asRef: widget.accessStructureRef,
  )!;
  late final _threshold = _accessStructure.threshold();
  late final _devices = _accessStructure.devices();

  @override
  void initState() {
    super.initState();
    _name.addListener(_onFormChanged);
    _address.addListener(_onFormChanged);
    _devicesSub = GlobalStreams.deviceListSubject.listen((update) {
      _connected
        ..clear()
        ..addAll(update.state.devices.map((device) => device.id));
      if (mounted) setState(() {});
    });
  }

  @override
  void dispose() {
    _devicesSub?.cancel();
    _ecdhSub?.cancel();
    _signSub?.cancel();
    _name.dispose();
    _address.dispose();
    if (!_finished) _cancel();
    super.dispose();
  }

  // The controllers also notify on selection changes, which should not clear the error.
  void _onFormChanged() {
    if (_address.text != _submittedAddress) _error = null;
    if (mounted) setState(() {});
  }

  /// Takes the devices off the prompt and drops whatever was collected.
  void _cancel() {
    if (_state == null && _signSession == null) return;
    coord.cancelProtocol();
    final sessionId = _state?.sessionId;
    if (sessionId != null) coord.cancelSpContact(sessionId: sessionId);
    final signSession = _signSession;
    if (signSession != null) coord.cancelSignSession(ssid: signSession);
  }

  void _start() {
    _submittedAddress = _address.text;
    setState(() => _error = null);
    _ecdhSub = coord
        .startSpContact(
          accessStructureRef: widget.accessStructureRef,
          name: _name.text.trim(),
          address: _address.text,
          devices: _devices,
          encryptionKey: widget.encryptionKey,
        )
        .listen(_onState, onError: _onError);
  }

  void _onError(Object error) {
    if (!mounted) return;
    _ecdhSub?.cancel();
    setState(() {
      _ecdhSub = null;
      _state = null;
      _error = contactErrorMessage(error);
    });
  }

  void _onState(SpContactState state) {
    if (!mounted || _signSession != null || _opening) return;
    setState(() => _state = state);
    for (final deviceId in state.connectedButNeedRequest) {
      try {
        coord.requestSpContactEcdh(
          sessionId: state.sessionId,
          deviceId: deviceId,
          encryptionKey: widget.encryptionKey,
        );
      } catch (e) {
        debugPrint('contact ECDH request failed: $e');
      }
    }
    if (state.confirmed.length >= _threshold) _sign(state);
  }

  /// A threshold have confirmed, so the devices sign without another hold.
  Future<void> _sign(SpContactState state) async {
    setState(() => _opening = true);
    try {
      final signSession = await coord.startSpContactSign(
        sessionId: state.sessionId,
      );
      if (!mounted) {
        await coord.cancelSignSession(ssid: signSession);
        return;
      }
      setState(() => _signSession = signSession);
      _signSub = coord
          .tryRestoreSigningSession(sessionId: signSession)
          .listen(_onSigning, onError: _onError);
    } catch (e) {
      if (!mounted) return;
      showErrorSnackbar(
        context,
        'Could not add the contact: ${contactErrorMessage(e)}',
      );
      Navigator.pop(context);
    } finally {
      _opening = false;
    }
  }

  Future<void> _onSigning(SigningState state) async {
    if (!mounted) return;
    final signSession = _signSession!;
    if (state.finishedSignatures != null) {
      await coord.finishSpContact(sessionId: signSession);
      _finished = true;
      if (mounted) Navigator.pop(context, _name.text.trim());
      return;
    }
    for (final deviceId in state.connectedButNeedRequest) {
      coord.requestDeviceSign(
        deviceId: deviceId,
        sessionId: signSession,
        encryptionKey: widget.encryptionKey,
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final started = _ecdhSub != null;
    final confirmed = _state?.confirmed ?? const [];
    final existing = coord.spContacts(keyId: widget.accessStructureRef.keyId);
    final nameError = contactNameError(
      _name.text,
      existing.map((contact) => contact.name()),
    );
    final address = _address.text.trim().toLowerCase();
    final savedAs = existing
        .where((contact) => contact.address() == address)
        .firstOrNull
        ?.name();
    return ListView(
      controller: widget.scrollController,
      shrinkWrap: true,
      padding: const EdgeInsets.all(16),
      children: [
        if (!started) ...[
          TextField(
            controller: _name,
            maxLength: 20,
            decoration: InputDecoration(
              labelText: 'Name',
              errorText: nameError,
            ),
          ),
          TextField(
            controller: _address,
            minLines: 2,
            maxLines: 5,
            decoration: InputDecoration(
              labelText: 'Silent payment address',
              errorMaxLines: 3,
              helperText: savedAs == null ? null : 'Already saved as $savedAs',
            ),
            style: monospaceTextStyle,
          ),
          if (_error case final error?)
            Padding(
              padding: const EdgeInsets.only(top: 8),
              child: Text(
                error,
                style: TextStyle(color: theme.colorScheme.error),
              ),
            ),
          const SizedBox(height: 16),
          FilledButton(
            onPressed:
                nameError != null ||
                    _name.text.trim().isEmpty ||
                    address.isEmpty
                ? null
                : _start,
            child: const Text('Confirm on devices'),
          ),
        ] else ...[
          Text(
            'Check the full address on each device against what the recipient gave you, '
            'then hold to trust it. Later sends to ${_name.text.trim()} rely on this check.',
            style: theme.textTheme.bodyMedium,
          ),
          ListTile(
            contentPadding: EdgeInsets.zero,
            title: const Text('Devices'),
            subtitle: const Text('Connect a device to confirm'),
            trailing: Text('${confirmed.length}/$_threshold'),
          ),
          for (final deviceId in _devices)
            ListTile(
              contentPadding: EdgeInsets.zero,
              enabled: _connected.contains(deviceId),
              title: Text(coord.getDeviceName(id: deviceId) ?? '<no-name>'),
              trailing: confirmed.contains(deviceId)
                  ? const AnimatedCheckCircle()
                  : Text(
                      _connected.contains(deviceId) ? 'Waiting to confirm' : '',
                      style: TextStyle(color: theme.colorScheme.primary),
                    ),
            ),
          Align(
            alignment: AlignmentDirectional.centerStart,
            child: TextButton(
              onPressed: () => Navigator.pop(context),
              style: TextButton.styleFrom(
                foregroundColor: theme.colorScheme.error,
              ),
              child: const Text('Cancel'),
            ),
          ),
        ],
      ],
    );
  }
}
