import 'package:flutter/material.dart';

// The pieces every screen of the card flows is made of: the card flow
// (card_flow.dart) and the card restore (restoration/enter_card_view.dart).

/// An [AnimatedSwitcher] layout that keeps each screen at the top: a
/// screen shorter than the space starts under the header, not in the
/// middle of it.
Widget topSwitcherLayout(Widget? current, List<Widget> previous) =>
    Stack(alignment: Alignment.topCenter, children: [...previous, ?current]);

/// A line of a flow page, in the body style.
Widget flowBody(BuildContext context, String text) =>
    Text(text, style: Theme.of(context).textTheme.bodyLarge);

/// One screen of a flow: an optional icon or leading widget, a title, then
/// the screen's lines.
class CardFlowPage extends StatelessWidget {
  final IconData? icon;
  final Widget? leading;
  final String? title;
  final Color? titleColor;
  final bool error;
  final List<Widget> children;

  const CardFlowPage({
    super.key,
    this.icon,
    this.leading,
    this.title,
    this.titleColor,
    this.error = false,
    required this.children,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final accent = error ? theme.colorScheme.error : titleColor;
    final leading = this.leading;
    final title = this.title;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      spacing: 16,
      children: [
        if (leading != null) Center(child: leading),
        if (icon != null)
          Icon(icon, size: 56, color: accent ?? theme.colorScheme.primary),
        if (title != null)
          Text(
            title,
            style: theme.textTheme.headlineSmall?.copyWith(color: accent),
          ),
        ...children,
      ],
    );
  }
}

/// A screen that only points at the device, such as "Finish the quiz on your
/// device". It names nothing the device shows.
class DevicePointer extends StatelessWidget {
  final String text;
  final List<Widget> children;

  const DevicePointer(this.text, {super.key, this.children = const []});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      spacing: 16,
      children: [
        const SizedBox(height: 24),
        ImageIcon(
          const AssetImage('assets/icons/device2.png'),
          size: 64,
          color: theme.colorScheme.primary,
        ),
        Text(
          text,
          textAlign: TextAlign.center,
          style: theme.textTheme.headlineSmall,
        ),
        ...children,
      ],
    );
  }
}

/// A standing rule, set apart from the page's own lines.
class RuleLine extends StatelessWidget {
  final String text;

  const RuleLine(this.text, {super.key});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Container(
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: theme.colorScheme.surfaceContainerHighest,
        borderRadius: BorderRadius.circular(8),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        spacing: 12,
        children: [
          Icon(Icons.info_outline, color: theme.colorScheme.primary),
          Expanded(child: Text(text, style: theme.textTheme.bodyMedium)),
        ],
      ),
    );
  }
}

class CardBanner extends StatelessWidget {
  final Color color;
  final String text;

  const CardBanner({super.key, required this.color, required this.text});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Container(
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.12),
        border: Border.all(color: color),
        borderRadius: BorderRadius.circular(8),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        spacing: 12,
        children: [
          Icon(Icons.warning_amber_rounded, color: color),
          Expanded(child: Text(text, style: theme.textTheme.bodyMedium)),
        ],
      ),
    );
  }
}

class ErrorCard extends StatelessWidget {
  final List<String> lines;

  const ErrorCard({super.key, required this.lines});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Card.filled(
      margin: EdgeInsets.zero,
      color: theme.colorScheme.errorContainer,
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          spacing: 8,
          children: [
            for (final line in lines)
              Text(
                line,
                style: theme.textTheme.bodyLarge?.copyWith(
                  color: theme.colorScheme.onErrorContainer,
                ),
              ),
          ],
        ),
      ),
    );
  }
}

/// The experimental label, small, at the end of an intro.
class ExperimentalLabel extends StatelessWidget {
  final String text;

  const ExperimentalLabel(this.text, {super.key});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Text(
      text,
      textAlign: TextAlign.center,
      style: theme.textTheme.labelSmall?.copyWith(
        color: theme.colorScheme.onSurfaceVariant,
      ),
    );
  }
}
