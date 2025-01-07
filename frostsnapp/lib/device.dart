import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';

class DeviceWidget extends StatelessWidget {
  final Widget child;
  const DeviceWidget({super.key, required this.child});
  final String deviceSvg = '''
<svg width="34.986689421321785mm" height="42.99022935571762mm" viewBox="0 0 34.986689421321785 42.99022935571762" xmlns="http://www.w3.org/2000/svg" version="1.1">
<g id="Binder022" transform="translate(17.493344,21.543007) scale(1,-1)">
<path id="Binder022_f0000"  d="M -14.964990406185642 18.586987365833934 C -12.222647326109056 21.262466115746093 -6.102466677437631 21.177514052687993 0.0 21.19999999999999 C 6.121690939255835 21.173197742531492 12.231613033666312 21.274073642081408 14.969082389611895 18.60771394747341 C 17.192036356405605 16.42026948598908 17.145133620721403 12.729626455124311 17.150337936776445 9.214654998137103 C 17.15016857520842 9.062541610979817 17.149999999999988 8.910928801845651 17.15 8.759999999999998 L 17.15 -8.760000000000002 C 17.15 -8.914753765468582 17.150169136378064 -9.069446409953057 17.150338175634953 -9.2239777310766 C 17.142579739222196 -12.72727276368968 17.19842080113543 -16.18678348818877 15.049795021706236 -18.379036431181923 C 12.355533216618058 -21.090033156964946 6.171452934273059 -21.04122596621769 0.0023556717517863035 -21.0999930634531 C -6.101080496066673 -21.113188247010854 -12.221201449009136 -21.235110995509284 -14.9651450300972 -18.57374603173769 C -17.20075798684543 -16.36281066897254 -17.14189748249141 -12.78040602545065 -17.150337727621697 -9.223998902144816 C -17.1501688375326 -9.069320114984011 -17.14999999999999 -8.914616780668991 -17.15 -8.76 L -17.15 8.76 C -17.149999999999984 8.914615841852937 -17.150168835482276 9.06931823718443 -17.150337724548145 9.223997026763582 C -17.14207388097222 12.781031521903902 -17.20025878454716 16.36447394034848 -14.964990406185638 18.586987365833927 Z M -14.99999999999999 15.099999999999996 A 4 4 0 0 0 -11 19.1L 10.999999999999982 19.099999999999994 A 4 4 0 0 0 15 15.1L 14.999999999999991 -11.900000000000004 A 4 4 0 0 0 11 -15.9L -10.999999999999982 -15.900000000000002 A 4 4 0 0 0 -15 -11.9L -14.999999999999991 15.099999999999996 Z " stroke="#666666" stroke-width="0.35 px" style="stroke-width:0.35;stroke-miterlimit:4;stroke-dasharray:none;stroke-linecap:square;fill:#888888;fill-opacity:0.3;fill-rule: evenodd"/>
<title>b'Binder022'</title>
</g>
<g id="Binder023" transform="translate(17.493344,21.543007) scale(1,-1)">
<path id="Binder023_f0000"  d="M -10.999999999999979 19.099999999999998 A 4 4 0 0 1 -15 15.1L -15.0 -11.9 A 4 4 0 0 1 -11 -15.9L 10.999999999999982 -15.900000000000002 A 4 4 0 0 1 15 -11.9L 15.0 15.099999999999996 A 4 4 0 0 1 11 19.1L -10.99999999999998 19.099999999999998 Z " stroke="#666666" stroke-width="0.35 px" style="stroke-width:0.35;stroke-miterlimit:4;stroke-dasharray:none;stroke-linecap:square;fill:#888888;fill-opacity:0.1;fill-rule: evenodd"/>
<title>b'Binder023'</title>
</g>
</svg>
''';
  @override
  Widget build(BuildContext context) {
    const scale = 1.22;
    const height = 122.0 * scale;
    const width = 100.0 * scale;
    return SizedBox(
        width: width,
        height: height,
        child: Stack(
          alignment: Alignment.center,
          children: [
            SvgPicture.string(
              deviceSvg,
              width: width,
              height: height,
            ),
            Padding(
              padding: EdgeInsets.symmetric(
                  vertical: 10 * scale, horizontal: 12 * scale),
              child: child,
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
  const ConfirmPrompt({super.key});

  @override
  Widget build(BuildContext context) {
    return DevicePrompt(icon: Icon(Icons.touch_app), text: "Confirm");
  }
}
