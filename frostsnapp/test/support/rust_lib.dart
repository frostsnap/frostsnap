import 'dart:io';

import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';
import 'package:frostsnap/src/rust/frb_generated.dart';

/// Loads the debug build of the app's Rust library that `just test-app` builds.
///
/// Call once per test file, from `setUpAll`.
Future<void> initRustLibForTests() async {
  // `flutter test` runs with `frostsnapp/` as the working directory, and the
  // workspace puts the cdylib in the repo-root `target/`, not the
  // `rust/target/` that FRB's default loader looks in.
  final dir = '${Directory.current.parent.path}/target/debug/';
  if (!Directory(dir).existsSync()) {
    throw StateError('$dir is missing; run `just test-app`');
  }
  await RustLib.init(
    externalLibrary: await loadExternalLibrary(
      ExternalLibraryLoaderConfig(
        stem: 'rust_lib_frostsnapp',
        ioDirectory: dir,
        webPrefix: null,
      ),
    ),
  );
}
