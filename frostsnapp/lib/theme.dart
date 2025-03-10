import 'dart:ui';

import 'package:flutter/material.dart';
import 'package:google_fonts/google_fonts.dart';

final monospaceTextStyle = GoogleFonts.notoSansMono();

final blurFilter = ImageFilter.blur(sigmaX: 2.1, sigmaY: 2.1);

Color tintSurfaceContainer(
  BuildContext context,
  Color tint,
  double? elevation,
) {
  final theme = Theme.of(context);
  return ElevationOverlay.applySurfaceTint(
    theme.colorScheme.surfaceContainer,
    tint,
    elevation ?? 3.0,
  );
}

Future<T?> showBottomSheetOrDialog<T>(
  BuildContext context, {
  required Widget Function(BuildContext) builder,
  Color? dialogBackgroundColor,
}) {
  final mediaSize = MediaQuery.sizeOf(context);

  if (mediaSize.width < 600) {
    return showModalBottomSheet<T>(
      context: context,
      isScrollControlled: true,
      useSafeArea: true,
      isDismissible: true,
      showDragHandle: false,
      builder: (context) => builder(context),
    );
  } else {
    return showDialog<T>(
      context: context,
      builder:
          (context) => Dialog(
            backgroundColor: dialogBackgroundColor,
            child: ConstrainedBox(
              constraints: BoxConstraints(maxWidth: 560),
              child: builder(context),
            ),
          ),
    );
  }
}
