#![no_std]
#![no_main]

mod temperature;
mod voltage;

use {defmt_rtt as _, embassy_nrf as _, panic_probe as _};

use core::{mem, slice};
use defmt::{info, unwrap};
use embassy_executor::Spawner;
use embassy_nrf::{
    bind_interrupts,
    interrupt::Priority,
    peripherals,
    saadc::{self, Saadc},
    twim::{self, Twim},
};
use futures::{
    future::{self, Either},
    pin_mut,
};
use nrf_softdevice::ble::advertisement_builder::{
    AdvertisementDataType, Flag, LegacyAdvertisementBuilder, LegacyAdvertisementPayload,
};
use nrf_softdevice::ble::peripheral;
use nrf_softdevice::Softdevice;

bind_interrupts!(struct Irqs {
    SAADC => saadc::InterruptHandler;
    SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0 => twim::InterruptHandler<peripherals::TWISPI0>;
});

#[repr(C)]
#[allow(dead_code)]
struct ManufData {
    id: u16,
    version: u8,
    counter: u8,
    voltage: i16,
    temperature: i16,
}

#[embassy_executor::task]
async fn softdevice_task(sd: &'static Softdevice) -> ! {
    sd.run().await
}

async fn advertise(sd: &'static Softdevice, counter: u8, voltage: i16, temperature: i16) {
    let config = peripheral::Config {
        // interval: 8000, // 5000ms
        ..Default::default()
    };

    let data = ManufData {
        id: 0xFFFF,
        version: 0x01,
        counter,
        voltage,
        temperature,
    };

    let buff = unsafe {
        slice::from_raw_parts(&data as *const _ as *const u8, mem::size_of::<ManufData>())
    };

    let adv_data: LegacyAdvertisementPayload = LegacyAdvertisementBuilder::new()
        .flags(&[Flag::GeneralDiscovery, Flag::LE_Only])
        .short_name("Tempsys")
        .raw(AdvertisementDataType::MANUFACTURER_SPECIFIC_DATA, buff)
        .build();

    let adv = peripheral::NonconnectableAdvertisement::NonscannableUndirected {
        adv_data: &adv_data,
    };

    unwrap!(peripheral::advertise(sd, adv, &config).await)
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    info!("Tempsys Start");

    // 0 is Highest. Lower prio number can preempt higher prio number
    // Softdevice has reserved priorities 0, 1 and 4
    let mut config = embassy_nrf::config::Config::default();
    config.gpiote_interrupt_priority = Priority::P2;
    config.time_interrupt_priority = Priority::P2;

    let p = embassy_nrf::init(config);

    let config = nrf_softdevice::Config::default();
    let sd = Softdevice::enable(&config);
    unwrap!(spawner.spawn(softdevice_task(sd)));

    let mut twim_config = twim::Config::default();
    twim_config.sda_pullup = true;
    twim_config.scl_pullup = true;
    let twi = Twim::new(p.TWISPI0, Irqs, p.P0_24, p.P0_13, twim_config);
    unwrap!(spawner.spawn(temperature::init(twi)));

    let adc_config = saadc::Config::default();
    let channel_config = saadc::ChannelConfig::single_ended(saadc::VddInput);
    let mut saadc = Saadc::new(p.SAADC, Irqs, adc_config, [channel_config]);
    saadc.calibrate().await;

    let mut counter = 0;
    let mut temperature = temperature::read().await;
    loop {
        // ADC
        let voltage = voltage::read(&mut saadc).await;
        defmt::info!("Voltage: {}", voltage);

        info!("Start advertising {}", counter);
        let adv_fut = advertise(sd, counter, voltage, temperature);
        pin_mut!(adv_fut);
        let temp_fut = temperature::read();
        pin_mut!(temp_fut);

        if let Either::Right((t, _)) = future::select(adv_fut, temp_fut).await {
            temperature = t;
        }
        counter = counter.wrapping_add(1);
    }
}
