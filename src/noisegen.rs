use std::f64::consts::PI;
use rand::random_range;

pub trait Noise {
    fn get_tick_noise(&self, _current_tick: u64, _tick_freq: u64) -> f64;    
}

pub struct MainsNoise {
    amplitude: f64,
    frequency: u64,
    tick_shift: u64,
}

impl MainsNoise {
    pub fn new(amplitude: f64, frequency: u64) -> MainsNoise {
        MainsNoise { amplitude: amplitude, frequency: frequency, tick_shift: 0 }
    }

    pub fn set_tick_shift(&mut self, new_shift: u64) {
        self.tick_shift = new_shift;
    } 
}

impl Noise for MainsNoise {
    fn get_tick_noise(&self, _current_tick: u64, _tick_freq: u64) -> f64 {
        let wave_len = self.frequency / _tick_freq;
        // The current tick of the wave 
        let current_wave_point_tick = (_current_tick % wave_len) + self.tick_shift;
        let current_wave_point_radians: f64 = (current_wave_point_tick as f64 / wave_len as f64) * PI;

        current_wave_point_radians.sin() * self.amplitude
    }
}

pub struct RandomNoise {
    amplitude: f64,
}

impl Noise for RandomNoise {
    fn get_tick_noise(&self, _current_tick: u64, _tick_freq: u64) -> f64 {
        random_range(0.0..self.amplitude)
    }
}