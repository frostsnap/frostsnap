import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/device_demo_playback.dart';
import 'package:frostsnap/src/rust/api/device_demo.dart';

Duration ms(int n) => Duration(milliseconds: n);

DeviceDemoCue swipe({required int contactInMs}) => DeviceDemoCue(
  contactInMs: contactInMs,
  gesture: const DeviceDemoGesture.swipe(),
);

DeviceDemoCue hold({required int contactInMs, required int holdMs}) =>
    DeviceDemoCue(
      contactInMs: contactInMs,
      gesture: DeviceDemoGesture.hold(holdMs: holdMs),
    );

/// When the hand comes to rest after a cue at zero, to the frame.
Duration settlesAt(DeviceDemoCue cue) {
  final track = HandTrack()..cue(cue, Duration.zero);
  var t = Duration.zero;
  while (track.isMovingAt(t)) {
    t += ms(1);
  }
  return t;
}

void main() {
  test('a swipe cue puts the finger on the screen when the touch lands', () {
    final track = HandTrack();
    track.cue(swipe(contactInMs: 1400), ms(500));

    expect(track.frameAt(ms(500)), HandFrame.rest);
    expect(
      track.frameAt(ms(1900)),
      const HandFrame(HandSheet.swipe, HandTrack.swipeContactFrame),
    );
  });

  test('there is no hand until a gesture is cued', () {
    expect(HandTrack().frameAt(ms(5000)), isNull);
    expect(HandTrack().isMovingAt(ms(5000)), isFalse);
  });

  test(
    'a hold cue presses when the touch lands and stays down for the hold',
    () {
      final track = HandTrack();
      track.cue(hold(contactInMs: 1500, holdMs: 8000), ms(0));

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

  test('the timing the demo scripts by is the clips\' own', () {
    final timing = HandTrack.timing;
    const oneFrame = Duration(microseconds: 1000000 ~/ HandTrack.fps);

    final swipeEnd = ms(timing.swipeContactMs + timing.swipeAfterContactMs);
    expect(
      settlesAt(swipe(contactInMs: timing.swipeContactMs)) - swipeEnd,
      lessThanOrEqualTo(oneFrame),
    );

    final holdEnd = ms(
      timing.holdContactMs + timing.holdPressMs + timing.holdAfterReleaseMs,
    );
    expect(
      (settlesAt(
                hold(
                  contactInMs: timing.holdContactMs,
                  holdMs: timing.holdPressMs,
                ),
              ) -
              holdEnd)
          .abs(),
      lessThanOrEqualTo(oneFrame),
    );
  });

  test('the hand keeps moving with nothing arriving from the device', () {
    final track = HandTrack();
    track.cue(swipe(contactInMs: 1000), ms(0));

    final frames = {
      for (var t = 0; t < 1000; t += 1000 ~/ HandTrack.fps)
        track.frameAt(ms(t)),
    };
    expect(frames.length, greaterThan(20));
    expect(track.isMovingAt(ms(999)), isTrue);
  });

  test('after its clip the hand rests in the pose every clip starts from', () {
    final track = HandTrack();
    track.cue(swipe(contactInMs: 1000), ms(0));

    expect(track.frameAt(ms(60000)), HandFrame.rest);
    expect(track.isMovingAt(ms(60000)), isFalse);
  });
}
