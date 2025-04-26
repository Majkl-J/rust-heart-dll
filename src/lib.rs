use std::{ffi::CString, os::raw::c_char, sync::{atomic::AtomicBool, Arc, Mutex}, thread, time::Duration, vec};

use noisegen::Noise;

pub mod freeing;
pub mod wrappers;
pub mod noisegen;

/// Returns a CString pointer, and keeps the string in memory for it to be cleared up later
/// 
/// Generally only for debugging

#[unsafe(no_mangle)]
pub extern "C" fn test_lib() -> *const c_char {
    let test_string = CString::new("Hello! This is rust-heart.").expect("CString::new failed");
    let test_pointer = test_string.as_ptr();

    // Leak the address, the cleanup is on the C# user (So never)
    // Clean via `free_rust_string`
    std::mem::forget(test_string);

    test_pointer
}

/// Returns a vector with actual thread wait time (in ms), and actual freq rate. Input desired freq rate
fn actual_thread_wait_time(freq: u64) -> Vec<u64> {
    // We need this because the Windows wait, unlike the Unix one, is shit, and I'm targeting windows
    // This means we are limited to 1ms waits. Sad but more than enough for an ECG signal
    let mut actual_baud = freq;
    if actual_baud > 1000 {
        actual_baud = 1000;
    }
    let mut return_vec: Vec<u64>= Vec::new();
    return_vec.push(1000/actual_baud);
    return_vec.push(actual_baud);
    return_vec
}



// Partially based on:
// M. J. Burke, & M. Nasor (2001) An accurate programmable ECG simulator, Journal of Medical Engineering & Technology, 25:3, 97-102, DOI: 10.1080/03091900110051640

/**
 * A list of amplitude multipliers for respective waves
 */
pub struct WaveAmps;

impl WaveAmps {
    pub const PWAVE:f64 = 0.15;
    pub const QWAVE:f64 = 0.0156;
    pub const RWAVE:f64 = 1.0;
    pub const SWAVE:f64 = 0.1563;
    pub const TWAVE:f64 = 0.2188;
}

/** 
 * A 0D model of a heart that can only simulate the I lead
 * Provides a fairly readable signal with no need of filtration/amplification
 */
#[repr(C)]
pub struct SimpleHeart {
    /// are we currently beating?
    active: AtomicBool,
    /// The amplitude of the R wave
    amplitude: f64,
    /// The interval between two R waves in seconds
    total_r_to_r: f64,

    // Holder variables for wave lengths
    p_duration: f64,
    p_to_q_interval: f64,
    q_duration: f64,
    r_duration: f64,
    s_duration: f64,
    s_to_t_interval: f64,
    t_duration: f64,

    /// Attached noise generators. 
    /// These all get passed through and called each tick and return
    /// from their implemented Noise trait
    attached_noises: Arc<Mutex<Vec<Box<dyn Noise + Send + Sync>>>>,

    /// Current output
    pub output_value: Arc<Mutex<Vec<f64>>>,
}

impl SimpleHeart {
    pub fn new(bpm: u64, amplitude: f64) -> SimpleHeart {
        let total_r_to_r = 60.0 / (bpm as f64);
        let qrs_duration = 0.25 * total_r_to_r.sqrt() - 0.16 * total_r_to_r - 0.02;

        SimpleHeart { 
            active: false.into(),
            amplitude: amplitude, 
            total_r_to_r: total_r_to_r,
            // Experimentally obtained, see M. Nasor's work
            p_duration: 0.37 * total_r_to_r.sqrt() - 0.22 * total_r_to_r - 0.06, 
            p_to_q_interval: 0.33 * total_r_to_r.sqrt() - 0.18 * total_r_to_r - 0.08, 
            q_duration: qrs_duration * 0.23,
            r_duration: qrs_duration * 0.42, 
            s_duration: qrs_duration * 0.35,
            s_to_t_interval: -0.09 * total_r_to_r.sqrt() + 0.13 * total_r_to_r + 0.04, 
            t_duration: 1.06 * total_r_to_r.sqrt() - 0.51 * total_r_to_r - 0.33,

            attached_noises: Arc::new(Mutex::new(Vec::new())),
            output_value: Arc::new(Mutex::new(vec![])),
        }
    }

    fn start_beat(this: Arc<Mutex<Self>>, freq: u64) {
        if this.lock().unwrap().active.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        let output_value = Arc::clone(&this.lock().unwrap().output_value);
        let amplitude = this.lock().unwrap().amplitude;
        thread::spawn(move || {
            // Initialize with the correct wait time
            let timings = actual_thread_wait_time(freq);

            let mut current_tick: u64 = 0;
            let mut this_beat_start_tick: u64 = 0;
            let mut output: f64;
            loop {
                let mut heart = this.lock().unwrap();
                let tick_r_to_r = (heart.total_r_to_r * timings[1] as f64) as u64; // We recalculate this since total_r_to_r may change

                if current_tick - this_beat_start_tick >= tick_r_to_r {
                    this_beat_start_tick = current_tick;
                }

                let mut wave_delay = this_beat_start_tick; // We cache this so I don't have to copypasta shit as much
                output = 0.0;
                output += second_order(current_tick, wave_delay, (heart.p_duration * timings[1] as f64) as u64, WaveAmps::PWAVE);
                wave_delay += ((heart.p_duration + heart.p_to_q_interval) * timings[1] as f64) as u64;
                output += triangle(current_tick, wave_delay, (heart.q_duration * timings[1] as f64) as u64, -WaveAmps::QWAVE);
                wave_delay += (heart.q_duration * timings[1] as f64) as u64;
                output += triangle(current_tick, wave_delay, (heart.r_duration * timings[1] as f64) as u64, WaveAmps::RWAVE);
                wave_delay += (heart.r_duration * timings[1] as f64) as u64;
                output += triangle(current_tick, wave_delay, (heart.s_duration * timings[1] as f64) as u64, -WaveAmps::SWAVE);
                wave_delay += ((heart.s_duration + heart.s_to_t_interval) * timings[1] as f64) as u64;
                output += triangle(current_tick, wave_delay, (heart.t_duration * timings[1] as f64) as u64, WaveAmps::TWAVE);
                let output_scaled = output * amplitude;
                if cfg!(debug_assertions) {
                    println!("Current value: {output}");
                }
                let output_noise = heart.calculate_noise(current_tick, freq);
                {
                let mut out_val = output_value.lock().unwrap();
                if out_val.len() < 5000 {
                    out_val.push(output_scaled + output_noise);
                }
                }
                current_tick += 1;
                spin_sleep::sleep(Duration::from_millis(timings[0]));
            }
        });
    }

    fn return_values(&mut self) -> Vec<f64> {
        let readvalue: Vec<f64> = match self.output_value.lock() {
            Ok(mut val) => {
                let slice = val.clone();
                *val = vec![];
                slice.to_vec()
            },
            Err(_) => vec![],
        };
        readvalue
    }

    fn calculate_noise(&mut self, current_tick: u64, tick_freq: u64) -> f64{
        let mut final_noise: f64 = 0.0;
        let locked_noises = self.attached_noises.lock().unwrap();
        for noise_gen in locked_noises.iter() {
            final_noise += noise_gen.get_tick_noise(current_tick, tick_freq)
        };
        final_noise
    }

    fn attach_noise(&mut self, noise: Box<dyn Noise + Send + Sync>) {
        let mut locked_noises = self.attached_noises.lock().unwrap();
        locked_noises.push(noise);
    }

    fn reset_noise(&mut self) {
        let mut locked_noises = self.attached_noises.lock().unwrap();
        *locked_noises = Vec::new();
    }
}

/**
 * Generates a one tick long part of a triangle wave.
 * `tick_now` - The current tick `u64`
 * `tick_begin` - The tick the wave started on `u64`
 * `duration` - The length of the entire wave, in ticks `u64`
 * `ampl` - Amplitude of the signal `f64`
 */
fn triangle(tick_now: u64, tick_begin: u64, duration: u64, ampl: f64) -> f64 {
    let peak = duration / 2; // Time in ticks to reach top of the wave
    let peak_tick = tick_begin + peak;

    let gain = ampl / peak as f64; // Gain per tick
    if tick_now < tick_begin || tick_now > tick_begin + duration {
        return 0.0;
    }
    // Guess who neglected that they're going to need signs but realized too late
    // What this hellish thing does is get the distance in ticks toward the peak
    // Then it multiplies it by gain
    let output = -(tick_now as i128 - peak_tick as i128).abs() + peak as i128;
    return output as f64 * gain;
}


/**
 * Generates a one tick part of a second order wave.
 * `tick_now` - The current tick `u64`
 * `tick_begin` - The tick the wave started on `u64`
 * `duration` - The length of the entire wave, in ticks `u64`
 * `ampl` - Amplitude of the signal `f64`
 */
fn second_order(tick_now: u64, tick_begin: u64, duration: u64, ampl: f64) -> f64 {
    let a = duration * duration / 4;
    let center_tick = tick_begin + duration / 2;

    let gain = ampl / a as f64;
    if tick_now < tick_begin || tick_now > tick_begin + duration {
        return 0.0;
    }

    let output = -((tick_now as i128 - center_tick as i128) * (tick_now as i128 - center_tick as i128)) + a as i128;
    return output as f64 * gain;
}