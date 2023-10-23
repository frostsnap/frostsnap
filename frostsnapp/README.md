# frostsnapp

This repository serves as a template for Flutter projects calling into native Rust
libraries via `flutter_rust_bridge`.

## Install

To begin, ensure that you have a working installation of the following items:

- [Flutter SDK](https://docs.flutter.dev/get-started/install)
- install [`just`](https://github.com/casey/just)
- [Rust language](https://rustup.rs/)
- Appropriate [Rust targets](https://rust-lang.github.io/rustup/cross-compilation.html) for cross-compiling the app for different platforms.
- For Android targets:
  - Install [Android Studio](https://docs.flutter.dev/get-started/install/linux#install-android-studio) and SDK tools required by Flutter
  - Install [Android NDK](https://github.com/android/ndk/wiki) (can be done through Android Studio > SDK Manager > SDK Tools > NDK (Side by side))
  - Install [cargo-ndk](https://github.com/bbqsrc/cargo-ndk#installing)
  - `export ANDROID_NDK_HOME=/home/$USER/Android/Sdk/ndk/<version installed>/`
- install cargo build tools: `just install-rust-deps`
- Install the following development libraries

```sh
sudo apt install -y ninja-build libstdc++-12-dev libgtk-3-0 libgtk-3-dev libudev-dev
```

```
echo "ANDROID_NDK=.." >> ~/.gradle/gradle.properties
```

## Generate bindings

On non-debian linux you will need to export your CPATH

```
export CPATH="$(clang -v 2>&1 | grep "Selected GCC installation" | rev | cut -d' ' -f1 | rev)/include"
```

```sh
just gen
```

## Run

```sh
flutter run
```

Flutter will give you an option of running on an android device if one is connected (in [debug mode](https://www.lifewire.com/enable-usb-debugging-android-46L90927)). If you can not see your device you may need to check `adb devices` ([android debug bridge](https://wiki.archlinux.org/title/Android_Debug_Bridge)) shows your device.

## Build

```
just build linux
just build apk
```

When this doesn't work figure out why and fix these instructions please. If you want to run on android it may help to open the project in android studio
