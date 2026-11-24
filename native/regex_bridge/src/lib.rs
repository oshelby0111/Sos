use regex::{Regex, RegexBuilder};
use std::ffi::{c_char, c_void, CStr, CString};
use std::ptr;
use std::slice;
use std::str;

struct CompiledRegex {
    regex: Regex,
    names: Vec<Option<CString>>,
}

fn write_error(dst: *mut c_char, capacity: usize, message: &str) {
    if dst.is_null() || capacity == 0 { return; }
    let bytes = message.as_bytes();
    let count = bytes.len().min(capacity - 1);
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), dst.cast::<u8>(), count);
        *dst.add(count) = 0;
    }
}

#[no_mangle]
pub unsafe extern "C" fn lovely_regex_compile(
    pattern: *const c_char,
    verbose: u8,
    error_buffer: *mut c_char,
    error_capacity: usize,
) -> *mut c_void {
    if pattern.is_null() {
        write_error(error_buffer, error_capacity, "null regex pattern");
        return ptr::null_mut();
    }
    let pattern = match CStr::from_ptr(pattern).to_str() {
        Ok(value) => value,
        Err(error) => {
            write_error(error_buffer, error_capacity, &format!("pattern is not UTF-8: {error}"));
            return ptr::null_mut();
        }
    };
    let regex = match RegexBuilder::new(pattern)
        .multi_line(true)
        .crlf(true)
        .ignore_whitespace(verbose != 0)
        .build()
    {
        Ok(value) => value,
        Err(error) => {
            write_error(error_buffer, error_capacity, &error.to_string());
            return ptr::null_mut();
        }
    };
    let names = regex.capture_names().map(|name| {
        name.and_then(|value| CString::new(value).ok())
    }).collect();
    Box::into_raw(Box::new(CompiledRegex { regex, names })).cast::<c_void>()
}

#[no_mangle]
pub unsafe extern "C" fn lovely_regex_capture_count(handle: *const c_void) -> usize {
    if handle.is_null() { return 0; }
    let compiled = &*handle.cast::<CompiledRegex>();
    compiled.names.len()
}

#[no_mangle]
pub unsafe extern "C" fn lovely_regex_capture_name(
    handle: *const c_void,
    index: usize,
) -> *const c_char {
    if handle.is_null() { return ptr::null(); }
    let compiled = &*handle.cast::<CompiledRegex>();
    match compiled.names.get(index).and_then(|name| name.as_ref()) {
        Some(name) => name.as_ptr(),
        None => ptr::null(),
    }
}

// Returns -1 for no match, -2 for invalid UTF-8, -3 for insufficient output
// space, or the number of capture groups (including group 0) on success.
// Each output pair is [start_byte, end_byte_exclusive]; usize::MAX means unset.
#[no_mangle]
pub unsafe extern "C" fn lovely_regex_find(
    handle: *const c_void,
    subject_ptr: *const u8,
    subject_len: usize,
    start_offset: usize,
    output_ranges: *mut usize,
    output_pairs: usize,
) -> i32 {
    if handle.is_null() || (subject_ptr.is_null() && subject_len > 0) || output_ranges.is_null() {
        return -3;
    }
    let compiled = &*handle.cast::<CompiledRegex>();
    let bytes: &[u8] = if subject_len == 0 { &[] } else { slice::from_raw_parts(subject_ptr, subject_len) };
    let subject = match str::from_utf8(bytes) {
        Ok(value) => value,
        Err(_) => return -2,
    };
    if output_pairs < compiled.names.len() { return -3; }
    let mut start = start_offset.min(subject.len());
    while start < subject.len() && !subject.is_char_boundary(start) { start += 1; }
    let captures = match compiled.regex.captures_at(subject, start) {
        Some(value) => value,
        None => return -1,
    };
    let output = slice::from_raw_parts_mut(output_ranges, output_pairs * 2);
    for pair in output.iter_mut() { *pair = usize::MAX; }
    for index in 0..compiled.names.len() {
        if let Some(found) = captures.get(index) {
            output[index * 2] = found.start();
            output[index * 2 + 1] = found.end();
        }
    }
    compiled.names.len() as i32
}

#[no_mangle]
pub unsafe extern "C" fn lovely_regex_free(handle: *mut c_void) {
    if !handle.is_null() {
        drop(Box::from_raw(handle.cast::<CompiledRegex>()));
    }
          }
