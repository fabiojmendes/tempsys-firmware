use embassy_nrf::twim::{self, Twim};
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer};

const MCP9808_ADDRESS: u8 = 0x18;
static SAMPLE_RATE: Duration = Duration::from_secs(30);
static I2C_TIMEOUT: Duration = Duration::from_millis(2000);

static SHARED: Signal<ThreadModeRawMutex, i16> = Signal::new();

pub async fn read() -> i16 {
    SHARED.wait().await
}

#[embassy_executor::task]
pub async fn init(mut twi: Twim<'static, impl twim::Instance>) -> ! {
    let mut set_resolution = true;
    loop {
        if set_resolution {
            if let Err(e) = setup_temp_reader(&mut twi) {
                defmt::error!("Error setting sensor resolution {}", e)
            } else {
                set_resolution = false;
                // Give some time for the sensor to begin normal operation
                Timer::after_millis(400).await;
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

fn setup_temp_reader(twi: &mut Twim<'_, impl twim::Instance>) -> Result<(), twim::Error> {
    defmt::info!("Set resolution");
    twi.blocking_write_timeout(MCP9808_ADDRESS, &[0x08, 0x00], I2C_TIMEOUT)?;
    Ok(())
}

async fn read_temperature(twi: &mut Twim<'_, impl twim::Instance>) -> Result<i16, twim::Error> {
    // Temp Sensor
    // Wake up
    twi.blocking_write_timeout(MCP9808_ADDRESS, &[0x01, 0x00, 0x00], I2C_TIMEOUT)?;
    Timer::after_millis(50).await;
    // Read
    let mut buf = [0u8; 2];
    twi.blocking_write_read_timeout(MCP9808_ADDRESS, &[0x05], &mut buf, I2C_TIMEOUT)?;
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
    // Shutdown
    twi.blocking_write_timeout(MCP9808_ADDRESS, &[0x01, 0x01, 0x00], I2C_TIMEOUT)?;
    Ok(temp)
}
