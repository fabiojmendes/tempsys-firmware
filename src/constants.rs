use embassy_time::Duration;

// Debug timmings
#[cfg(debug_assertions)]
pub static SAMPLE_RATE: Duration = Duration::from_secs(1);
#[cfg(debug_assertions)]
pub static ADV_INTERVAL: u32 = 400; // 400 * 0.625 = 250ms

// Production timmings for power saving
#[cfg(not(debug_assertions))]
pub static SAMPLE_RATE: Duration = Duration::from_secs(30);
#[cfg(not(debug_assertions))]
pub static ADV_INTERVAL: u32 = 8000; // 8000 * 0.625 = 5000ms
