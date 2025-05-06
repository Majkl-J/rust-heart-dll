use std::{ffi::CString, os::raw::c_char, sync::{atomic::AtomicBool, Arc, Mutex}, thread, time::Duration, vec};

use noisegen::Noise;

pub mod freeing;
pub mod wrappers;
pub mod noisegen;

/// Returns a CString pointer, and keeps the string in memory for it to be cleared up later
/// 
/// Mostly a test to ensure the dll is hooked and functioning correctly.

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
/// 
/// TODO: Implement OS checks to make this multiplatform compatible
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

/// Struct holder for experimentally obtained amplitude multipliers for respective
/// ECG waves.
pub struct WaveAmps;

impl WaveAmps {
    pub const PWAVE:f64 = 0.15;
    pub const QWAVE:f64 = 0.0156;
    pub const RWAVE:f64 = 1.0;
    pub const SWAVE:f64 = 0.1563;
    pub const TWAVE:f64 = 0.2188;
}

/// A simple 0D generator of an ECG signal.
/// 
/// Generates the **I lead signal** with no disorders according to 
/// preset experimentally obtained constants. By default this signal is
/// clean.
/// 
/// - Can be created with custom BPM which gets recalculated
/// into the beat period `total_r_to_r` and R-wave amplitude `amplitude`
/// - Can have a `Noise` trait struct attached
#[repr(C)]
pub struct SimpleHeart {
    /// Is the heart currently beating?
    active: AtomicBool,
    /// The maximum amplitude of the R wave
    amplitude: f64,
    /// The interval between two R waves in seconds
    total_r_to_r: f64,

    // Holder variables for wave lengths
    // maybe could be made into its own struct?
    p_duration: f64,
    p_to_q_interval: f64,
    q_duration: f64,
    r_duration: f64,
    s_duration: f64,
    s_to_t_interval: f64,
    t_duration: f64,

    /// Attached noise generators. 
    /// 
    /// All the held noises get passed through and called each 
    /// tick and return from their implemented Noise trait
    attached_noises: Arc<Mutex<Vec<Box<dyn Noise + Send + Sync>>>>,

    /// Current amplitude values we are storing, cleared on read
    output_value: Arc<Mutex<Vec<f64>>>,
}

impl SimpleHeart {
    pub fn new(bpm: u64, amplitude: f64) -> SimpleHeart {
        // Convert bpm into the period of a single beat
        let total_r_to_r = 60.0 / (bpm as f64);
        // Calculate the length of the whole qrs complex from this length
        let qrs_duration = 0.25 * total_r_to_r.sqrt() - 0.16 * total_r_to_r - 0.02;

        SimpleHeart { 
            active: false.into(),
            amplitude: amplitude, 
            total_r_to_r: total_r_to_r,

            // Experimentally obtained ratios, see M. Nasor's work
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

    /// Starts the processing of the heart's ECG signal unless `active` is already true
    /// 
    /// Spawns an async thread loop that calculates the ECG value in real time and outputs it into 
    /// `output_value`
    pub fn start_beat(this: Arc<Mutex<Self>>, freq: u64) {
        // Sets the active bool to true and returns its previous value 
        if this.lock().unwrap().active.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        // Clone the atomic reference of the output value, increasing the refcount.
        // This is so the value can be moved into the spawned thread
        let output_value = Arc::clone(&this.lock().unwrap().output_value);
        // Does the same for the amplitude
        let amplitude = this.lock().unwrap().amplitude;
        thread::spawn(move || {
            // Initialize with the correct wait time
            let timings = actual_thread_wait_time(freq);

            // The current heart processing tick
            let mut current_tick: u64 = 0;
            // The tick the current heartbeat has started on
            let mut this_beat_start_tick: u64 = 0;
            // The output value, gets added to the heart's `output_value`
            // once fully calculated
            let mut output: f64;

            // Values copied from the heart each process tick
            let mut total_r_to_r: f64;
            let mut p_duration: f64;
            let mut p_to_q_interval: f64;
            let mut q_duration: f64;
            let mut r_duration: f64;
            let mut s_duration: f64;
            let mut s_to_t_interval: f64;
            let mut t_duration: f64;

            let mut output_noise: f64;

            loop { 
                {
                    // Copy all the values from the heart so we don't have to keep it locked
                    let mut heart = this.lock().unwrap();

                    total_r_to_r = heart.total_r_to_r;

                    p_duration = heart.p_duration;
                    p_to_q_interval = heart.p_to_q_interval;
                    q_duration = heart.q_duration;
                    r_duration = heart.r_duration;
                    s_duration = heart.s_duration;
                    s_to_t_interval = heart.s_to_t_interval;
                    t_duration = heart.t_duration;

                    output_noise = heart.calculate_noise(current_tick, freq);
                }
    
                let tick_r_to_r = (total_r_to_r * timings[1] as f64) as u64; // We recalculate this since total_r_to_r may change

                // Updates us to a new beat tick if we just finished the last one
                if current_tick - this_beat_start_tick >= tick_r_to_r {
                    this_beat_start_tick = current_tick;
                }

                // We cache this so code doesn't have to be as copypastey. Mostly for readability
                let mut wave_delay = this_beat_start_tick;

                // Calculate each one of the waves and how much they add to the total output individually
                // This is a flat value and not the final output
                output = 0.0;
                output += second_order(current_tick, wave_delay, (p_duration * timings[1] as f64) as u64, WaveAmps::PWAVE);
                wave_delay += ((p_duration + p_to_q_interval) * timings[1] as f64) as u64;
                output += triangle(current_tick, wave_delay, (q_duration * timings[1] as f64) as u64, -WaveAmps::QWAVE);
                wave_delay += (q_duration * timings[1] as f64) as u64;
                output += triangle(current_tick, wave_delay, (r_duration * timings[1] as f64) as u64, WaveAmps::RWAVE);
                wave_delay += (r_duration * timings[1] as f64) as u64;
                output += triangle(current_tick, wave_delay, (s_duration * timings[1] as f64) as u64, -WaveAmps::SWAVE);
                wave_delay += ((s_duration + s_to_t_interval) * timings[1] as f64) as u64;
                output += second_order(current_tick, wave_delay, (t_duration * timings[1] as f64) as u64, WaveAmps::TWAVE);
                
                // Multiply the output by amplitude
                let output_scaled = output * amplitude;
                if cfg!(debug_assertions) {
                    println!("Current value: {output}");
                }

                {
                    // Output the value into the output vector
                    // Limited to 5000 logs maximum for the sake of not flooding memory
                    // Might consider making this user-adjustable
                    let mut out_val = output_value.lock().unwrap();
                    if out_val.len() < 5000 {
                        out_val.push(output_scaled + output_noise);
                    }
                }
                // Increment our tick, sleep
                current_tick += 1;
                spin_sleep::sleep(Duration::from_millis(timings[0]));
            }
        });
    }

    /// Returns the stored readings from the `output_value` vector.
    /// Empties the vector after returning, allowing new values to
    /// be added to it
    pub fn return_values(&mut self) -> Vec<f64> {
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

    /// Calculates the current noise of the signal by passing through all the dyn refs in `attached_noises`.
    fn calculate_noise(&mut self, current_tick: u64, tick_freq: u64) -> f64{
        let mut final_noise: f64 = 0.0;
        let locked_noises = self.attached_noises.lock().unwrap();
        for noise_gen in locked_noises.iter() {
            final_noise += noise_gen.get_tick_noise(current_tick, tick_freq)
        };
        final_noise
    }

    /// Attaches a new `Noise` trait dyn to the heart by adding it to the `attached_noises`
    pub fn attach_noise(&mut self, noise: Box<dyn Noise + Send + Sync>) {
        let mut locked_noises = self.attached_noises.lock().unwrap();
        locked_noises.push(noise);
    }

    /// Removes all attached `Noise` trait dynamic refs from the heart.
    pub fn reset_noise(&mut self) {
        let mut locked_noises = self.attached_noises.lock().unwrap();
        *locked_noises = Vec::new();
    }
}

/// Generates a one tick long part of a triangle wave.
/// 
/// ## Arguments
/// * `tick_now` - The current tick `u64`
/// * `tick_begin` - The tick the wave started on `u64`
/// * `duration` - The length of the entire wave, in ticks `u64`
/// * `ampl` - Amplitude of the signal `f64`
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


/// Generates a one tick part of a second order wave.
/// 
/// ## Arguments
/// * `tick_now` - The current tick `u64`
/// * `tick_begin` - The tick the wave started on `u64`
/// * `duration` - The length of the entire wave, in ticks `u64`
/// * `ampl` - Amplitude of the signal `f64`
fn second_order(tick_now: u64, tick_begin: u64, duration: u64, ampl: f64) -> f64 {
    let a = duration * duration / 4; // Time in ticks to reach top of the wave
    let center_tick = tick_begin + duration / 2;

    let gain = ampl / a as f64; // Gain per tick
    if tick_now < tick_begin || tick_now > tick_begin + duration {
        return 0.0;
    }

    let output = -((tick_now as i128 - center_tick as i128) * (tick_now as i128 - center_tick as i128)) + a as i128;
    return output as f64 * gain;
}