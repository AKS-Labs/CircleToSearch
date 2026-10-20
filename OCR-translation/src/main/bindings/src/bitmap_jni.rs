//! Zero-copy access to an Android `Bitmap`'s pixels for the still-image OCR path.
//! [`lockPixels`] pins an `ARGB_8888` bitmap and returns the native pixel address as a
//! `jlong`; Rust reads/writes it directly (no `byte[]` copy, no uniffi marshal) and the
//! caller releases it with [`unlockPixels`]. Returns 0 unless the bitmap is tightly-packed
//! `RGBA_8888` (`stride == width*4`), the only layout the OCR pipeline indexes.

use jni::objects::{JClass, JObject};
use jni::sys::jlong;
use jni::JNIEnv;
use std::os::raw::{c_int, c_void};

const ANDROID_BITMAP_FORMAT_RGBA_8888: i32 = 1;
const ANDROID_BITMAP_RESULT_SUCCESS: c_int = 0;

#[repr(C)]
struct AndroidBitmapInfo {
    width: u32,
    height: u32,
    stride: u32,
    format: i32,
    flags: u32,
}

#[link(name = "jnigraphics")]
unsafe extern "C" {
    fn AndroidBitmap_getInfo(
        env: *mut jni::sys::JNIEnv,
        bitmap: jni::sys::jobject,
        info: *mut AndroidBitmapInfo,
    ) -> c_int;
    fn AndroidBitmap_lockPixels(
        env: *mut jni::sys::JNIEnv,
        bitmap: jni::sys::jobject,
        addr: *mut *mut c_void,
    ) -> c_int;
    fn AndroidBitmap_unlockPixels(env: *mut jni::sys::JNIEnv, bitmap: jni::sys::jobject) -> c_int;
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_davidv_translator_NativeBitmap_lockPixels(
    env: JNIEnv,
    _class: JClass,
    bitmap: JObject,
) -> jlong {
    let raw_env = env.get_raw();
    let raw_bitmap = bitmap.as_raw();
    let mut info = AndroidBitmapInfo {
        width: 0,
        height: 0,
        stride: 0,
        format: 0,
        flags: 0,
    };
    unsafe {
        if AndroidBitmap_getInfo(raw_env, raw_bitmap, &mut info) != ANDROID_BITMAP_RESULT_SUCCESS {
            return 0;
        }
        if info.format != ANDROID_BITMAP_FORMAT_RGBA_8888 || info.stride != info.width * 4 {
            return 0;
        }
        let mut addr: *mut c_void = std::ptr::null_mut();
        if AndroidBitmap_lockPixels(raw_env, raw_bitmap, &mut addr) != ANDROID_BITMAP_RESULT_SUCCESS
        {
            return 0;
        }
        addr as jlong
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_davidv_translator_NativeBitmap_unlockPixels(
    env: JNIEnv,
    _class: JClass,
    bitmap: JObject,
) {
    unsafe {
        AndroidBitmap_unlockPixels(env.get_raw(), bitmap.as_raw());
    }
}
