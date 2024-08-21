import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';

class DeviceWidget extends StatelessWidget {
  final Widget child;

  const DeviceWidget({super.key, required this.child});

  final String deviceSvg = '''
<svg
  width="100"
  height="110"
  viewBox="0 0 100 110"
  xmlns="http://www.w3.org/2000/svg"
>
  <!-- Outer rounded rectangle for the device body -->
  <rect
    x="1"
    y="1"
    rx="12"
    ry="12"
    width="98"
    height="108"
    fill="#777777"
    stroke="#000000"
    stroke-width="2"
  />

  <!-- Screen area with a higher proportion of the device -->
  <rect
    x="6"
    y="6"
    rx="8"
    ry="8"
    width="88"
    height="90"
    fill="#e6e6e6"
  />
</svg>
''';

  @override
  Widget build(BuildContext context) {
    const scale = 1.2;
    return SizedBox(
        width: 100 * scale,
        height: 110 * scale,
        child: Stack(
          alignment: Alignment.center,
          children: [
            SvgPicture.string(
              deviceSvg,
              width: 100 * scale,
              height: 110 * scale,
            ),
            Padding(
              padding: EdgeInsets.symmetric(
                  vertical: 8 * scale, horizontal: 8 * scale),
              child: DefaultTextStyle(
                style: TextStyle(color: Colors.black, fontSize: 18.0),
                child: child,
              ),
            ),
          ],
        ));
  }
}

class DevicePrompt extends StatelessWidget {
  final Widget icon;
  final String text;

  const DevicePrompt({super.key, required this.icon, required this.text});

  @override
  Widget build(BuildContext context) {
    return FittedBox(
      fit: BoxFit.contain,
      child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [icon, SizedBox(width: 4), Text(text)]),
    );
  }
}

class ConfirmPrompt extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    return DevicePrompt(
        icon: Icon(Icons.touch_app, color: Colors.orange), text: "Confirm");
  }
}
