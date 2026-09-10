# Preserve file names and line numbers for readable stack traces
-keepattributes SourceFile,LineNumberTable

# Replace source file paths with just "SourceFile" to avoid leaking build paths
-renamesourcefileattribute SourceFile

# Disable obfuscation entirely
-dontobfuscate

-keepclasseswithmembernames class * {
    native <methods>;
}

# JNA: native code looks up methods/fields reflectively via initIDs
-dontwarn java.awt.*
-dontwarn com.sun.jna.**
-keep class com.sun.jna.** { *; }
-keepclassmembers class * extends com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }

# uniffi-generated bindings: Structure subclasses, Callback interfaces, and
# enum/data classes crossing the FFI boundary must retain their fields.
-keep class uniffi.bindings.** { *; }
-keep class uniffi.translator.** { *; }

# uCrop contains an optional HTTP(S) download path that depends on OkHttp, but
# this app only sends local content/file URIs to the cropper.
-dontwarn okhttp3.Call
-dontwarn okhttp3.Dispatcher
-dontwarn okhttp3.OkHttpClient
-dontwarn okhttp3.Request$Builder
-dontwarn okhttp3.Request
-dontwarn okhttp3.Response
-dontwarn okhttp3.ResponseBody
-dontwarn okio.BufferedSource
-dontwarn okio.Okio
-dontwarn okio.Sink
