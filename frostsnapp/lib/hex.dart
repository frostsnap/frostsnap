import 'dart:typed_data';

String toHex(Uint8List data) {
  return data.map((byte) => byte.toRadixString(16).padLeft(2, '0')).join(' ');
}

String toHexBox(Uint8List bytes) {
  const int lineLength = 8;
  var buffer = StringBuffer();
  for (var i = 0; i < bytes.length; i++) {
    buffer.write(bytes[i].toRadixString(16).padLeft(2, '0').toUpperCase());
    if ((i + 1) % lineLength == 0) {
      buffer.write('\n');
    } else {
      buffer.write(' '); // Add space between bytes
    }
  }
  return buffer.toString().trim();
}
