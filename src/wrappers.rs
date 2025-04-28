use crate::{noisegen::{MainsNoise, Noise, RandomNoise}, SimpleHeart};

use std::sync::{Arc, Mutex};

#[unsafe(no_mangle)]
pub extern "C" fn build_simple_heart(bpm: u64, amplitude: f64) -> *mut Arc<Mutex<SimpleHeart>> {
    let heart = Arc::new(Mutex::new(SimpleHeart::new(bpm, amplitude)));

    Box::into_raw(Box::new(heart))
}

#[unsafe(no_mangle)]
pub extern "C" fn simple_heart_start(ptr: *mut Arc<Mutex<SimpleHeart>>, freq: u64) {
    if ptr.is_null() {return}
    let heart: &mut Arc<Mutex<SimpleHeart>> = unsafe{&mut *ptr};
    SimpleHeart::start_beat(Arc::clone(&heart), freq)
}

/// C compatible struct for holding an array of doubles
#[repr(C)]
pub struct F64Array {
    /// Pointer to the held data
    pub data: *const f64,
    /// Length of the data
    pub len: usize,
}

/// Turns a vector into a C compatible holder struct
/// 
/// # Unsafe
/// Make sure to correctly cleanup when calling this as it forgets the entire vector,
/// leaving it hanging around in memory.
unsafe fn f64_vector_as_c_ptr(vector: Vec<f64>) -> *mut F64Array {
    //let mut buffer = vector.into_boxed_slice(); 
    let ptr = vector.as_ptr();
    let len = vector.len();

    std::mem::forget(vector);

    let holder = Box::new(F64Array{data: ptr, len: len});
    Box::into_raw(holder)
}

#[unsafe(no_mangle)]
pub extern  "C" fn simple_heart_read(ptr: *mut Arc<Mutex<SimpleHeart>>) -> *mut F64Array {
    if ptr.is_null() {
        return unsafe{f64_vector_as_c_ptr(vec![])};
    }
    let heart_ref: &mut Arc<Mutex<SimpleHeart>>= unsafe{&mut *ptr};
    match heart_ref.lock() {
        Ok(mut heart_guard) => {
            let data_vec: Vec<f64> = heart_guard.return_values();
            return unsafe{f64_vector_as_c_ptr(data_vec)};
        },
        Err(_) => return unsafe{f64_vector_as_c_ptr(vec![])}
    }
}

#[repr(u32)] // Fix the size to u32 just like C
pub enum NoiseTypes {
    MainsNoise,
    RandomNoise,
}

#[unsafe(no_mangle)]
pub extern "C" fn simple_heart_add_noise(ptr: *mut Arc<Mutex<SimpleHeart>>, noise_type: NoiseTypes, amplitude: f64, freq: u64) {
    if ptr.is_null() {return}
    
    let inited_noise: Box<dyn Noise + Send + Sync>;
    match noise_type {
        NoiseTypes::MainsNoise => inited_noise = Box::new(MainsNoise::new(amplitude, freq)),
        NoiseTypes::RandomNoise => inited_noise = Box::new(RandomNoise::new(amplitude)),
    }

    let heart_ref: &mut Arc<Mutex<SimpleHeart>>= unsafe{&mut *ptr};
    match heart_ref.lock() {
        Ok(mut heart_guard) => {
            heart_guard.attach_noise(inited_noise);
        },
        Err(_) => return
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn simple_heart_reset_noise(ptr: *mut Arc<Mutex<SimpleHeart>>){
    if ptr.is_null() {return}

    let heart_ref: &mut Arc<Mutex<SimpleHeart>>= unsafe{&mut *ptr};
    match heart_ref.lock() {
        Ok(mut heart_guard) => {
            heart_guard.reset_noise();
        },
        Err(_) => return
    }

}