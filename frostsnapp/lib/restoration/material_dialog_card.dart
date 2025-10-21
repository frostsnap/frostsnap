import 'package:flutter/material.dart';

class MaterialDialogCard extends StatelessWidget {
  final IconData? iconData;
  final Widget title;
  final Widget content;
  final List<Widget> actions;
  final MainAxisAlignment actionsAlignment;
  final Color? backgroundColor;
  final Color? textColor;
  final Color? variantTextColor;
  final Color? iconColor;

  const MaterialDialogCard({
    super.key,
    this.iconData,
    required this.title,
    required this.content,
    required this.actions,
    this.actionsAlignment = MainAxisAlignment.end,
    this.backgroundColor,
    this.textColor,
    this.variantTextColor,
    this.iconColor,
  });

  static const borderRadius = BorderRadius.all(Radius.circular(24));

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Card(
      color: backgroundColor ?? theme.colorScheme.surfaceContainerHigh,
      shape: RoundedRectangleBorder(borderRadius: borderRadius),
      child: Padding(
        padding: EdgeInsets.all(24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          spacing: 16,
          children: [
            if (iconData != null)
              Icon(
                iconData,
                color: iconColor ?? theme.colorScheme.secondary,
                size: 24,
              ),
            DefaultTextStyle(
              style: theme.textTheme.headlineSmall!.copyWith(
                color: textColor ?? theme.colorScheme.onSurface,
              ),
              textAlign: TextAlign.center,
              child: title,
            ),
            DefaultTextStyle(
              style: theme.textTheme.bodyLarge!.copyWith(
                color: variantTextColor ?? theme.colorScheme.onSurfaceVariant,
              ),
              textAlign: TextAlign.start,
              child: content,
            ),
            Padding(
              padding: EdgeInsets.only(top: 8),
              child: Row(
                mainAxisAlignment: actionsAlignment,
                spacing: 8,
                children: actions.map((w) => Flexible(child: w)).toList(),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
