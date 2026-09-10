use std::path::Path;

use jni::JNIEnv;
use jni::objects::{JClass, JString};
use jni::sys::{jint, jstring};
use translator_convert::{WeightQuant, convert};

/// Convert one ONNX model to MNN. Returns `null` on success or a Java string
/// with the error message on failure. Lives in its own native lib (linking only
/// `translator-convert` → `mnn-sys`, never slimt) because the MNN converter's
/// full protobuf and sentencepiece's protobuf-lite cannot share a binary.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_davidv_translator_ModelConverterJni_convert<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    onnx: JString<'local>,
    mnn: JString<'local>,
    quant_bits: jint,
) -> jstring {
    match run(&mut env, onnx, mnn, quant_bits) {
        Ok(()) => std::ptr::null_mut(),
        Err(message) => env
            .new_string(message)
            .map(|s| s.into_raw())
            .unwrap_or(std::ptr::null_mut()),
    }
}

fn run(
    env: &mut JNIEnv,
    onnx: JString,
    mnn: JString,
    quant_bits: jint,
) -> Result<(), String> {
    let onnx: String = env.get_string(&onnx).map_err(|error| error.to_string())?.into();
    let mnn: String = env.get_string(&mnn).map_err(|error| error.to_string())?.into();
    let quant = if quant_bits <= 0 {
        WeightQuant::None
    } else {
        WeightQuant::Bits(quant_bits as u8)
    };
    convert(Path::new(&onnx), Path::new(&mnn), quant).map_err(|error| error.to_string())
}
