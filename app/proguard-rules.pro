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
