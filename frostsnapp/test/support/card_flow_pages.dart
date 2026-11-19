import 'package:frostsnap/src/rust/api/verifiable_card.dart';

// Bridge-shaped card flow states for the flow tests.

/// Android's usual insets: the status bar and the gesture bar.
const statusBarHeight = 24.0;
const navigationBarHeight = 24.0;

/// The seal's size the fake drivers give: the real card's (`card_seal_size`
/// on the bridge, checked against it in rust_lib_load_test.dart).
const fakeSealSize = 311;

CardFlowState cardFlowState(
  CardFlowStage stage, {
  bool connected = true,
  int points = fakeSealSize,
  int proofs = 0,
  int boxes = 0,
  bool started = false,
  bool halves = false,
  bool deviceDone = false,
  bool recordChanged = false,
  bool unverified = false,
}) => CardFlowState(
  stage: stage,
  deviceConnected: connected,
  pointsReceived: points,
  proofsVerified: proofs,
  checkStarted: started,
  boxesDone: boxes,
  halvesRecorded: halves,
  recheckDeviceDone: deviceDone,
  recordChanged: recordChanged,
  unverified: unverified,
);

/// Words 33–64 of a made-up card, every one a backup word.
const fakePhoneHalf = <String>[
  'SIZE', 'CLOCK', 'MARGIN', 'ORBIT', 'TRIBE', 'VALVE', 'HOLIDAY', 'LEMON', //
  'PUZZLE', 'ANCHOR', 'GRAVITY', 'BUNDLE', 'CANYON', 'DOLPHIN', 'EMBRACE', //
  'FABRIC', 'GLIMPSE', 'HARVEST', 'IMPULSE', 'JUNGLE', 'KITCHEN', 'LAPTOP', //
  'MEADOW', 'NEPHEW', 'OYSTER', 'PELICAN', 'QUANTUM', 'RIBBON', 'SALMON', //
  'TORNADO', 'UNVEIL', 'VOYAGE',
];
