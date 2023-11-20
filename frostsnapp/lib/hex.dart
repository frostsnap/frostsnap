import 'dart:typed_data';

import 'package:flutter/material.dart';

String toHex(Uint8List data) {
  return data.map((byte) => byte.toRadixString(16).padLeft(2, '0')).join(' ');
}

String toHexBox(Uint8List bytes, Orientation orientation) {
  final lineLength = orientation == Orientation.portrait ? 8 : 16;
  var buffer = StringBuffer();
  for (var i = 0; i < bytes.length; i++) {
    buffer.write(bytes[i].toRadixString(16).padLeft(2, '0').toUpperCase());
    if ((i + 1) % lineLength == 0) {
      buffer.write('\n');
    } else if ((i + 1) % 2 == 0) {
      buffer.write(' '); // Add space between bytes
    }
  }
  return buffer.toString().trim();
}
