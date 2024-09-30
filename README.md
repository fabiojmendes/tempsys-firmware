<!-- vim: set tw=80: -->

# Tempsys Firmware

Bluetooth LE temperature sensor using embassy-rs. This is the result of a
direct port of a very [similar project](https://github.com/fabiojmendes/ble-temp-sensor)
using Zephyr RTOS.

This time around the language of choice is Rust and the [embassy](https://github.com/embassy-rs)
ecosystem.

## Overview

The main purpose of this firmware is to interface with the MCP9808 sensor and
emit Bluetooth LE advertising packets that can be collected by
[tempsys-scan](https://github.com/fabiojmendes/tempsys-scan).

## Advertising Packet Format

| Manufacturer Id | Version | Counter | Voltage | Temperature |
| --------------- | ------- | ------- | ------- | ----------- |
| u16             | u8      | u8      | u16     | i16         |

- Manufacturer Id is fixed to 0xFFFF for testing purposes.
- Version of this packet format, currently 1.
- This counter is incremented every time the firmware performs a reading. It
  will wrap around once it spill over.
- Voltage: 16 bit unsigned value of the battery voltage in millivolts.
- Temperature: 16 bit signed value \* 2^10 of of the temperature in Celsius.

> [!WARNING]
> If the temperature reading is equal to `i16::MAX` an error has occurred and
> this value should be discarded.

## Steps for flashing

- Download the latest release of this package
- Download the nRF softdevice S113 from [here](https://www.nordicsemi.com/Products/Development-software/s113/download)

Execute these commands using [probe-rs](https://probe.rs)

```shell
probe-rs erase --chip nrf52840_xxAA
probe-rs download --verify --binary-format hex --chip nRF52840_xxAA s113_nrf52_7.X.X_softdevice.hex
probe-rs run --chip nrf52840_xxAA tempsys-firmware
```

## TODO

- Migrate to postcard for serialization and return a proper Result instead of
  `i16::MAX` to denote errors.
