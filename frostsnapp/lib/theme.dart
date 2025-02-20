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
