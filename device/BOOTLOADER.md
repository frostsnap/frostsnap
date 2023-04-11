# ESP32-C3 Custom Bootloader
2nd stage ESP32-C3 custom bootloader from our fork of ESP-IDF. The device needs a bootloader with no logs and doesn't disable HWRNG.

To include bootloader.bin in firmware upload: `cargo espflash --bootloader .\bootloader.bin`

bootloader.debug.bin displays default logs.

## Build from source
1. [Install OS prerequisites](https://docs.espressif.com/projects/esp-idf/en/release-v5.0/esp32c3/get-started/linux-macos-setup.html#step-1-install-prerequisites)

2. Get Frostkey ESP-IDF fork
```bash
mkdir -p ~/esp
cd ~/esp
git clone --recursive https://github.com/frostkey/esp-idf.git
``` 

3. Set up the tools
```bash
cd ~/esp/esp-idf
./install.sh esp32c3
```

4. Set up the environment variables, then copy the hello_world example
```bash
. $HOME/esp/esp-idf/export.sh
cd ~/esp
cp -r $IDF_PATH/examples/get-started/hello_world .
```

5. Configure and build
```bash
cd ~/esp/hello_world
idf.py set-target esp32c3
idf.py build
```

6. `cd build/bootloader` then copy bootloader.bin into crate directory