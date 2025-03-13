import 'dart:ui';

import 'package:flutter/material.dart';
import 'package:google_fonts/google_fonts.dart';

final monospaceTextStyle = GoogleFonts.notoSansMono();

final blurFilter = ImageFilter.blur(sigmaX: 2.1, sigmaY: 2.1);

Color tintSurfaceContainer(
  BuildContext context, {
  required Color tint,
  double elevation = 3.0,
}) => ElevationOverlay.applySurfaceTint(
  Theme.of(context).colorScheme.surfaceContainer,
  tint,
  elevation,
);

Color tintOnSurface(
  BuildContext context, {
  required Color tint,
  double elevation = 3.0,
}) => ElevationOverlay.applySurfaceTint(
  Theme.of(context).colorScheme.onSurface,
  tint,
  elevation,
);

Future<T?> showBottomSheetOrDialog<T>(
  BuildContext context, {
  required Widget Function(BuildContext) builder,
  Color? backgroundColor,
}) {
  final mediaSize = MediaQuery.sizeOf(context);
  backgroundColor =
      backgroundColor ?? Theme.of(context).colorScheme.surfaceContainerLow;

  if (mediaSize.width < 600) {
    return showModalBottomSheet<T>(
      context: context,
      backgroundColor: backgroundColor,
      isScrollControlled: true,
      useSafeArea: true,
      isDismissible: true,
      showDragHandle: false,
      builder: builder,
    );
  } else {
    return showDialog<T>(
      context: context,
      builder:
          (context) => Dialog(
            backgroundColor: backgroundColor,
            child: ConstrainedBox(
              constraints: BoxConstraints(maxWidth: 560),
              child: Builder(builder: builder),
            ),
          ),
    );
  }
}

String spacedHex(String input, {int spacing = 4}) {
  StringBuffer result = StringBuffer();

  for (int i = 0; i < input.length; i++) {
    result.write(input[i]);

    // Add a space after every x characters
    if ((i + 1) % spacing == 0) result.write(' ');
  }

  // Ensure the last group has exactly x characters by adding spaces
  int remainder = input.length % spacing;
  if (remainder > 0) {
    for (int i = 0; i < spacing - remainder; i++) {
      result.write('\u00A0');
    }
  }
  return result.toString();
}
