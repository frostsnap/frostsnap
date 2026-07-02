import 'package:flutter/material.dart';
import 'package:frostsnap/copy_feedback.dart';
import 'package:frostsnap/fullscreen_dialog_scaffold.dart';
import 'package:frostsnap/maybe_fullscreen_dialog.dart';
import 'package:pretty_qr_code/pretty_qr_code.dart';

/// Dashed-border "Invite participants" tile used by ceremony lobbies
/// (keygen, remote recovery). Tapping opens [showInviteDialog].
class InviteTile extends StatelessWidget {
  const InviteTile({super.key, required this.onTap, this.label});
  final VoidCallback onTap;
  final String? label;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Material(
      color: Colors.transparent,
      borderRadius: BorderRadius.circular(12),
      child: InkWell(
        borderRadius: BorderRadius.circular(12),
        onTap: onTap,
        child: CustomPaint(
          painter: _DashedBorderPainter(
            color: theme.colorScheme.outline,
            radius: 12,
          ),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 16),
            child: Row(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                Icon(
                  Icons.person_add_rounded,
                  size: 20,
                  color: theme.colorScheme.primary,
                ),
                const SizedBox(width: 10),
                Text(
                  label ?? 'Invite participants',
                  style: theme.textTheme.titleSmall?.copyWith(
                    color: theme.colorScheme.primary,
                    fontWeight: FontWeight.w600,
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _DashedBorderPainter extends CustomPainter {
  const _DashedBorderPainter({required this.color, this.radius = 12});

  final Color color;
  final double radius;

  static const double _dash = 6;
  static const double _gap = 4;
  static const double _stroke = 1.5;

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..color = color
      ..strokeWidth = _stroke
      ..style = PaintingStyle.stroke;

    final rrect = RRect.fromRectAndRadius(
      Offset.zero & size,
      Radius.circular(radius),
    );
    final path = Path()..addRRect(rrect);

    for (final metric in path.computeMetrics()) {
      double distance = 0;
      while (distance < metric.length) {
        final end = distance + _dash;
        canvas.drawPath(
          metric.extractPath(distance, end.clamp(0, metric.length)),
          paint,
        );
        distance = end + _gap;
      }
    }
  }

  @override
  bool shouldRepaint(covariant _DashedBorderPainter old) =>
      old.color != color || old.radius != radius;
}

/// QR + copyable-link invite dialog shared by the ceremony lobbies.
/// The QR encodes the raw `frostsnap://…` link, which the join-side
/// `QrStringScanner` reads back verbatim.
void showInviteDialog(BuildContext context, String inviteLink) {
  MaybeFullscreenDialog.show<void>(
    context: context,
    barrierDismissible: true,
    child: _InviteDialog(inviteLink: inviteLink),
  );
}

class _InviteDialog extends StatelessWidget {
  const _InviteDialog({required this.inviteLink});
  final String inviteLink;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return FullscreenDialogScaffold(
      title: const Text('Invite participants'),
      body: SliverList.list(
        children: [
          Center(
            child: Container(
              width: 220,
              height: 220,
              padding: const EdgeInsets.all(12),
              decoration: BoxDecoration(
                color: Colors.white,
                borderRadius: BorderRadius.circular(16),
              ),
              child: PrettyQrView.data(
                data: inviteLink,
                decoration: const PrettyQrDecoration(
                  shape: PrettyQrSmoothSymbol(),
                ),
              ),
            ),
          ),
          const SizedBox(height: 16),
          SelectableText(
            inviteLink,
            textAlign: TextAlign.center,
            style: theme.textTheme.bodyMedium?.copyWith(
              color: theme.colorScheme.onSurfaceVariant,
            ),
          ),
        ],
      ),
      footer: Row(
        spacing: 12,
        children: [
          Expanded(
            child: FilledButton.tonalIcon(
              icon: const Icon(Icons.copy_rounded, size: 18),
              label: const Text('Copy'),
              onPressed: () => copyToClipboard(inviteLink),
            ),
          ),
          Expanded(
            child: FilledButton.tonalIcon(
              icon: const Icon(Icons.share_rounded, size: 18),
              label: const Text('Share invite'),
              onPressed: () {
                ScaffoldMessenger.of(context).showSnackBar(
                  const SnackBar(
                    content: Text('Share not wired up yet'),
                    duration: Duration(seconds: 2),
                  ),
                );
              },
            ),
          ),
        ],
      ),
    );
  }
}
