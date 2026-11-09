import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/device_demo_playback.dart';
import 'package:frostsnap/src/rust/api/device_demo.dart';

Duration ms(int n) => Duration(milliseconds: n);

void main() {
  test('a swipe cue puts the finger on the screen when the touch lands', () {
    final track = HandTrack();
    track.cue(const DeviceDemoEvent.swipe(contactInMs: 1400), ms(500));

    expect(track.frameAt(ms(500)), HandFrame.rest);
    expect(
      track.frameAt(ms(1900)),
      const HandFrame(HandSheet.swipe, HandTrack.swipeContactFrame),
    );
  });

  test('there is no hand until a gesture is cued', () {
    expect(HandTrack().frameAt(ms(5000)), isNull);
  });

  test(
    'a hold cue presses when the touch lands and stays down for the hold',
    () {
      final track = HandTrack();
      track.cue(
        const DeviceDemoEvent.hold(contactInMs: 1500, holdMs: 8000),
        ms(0),
      );

      expect(
        track.frameAt(ms(1500)),
        const HandFrame(HandSheet.hold, HandTrack.holdContactFrame),
      );
      expect(
        track.frameAt(ms(1500 + 4000)),
        const HandFrame(HandSheet.hold, 90),
      );
      // Frame 137 is where the finger lifts in the clip.
      expect(
        track.frameAt(ms(1500 + 8000)),
        const HandFrame(HandSheet.hold, 137),
      );
    },
  );

  test('the hand keeps moving with nothing arriving from the device', () {
    final track = HandTrack();
    track.cue(const DeviceDemoEvent.swipe(contactInMs: 1000), ms(0));

    final frames = {
      for (var t = 0; t < 1000; t += 1000 ~/ HandTrack.fps)
        track.frameAt(ms(t)),
    };
    expect(frames.length, greaterThan(20));
  });

  test('after its clip the hand waits in the pose every clip starts from', () {
    final track = HandTrack();
    track.cue(const DeviceDemoEvent.swipe(contactInMs: 1000), ms(0));

    expect(track.frameAt(ms(60000)), HandFrame.rest);
  });
}
