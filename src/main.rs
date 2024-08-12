#![no_std]
#![no_main]

use {defmt_rtt as _, embassy_nrf as _, panic_probe as _};

use core::mem;

use defmt::{info, *};
use embassy_executor::Spawner;
use embassy_nrf::interrupt::Priority;
use embassy_nrf::saadc::{self, Saadc};
use embassy_nrf::twim::Twim;
use embassy_nrf::{bind_interrupts, peripherals, twim};
use embassy_time::{Duration, Timer};
use futures::{future, pin_mut};
use nrf_softdevice::ble::advertisement_builder::{
    AdvertisementDataType, Flag, LegacyAdvertisementBuilder, LegacyAdvertisementPayload,
};
use nrf_softdevice::ble::peripheral;
use nrf_softdevice::{raw, Softdevice};

static ADC_REF_VOLTAGE: i32 = 600;
static ADC_GAIN: i32 = 6;
static ADC_RESOLUTION: i32 = 12;

const MCP9808_ADDRESS: u8 = 0x18;

static SAMPLE_RATE: Duration = Duration::from_secs(30);

bind_interrupts!(struct Irqs {
    SAADC => saadc::InterruptHandler;
    SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0 => twim::InterruptHandler<peripherals::TWISPI0>;
});

#[embassy_executor::task]
async fn softdevice_task(sd: &'static Softdevice) -> ! {
    sd.run().await
}

async fn advertise(sd: &'static Softdevice, counter: u8, mv: i16, temp: i16) {
    let config = peripheral::Config {
        interval: 8000, // 5000ms
        ..Default::default()
    };

    let adv_data: LegacyAdvertisementPayload = LegacyAdvertisementBuilder::new()
        .flags(&[Flag::GeneralDiscovery, Flag::LE_Only])
        .short_name("Tempsys")
        .build();

    let mut buff: [u8; 8] = [0xFF; 8];
    buff[2] = 0x01;
    buff[3] = counter;
    buff[4] = (mv >> 8) as u8;
    buff[5] = (mv & 0xFF) as u8;
    buff[6] = (temp >> 8) as u8;
    buff[7] = (temp & 0xFF) as u8;

    // but we can put it in the scan data
    // so the full name is visible once connected
    let scan_data: LegacyAdvertisementPayload = LegacyAdvertisementBuilder::new()
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

    let adc_config = saadc::Config::default();
    let channel_config = saadc::ChannelConfig::single_ended(saadc::VddInput);
    let mut saadc = Saadc::new(p.SAADC, Irqs, adc_config, [channel_config]);

    saadc.calibrate().await;

    let mut twim_config = twim::Config::default();
    twim_config.sda_pullup = true;
    twim_config.scl_pullup = true;

    let mut twi = Twim::new(p.TWISPI0, Irqs, p.P0_24, p.P0_13, twim_config);
    let mut buf = [0u8; 2];
    match twi.write(MCP9808_ADDRESS, &[0x08, 0x00]).await {
        Ok(_) => defmt::info!("Set resolution"),
        Err(e) => defmt::error!("Error reading from sensor: {}", e),
    }

    let mut counter = 0;
    loop {
        // Temp Sensor
        if let Err(e) = twi.write(MCP9808_ADDRESS, &[0x01, 0x00, 0x00]).await {
            defmt::error!("Error reading from sensor: {}", e)
        }
        Timer::after_millis(45).await;
        let temp = match twi.write_read(MCP9808_ADDRESS, &[0x05], &mut buf).await {
            Ok(_) => {
                // Conversion code based on the datasheet
                // https://ww1.microchip.com/downloads/en/DeviceDoc/25095A.pdf pg25
                let [mut upper, lower] = buf;
                upper &= 0x1f; // clear flag bits
                let temp = if (upper & 0x10) == 0x10 {
                    upper &= 0x0f; // clear sign bit
                    256.0 - (upper as f32 * 16.0 + lower as f32 / 16.0)
                } else {
                    upper as f32 * 16f32 + lower as f32 / 16f32
                };
                let temp = (temp * 100.0) as i16;
                defmt::info!("Temperature: {}", temp);
                temp
            }
            Err(e) => {
                defmt::error!("Error reading from sensor: {}", e);
                i16::MIN
            }
        };
        if let Err(e) = twi.write(MCP9808_ADDRESS, &[0x01, 0x01, 0x00]).await {
            defmt::error!("Error reading from sensor: {}", e)
        }

        // ADC
        let mut buf = [0; 1];
        saadc.sample(&mut buf).await;
        let [raw] = buf;
        let mv = (raw as i32 * ADC_REF_VOLTAGE * ADC_GAIN) >> ADC_RESOLUTION;
        info!("Voltage: raw = {}, converted = {}", raw, mv);

        info!("Start advertising {}", counter);
        let adv_fut = advertise(sd, counter, mv as i16, temp);

        let update_counter = async {
            counter = counter.wrapping_add(1);
            Timer::after(SAMPLE_RATE).await;
        };

        pin_mut!(adv_fut);
        pin_mut!(update_counter);

        future::select(adv_fut, update_counter).await;
    }
}
