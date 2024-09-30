use embassy_nrf::twim::{self, Twim};
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::Timer;

use crate::constants::{SAMPLE_RATE, WAKEUP_DELAY};

const MCP9808_ADDRESS: u8 = 0x18;
const CONFIG_REG: u8 = 0x01;
const TEMPERATURE_REG: u8 = 0x05;
const RESOLUTION_REG: u8 = 0x08;

static SHARED: Signal<ThreadModeRawMutex, i16> = Signal::new();

pub async fn read() -> i16 {
    SHARED.wait().await
}

#[embassy_executor::task]
pub async fn init(mut twi: Twim<'static, impl twim::Instance>) -> ! {
    let mut set_resolution = true;
    loop {
        if set_resolution {
            if let Err(e) = setup_temp_reader(&mut twi).await {
                defmt::error!("Error setting sensor resolution {}", e)
            } else {
                set_resolution = false;
            }
        }
        let temperature = match read_temperature(&mut twi).await {
            Ok(temp) => temp,
            Err(e) => {
                defmt::error!("Error reading from sensor: {}", e);
                set_resolution = true;
                i16::MAX
            }
        };
        SHARED.signal(temperature);
        Timer::after(SAMPLE_RATE).await;
    }
}

async fn setup_temp_reader(twi: &mut Twim<'_, impl twim::Instance>) -> Result<(), twim::Error> {
    defmt::debug!("Set resolution");
    twi.write(MCP9808_ADDRESS, &[RESOLUTION_REG, 0x00]).await?;
    Ok(())
}

async fn read_temperature(twi: &mut Twim<'_, impl twim::Instance>) -> Result<i16, twim::Error> {
    // Temp Sensor
    // Wake up
    twi.write(MCP9808_ADDRESS, &[CONFIG_REG, 0x00, 0x00])
        .await?;
    Timer::after(WAKEUP_DELAY).await;
    // Read
    let mut buf = [0u8; 2];
    twi.write_read(MCP9808_ADDRESS, &[TEMPERATURE_REG], &mut buf)
        .await?;
    // Conversion code based on the datasheet
    // https://ww1.microchip.com/downloads/en/DeviceDoc/25095A.pdf pg25
    let [upper, lower] = buf;
    let temp_raw = raw_to_signed(upper, lower);
    let temperature = (temp_raw as f32 / 16.0 * 100.0) as i16;
    defmt::debug!(
        "Temperature: {} (upper: {:#02x}, lower: {:#02x})",
        temperature,
        upper,
        lower
    );
    // Shutdown
    twi.write(MCP9808_ADDRESS, &[CONFIG_REG, 0x01, 0x00])
        .await?;
    Ok(temperature)
}

/// Converts upper and lower u8 bytes to a signed 12 bit i16
fn raw_to_signed(upper: u8, lower: u8) -> i16 {
    // clear any bits after bit 11 and shift;
    // combine with the lower 8 bits
    let val = ((upper & 0x1f) as i16) << 8 | lower as i16;
    // shift to apply the signal bit to the i16
    (val << 4) >> 4
}
