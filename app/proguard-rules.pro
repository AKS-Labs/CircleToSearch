# Add project specific ProGuard rules here.
# You can control the set of applied configuration files using the
# proguardFiles setting in build.gradle.

# Keep Compose-specific classes
-keep class androidx.compose.** { *; }
-keep interface androidx.compose.** { *; }
-dontwarn androidx.compose.**

# Keep WebView JavaScript interfaces
-keepclassmembers class * {
    @android.webkit.JavascriptInterface <methods>;
}

# Keep WebView-related classes
-keep class android.webkit.** { *; }
-keepclassmembers class android.webkit.** { *; }
-dontwarn android.webkit.**

# Keep all classes that extend WebView
-keep public class * extends android.webkit.WebView

# For FileProvider
-keep class androidx.core.content.FileProvider { *; }

# Keep SearchEngine enum
-keep enum com.akslabs.circletosearch.data.SearchEngine { *; }

# Keep Preferences classes
-keep class com.akslabs.circletosearch.utils.UIPreferences { *; }
-keep class com.akslabs.circletosearch.utils.PrivacyPreferences { *; }

# Keep data classes
-keep class com.akslabs.circletosearch.data.** { *; }

# Preserve line number information for debugging
-keepattributes SourceFile,LineNumberTable

# Hide source file name
-renamesourcefileattribute SourceFile

# Optimization flags
-optimizationpasses 5
-dontusemixedcaseclassnames
-dontskipnonpubliclibraryclasses
-verbose