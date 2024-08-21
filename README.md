# Tempsys Firmware

Bluetooth LE temperature sensor using embassy-rs

## Steps for flashing

- Download the latest release of this package
- Download the nRF softdevice S113 from [here](https://www.nordicsemi.com/Products/Development-software/s113/download)

Execute these commands using [probe-rs](https://probe.rs)

```shell
probe-rs erase --chip nrf52840_xxAA
probe-rs download --verify --binary-format hex --chip nRF52840_xxAA s113_nrf52_7.X.X_softdevice.hex
probe-rs run --chip nrf52840_xxAA tempsys-firmware
```
