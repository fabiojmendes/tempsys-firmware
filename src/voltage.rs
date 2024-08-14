use embassy_nrf::saadc::Saadc;

static ADC_REF_VOLTAGE: i32 = 600;
static ADC_GAIN: i32 = 6;
static ADC_RESOLUTION: i32 = 12;

pub async fn read(saadc: &mut Saadc<'_, 1>) -> i16 {
    let mut buf = [0; 1];
    saadc.sample(&mut buf).await;
    let [raw] = buf;
    // convert raw input to millivolts
    let voltage = (raw as i32 * ADC_REF_VOLTAGE * ADC_GAIN) >> ADC_RESOLUTION;
    defmt::debug!("Voltage: raw = {}, converted = {}", raw, voltage);
    voltage as i16
}
