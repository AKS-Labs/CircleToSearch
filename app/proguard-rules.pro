# Add project specific ProGuard rules here.
# You can control the set of applied configuration files using the
# proguardFiles setting in build.gradle.

# R8 handles Compose optimization automatically. 
# Broadly keeping all compose classes prevents maximum shrinking.
# We only keep what's strictly necessary if we encounter issues.

# Keep WebView JavaScript interfaces - essential for functionality
-keepclassmembers class * {
    @android.webkit.JavascriptInterface <methods>;
}

# Keep the data classes for GSON but allow obfuscation of names
# Since we added @SerializedName, we don't need to keep the field names.
-keepclassmembers class com.akslabs.circletosearch.data.** {
    <fields>;
}

# Keep Enums for GSON serialization
-keepclassmembers enum com.akslabs.circletosearch.data.** {
    <fields>;
    public static **[] values();
    public static ** valueOf(java.lang.String);
}

# Preserve line number information for debugging (optional, remove for absolute min size)
-keepattributes SourceFile,LineNumberTable
-renamesourcefileattribute SourceFile

# Optimization flags
-optimizationpasses 5
-dontusemixedcaseclassnames
-dontskipnonpubliclibraryclasses

# Remove log messages in release builds (improves performance and size)
-assumenosideeffects class android.util.Log {
    public static *** d(...);
    public static *** v(...);
    public static *** i(...);
}

# uCrop contains optional HTTP download features depending on OkHttp.
# Ignore missing okhttp3 classes since OkHttp is not included as a dependency.
-dontwarn okhttp3.**

# --- JNA + UniFFI (translator module) --------------------------------------
# JNA's native initIDs() looks up fields such as Pointer.peer BY NAME via
# JNI. If R8 renames or strips them, release builds crash at startup with
# "UnsatisfiedLinkError: Can't obtain peer field ID for class
# com.sun.jna.Pointer" inside TranslatorApplication.onCreate.
-keep class com.sun.jna.** { *; }
-keep interface com.sun.jna.** { *; }
-keepclassmembers class com.sun.jna.** { *; }
-keepclasseswithmembernames,includedescriptorclasses class * {
    native <methods>;
}

# UniFFI-generated bindings and the translator module's JNI entry points are
# invoked from native code and must keep their names.
-keep class uniffi.** { *; }
-keep class dev.davidv.translator.** { *; }
-dontwarn java.awt.**
-dontwarn com.sun.jna.**

