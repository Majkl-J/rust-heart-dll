use std::{ffi::CString, os::raw::c_char};
use std::sync::{Arc, Mutex};

use crate::SimpleHeart;

#[unsafe(no_mangle)]
/// Cleans up any string the user may have received
pub extern "C" fn free_rust_string(s: *mut c_char) {
    if s.is_null(){
        return;
    }
    unsafe {
        drop(CString::from_raw(s)); // Retake ownership and drop the string
    }
}

#[unsafe(no_mangle)]
pub extern  "C" fn free_simple_heart(ptr: *mut Arc<Mutex<SimpleHeart>>) {
    if !ptr.is_null(){
        unsafe{drop(Box::from_raw(ptr))};
    }
}