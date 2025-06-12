# Frostsnap

This repository serves as a template for Flutter projects calling into native Rust
libraries via `flutter_rust_bridge`.

## Steps to build

###  Install the flutter SDK

https://docs.flutter.dev/get-started/install

### Install flutter rust bridge

Run `just install-cargo-bins` which will install all the necessary cargo binaries included `flutter_rust_bridge_codegen`.

Make a test app with `flutter_rust_bridge_codegen create testapp`

And make sure you can run the app for the platform you want to build on.

See the [`flutter_rust_bridge`](https://cjycode.com/flutter_rust_bridge/) website for up to date information on how set up your environment


### Get a RISCV c compiler


```
just fetch-riscv
```

And add the downloaded riscv toolchain to your path.


### Generate the bindings


```sh
just gen
```

## build



```
just build linux
just build apk
```

Or run it:

```
just run
```

Flutter will give you an option of running on an android device if one is connected (in [debug mode](https://www.lifewire.com/enable-usb-debugging-android-46L90927)). If you can not see your device you may need to check `adb devices` ([android debug bridge](https://wiki.archlinux.org/title/Android_Debug_Bridge)) shows your device.

## Build

When this doesn't work figure out why and fix these instructions please. If you want to run on android it may help to open the project in android studio.
