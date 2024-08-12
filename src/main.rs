#![no_std]
#![no_main]

use {defmt_rtt as _, embassy_nrf as _, panic_probe as _};

use core::mem;

use defmt::{info, *};
use embassy_executor::Spawner;
use embassy_nrf::interrupt::Priority;
use embassy_time::Timer;
use futures::{future, pin_mut};
use nrf_softdevice::ble::advertisement_builder::{
    AdvertisementDataType, Flag, LegacyAdvertisementBuilder, LegacyAdvertisementPayload,
};
use nrf_softdevice::ble::peripheral;
use nrf_softdevice::{raw, Softdevice};

#[embassy_executor::task]
async fn softdevice_task(sd: &'static Softdevice) -> ! {
    sd.run().await
}

async fn advertise(sd: &'static Softdevice, counter: u8) {
    let config = peripheral::Config {
        // interval: 8000, // 5000ms
        interval: 400, // 5000ms
        ..Default::default()
    };

    let adv_data: LegacyAdvertisementPayload = LegacyAdvertisementBuilder::new()
        .flags(&[Flag::GeneralDiscovery, Flag::LE_Only])
        .short_name("Hello")
        .build();

    let mut buff: [u8; 3] = [0xFF; 3];
    buff[2] = counter;

    // but we can put it in the scan data
    // so the full name is visible once connected
    let scan_data: LegacyAdvertisementPayload = LegacyAdvertisementBuilder::new()
        .full_name("Hello, Rust Bare!")
        .raw(AdvertisementDataType::MANUFACTURER_SPECIFIC_DATA, &buff)
        .build();

    let adv = peripheral::NonconnectableAdvertisement::ScannableUndirected {
        adv_data: &adv_data,
        scan_data: &scan_data,
    };

    unwrap!(peripheral::advertise(sd, adv, &config).await)
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    info!("Hello World!");

    // 0 is Highest. Lower prio number can preempt higher prio number
    // Softdevice has reserved priorities 0, 1 and 4
    let mut config = embassy_nrf::config::Config::default();
    config.gpiote_interrupt_priority = Priority::P2;
    config.time_interrupt_priority = Priority::P2;
    let _peripherals = embassy_nrf::init(config);

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
            p_value: b"HelloRust" as *const u8 as _,
            current_len: 9,
            max_len: 9,
            write_perm: unsafe { mem::zeroed() },
            _bitfield_1: raw::ble_gap_cfg_device_name_t::new_bitfield_1(
                raw::BLE_GATTS_VLOC_STACK as u8,
            ),
        }),
        ..Default::default()
    };

    let sd = Softdevice::enable(&config);
    unwrap!(spawner.spawn(softdevice_task(sd)));

    let mut counter = 0;
    loop {
        info!("Start advertising {}", counter);
        let adv_fut = advertise(sd, counter);
        let update_counter = async {
            counter = counter.wrapping_add(1);
            Timer::after_secs(30).await;
        };

        pin_mut!(adv_fut);
        pin_mut!(update_counter);
        future::select(adv_fut, update_counter).await;
    }
}
