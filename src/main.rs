#![no_std]
#![no_main]

use {defmt_rtt as _, embassy_nrf as _, panic_probe as _};

use core::{mem, slice};

use defmt::{info, unwrap};
use embassy_executor::Spawner;
use embassy_nrf::interrupt::Priority;
use embassy_nrf::saadc::{self, Saadc};
use embassy_nrf::twim::Twim;
use embassy_nrf::{bind_interrupts, peripherals, twim};
use futures::future::Either;
use futures::{future, pin_mut};
use nrf_softdevice::ble::advertisement_builder::{
    AdvertisementDataType, Flag, LegacyAdvertisementBuilder, LegacyAdvertisementPayload,
};
use nrf_softdevice::ble::peripheral;
use nrf_softdevice::{raw, Softdevice};

mod temperature;

static ADC_REF_VOLTAGE: i32 = 600;
static ADC_GAIN: i32 = 6;
static ADC_RESOLUTION: i32 = 12;

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

    let adv_data: LegacyAdvertisementPayload = LegacyAdvertisementBuilder::new()
        .flags(&[Flag::GeneralDiscovery, Flag::LE_Only])
        .short_name("Tempsys")
        .build();

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

    // but we can put it in the scan data
    // so the full name is visible once connected
    let scan_data: LegacyAdvertisementPayload = LegacyAdvertisementBuilder::new()
        .raw(AdvertisementDataType::MANUFACTURER_SPECIFIC_DATA, buff)
        .build();

    let adv = peripheral::NonconnectableAdvertisement::ScannableUndirected {
        adv_data: &adv_data,
        scan_data: &scan_data,
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

    let config = nrf_softdevice::Config {
        clock: Some(raw::nrf_clock_lf_cfg_t {
            source: raw::NRF_CLOCK_LF_SRC_RC as u8,
            rc_ctiv: 16,
            rc_temp_ctiv: 2,
            accuracy: raw::NRF_CLOCK_LF_ACCURACY_500_PPM as u8,
        }),
        conn_gap: Some(raw::ble_gap_conn_cfg_t {
            conn_count: 6,
            event_length: 24,
        }),
        conn_gatt: Some(raw::ble_gatt_conn_cfg_t { att_mtu: 256 }),
        gatts_attr_tab_size: Some(raw::ble_gatts_cfg_attr_tab_size_t {
            attr_tab_size: raw::BLE_GATTS_ATTR_TAB_SIZE_DEFAULT,
        }),
        gap_role_count: Some(raw::ble_gap_cfg_role_count_t {
            adv_set_count: 1,
            periph_role_count: 3,
            central_role_count: 3,
            central_sec_count: 0,
            _bitfield_1: raw::ble_gap_cfg_role_count_t::new_bitfield_1(0),
        }),
        gap_device_name: Some(raw::ble_gap_cfg_device_name_t {
            p_value: b"Tempsys" as *const u8 as _,
            current_len: 7,
            max_len: 7,
            write_perm: unsafe { mem::zeroed() },
            _bitfield_1: raw::ble_gap_cfg_device_name_t::new_bitfield_1(
                raw::BLE_GATTS_VLOC_STACK as u8,
            ),
        }),
        ..Default::default()
    };

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
        let voltage = {
            let mut buf = [0; 1];
            saadc.sample(&mut buf).await;
            let [raw] = buf;
            // convert raw input to millivolts
            let voltage = (raw as i32 * ADC_REF_VOLTAGE * ADC_GAIN) >> ADC_RESOLUTION;
            info!("Voltage: raw = {}, converted = {}", raw, voltage);
            voltage as i16
        };

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
