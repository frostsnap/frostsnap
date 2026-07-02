import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnap/camera/camera.dart';
import 'package:frostsnap/maybe_fullscreen_dialog.dart';

/// Text field for a `frostsnap://…` invite link with Paste + Scan
/// affordances. Extracted from `OrgKeygenPage`'s `_JoinSessionInput`
/// so both the keygen flow and the unified homepage join dialog
/// share the same paste + scan UI.
///
/// Owner supplies the [controller]. On focus, if the field is empty
/// and [defaultPrefix] is non-null, the field is auto-filled with
/// that prefix and the caret placed at the end — same one-time
/// courtesy the keygen input had. Pass null to leave the field
/// untouched (used by the universal join dialog, where any of
/// several prefixes is valid).
class InviteLinkInput extends StatefulWidget {
  const InviteLinkInput({
    super.key,
    required this.controller,
    required this.onSubmit,
    this.errorText,
    this.defaultPrefix,
    this.hintText,
    this.scanTitle = 'Scan invite',
  });

  final TextEditingController controller;
  final VoidCallback onSubmit;
  final String? errorText;

  /// Optional one-shot autofill placed in [controller] the first
  /// time the field gains focus while empty.
  final String? defaultPrefix;

  final String? hintText;
  final String scanTitle;

  @override
  State<InviteLinkInput> createState() => _InviteLinkInputState();
}

class _InviteLinkInputState extends State<InviteLinkInput> {
  bool _prefilled = false;
  final _focusNode = FocusNode();

  @override
  void initState() {
    super.initState();
    _focusNode.addListener(_onFocus);
  }

  @override
  void dispose() {
    _focusNode.removeListener(_onFocus);
    _focusNode.dispose();
    super.dispose();
  }

  void _onFocus() {
    if (!_focusNode.hasFocus) return;
    final prefix = widget.defaultPrefix;
    if (prefix == null) return;
    if (_prefilled || widget.controller.text.isNotEmpty) return;
    _prefilled = true;
    widget.controller.text = prefix;
    widget.controller.selection = TextSelection.collapsed(
      offset: prefix.length,
    );
  }

  Future<void> _paste() async {
    final data = await Clipboard.getData('text/plain');
    if (data?.text != null) {
      widget.controller.text = data!.text!;
    }
  }

  Future<void> _scan() async {
    final scanned = await MaybeFullscreenDialog.show<String>(
      context: context,
      child: QrStringScanner(title: widget.scanTitle),
    );
    if (!mounted || scanned == null) return;
    widget.controller.text = scanned.trim();
    widget.onSubmit();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Card.outlined(
      color: theme.colorScheme.surfaceContainerHigh,
      margin: EdgeInsets.zero,
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            TextField(
              autofocus: true,
              focusNode: _focusNode,
              controller: widget.controller,
              decoration: InputDecoration(
                filled: false,
                border: OutlineInputBorder(
                  borderRadius: BorderRadius.circular(8),
                  borderSide: BorderSide.none,
                ),
                hintText: widget.hintText ?? widget.defaultPrefix,
                errorText: widget.errorText,
                errorMaxLines: 2,
              ),
              onSubmitted: (_) => widget.onSubmit(),
            ),
            const SizedBox(height: 4),
            Row(
              children: [
                TextButton.icon(
                  onPressed: _paste,
                  icon: const Icon(Icons.paste),
                  label: const Text('Paste'),
                ),
                TextButton.icon(
                  onPressed: _scan,
                  icon: const Icon(Icons.qr_code_scanner_rounded),
                  label: const Text('Scan'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
