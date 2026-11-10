import 'package:flutter/foundation.dart';

/// Whether this platform offers the card flow and card restore. For now that
/// is Android and Linux only (design §8): the platforms whose capture rules
/// are built (spec §0.6.1.5: FLAG_SECURE on Android, a warning on Linux).
/// Elsewhere the restore chooser's card is hidden, and a card key's Backup
/// button shows [cardFlowsUnsupportedMessage].
bool get cardFlowsSupported =>
    !kIsWeb &&
    (defaultTargetPlatform == TargetPlatform.android ||
        defaultTargetPlatform == TargetPlatform.linux);

/// What an existing card key shows where card flows aren't offered (design
/// §8).
const cardFlowsUnsupportedMessage =
    "Card backups aren't supported on this platform yet.";
