# frostsnapp

This repository serves as a template for Flutter projects calling into native Rust
libraries via `flutter_rust_bridge`.

## Install

To begin, ensure that you have a working installation of the following items:
- [Flutter SDK](https://docs.flutter.dev/get-started/install)
- [Rust language](https://rustup.rs/)
- `flutter_rust_bridge_codegen` [cargo package](https://cjycode.com/flutter_rust_bridge/integrate/deps.html#build-time-dependencies) -- **Make sure to install the same version as in the project's [`Cargo.lock`](../Cargo.lock)**  
- Appropriate [Rust targets](https://rust-lang.github.io/rustup/cross-compilation.html) for cross-compiling to your device
- For Android targets:
    - Install [cargo-ndk](https://github.com/bbqsrc/cargo-ndk#installing)
    - Install [Android NDK 22](https://github.com/android/ndk/wiki/Unsupported-Downloads#r22b), then put its path in one of the `gradle.properties`, e.g.:
- install [cargo expand](https://github.com/dtolnay/cargo-expand)

```
echo "ANDROID_NDK=.." >> ~/.gradle/gradle.properties
```

## Generate bindings

``` sh
just gen
```

## Run

``` sh
flutter run
```

When this doesn't work figure out why and fix these instructions please. If you want to run on android it may help to open the project in android studio
