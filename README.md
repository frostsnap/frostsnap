# Usage

## Run the server
```
cd frost-server/
cargo run
```
you may need to `sudo ufw allow 3090`.

## Flash and run the device
Copy the config file and edit with your wifi information
and `frost-server` `http://IP_ADDRESS:3090`
```
cd frost-device/
cp cfp_example.toml cfg.toml
vim cfg.toml
```

Find device name with `ls -l /dev/serial/by-id/`. Run with
```
cargo espflash --release --monitor /dev/ttyUSB0
```


## Notes

### Resources
books
* https://esp-rs.github.io/book/overview/bare-metal.html
* https://espressif-trainings.ferrous-systems.com/04_4_1_interrupts.html


### Todo
Fix watchdog
* https://docs.espressif.com/projects/esp-idf/en/latest/esp32/api-reference/system/wdts.html

Better backtraces?

Fix reboot
* Reboot happens on some but not all panics

Can we run it no std?


### Devices
Reset pipe breaks on the longer device.
LED not changing on shorter device.
The board has a hardware random number generator. It can be called with esp_random().
No thread_rng:
https://github.com/arnauorriols/streams/commit/ef98676a3b0016d5c50ed384049a1a52448d76ec



## API outline
Laptop Rocket Frost Server: Endpoints
Devices talk to laptop
Allow for flexibile intiating parties? -> receive_poly finalises group?.

### Keygen

`/keygen?poly`
Parties submit polys -- Save to database?.. sled?

`/receive_polys`
Parties receive polys
Parties do keygen

`/send_shares?secret_shares`:
parties submit secret shrares and pops

`/receive_shares`
Allow to call endpoint that distributes them


### Signing

`/send_nonce`
Gen nonce and share

`/receive_nonces`
receive nonces

`/sendsig`
Sign and share


### Devices
Devices:
    KEYGEN //
         1. Create scalar poly and join keygen session
         2. receive polys and send secret shares
         3. receive shares and store secret

    SIGNING // 
        1. Gen nonce and share
        2. receive nonces
        3. Sign and share
        4. Collect signatures and verify



