package dev.davidv.translator

/**
 * Thin JNI front for the on-device ONNX→MNN converter, which lives in its own
 * native lib (`libmodel_converter.so`) because the MNN converter's protobuf
 * cannot share a binary with slimt's sentencepiece protobuf-lite.
 */
object ModelConverterJni {
  init {
    System.loadLibrary("model_converter")
  }

  /**
   * Convert [onnxPath] to [mnnPath]. Returns `null` on success or an error
   * message on failure. `quantBits` of 8 = int8 weight quant; 0 = none.
   */
  external fun convert(
    onnxPath: String,
    mnnPath: String,
    quantBits: Int,
  ): String?
}
