use crate::SimpleHeart;

use std::sync::{Arc, Mutex};

#[unsafe(no_mangle)]
pub extern "C" fn build_simple_heart(bpm: u64, amplitude: f64) -> *mut Arc<Mutex<SimpleHeart>> {
    let heart = Arc::new(Mutex::new(SimpleHeart::new(bpm, amplitude)));

    Box::into_raw(Box::new(heart))
}

#[unsafe(no_mangle)]
pub extern "C" fn simple_heart_start(ptr: *mut Arc<Mutex<SimpleHeart>>, baud: u64) {
    if ptr.is_null() {
        return;
    }
    let heart: &mut Arc<Mutex<SimpleHeart>> = unsafe{&mut *ptr};
    SimpleHeart::start_beat(Arc::clone(&heart), baud)
}

#[unsafe(no_mangle)]
pub extern  "C" fn simple_heart_read(ptr: *mut Arc<Mutex<SimpleHeart>>) -> f64 {
    if ptr.is_null() {
        return -50000.0;
    }
    let heart_ref: &mut Arc<Mutex<SimpleHeart>>= unsafe{&mut *ptr};
    match heart_ref.lock() {
        Ok(mut heart_guard) => heart_guard.return_value(),
        Err(_) => return -50000.0
    }
}