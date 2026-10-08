# Wobble Pull-Down & Device Info Animation Package

This package contains all necessary files and composables to implement the full-screen wobble liquid ripple pull-down gesture on a home screen and transition seamlessly into an App Info screen with elastic overscroll scaling.

## 📦 Package Contents

1. **`LiquidRippleModifier.kt`**: Custom Compose modifier applying an AGSL liquid wave ripple shader (`LIQUID_RIPPLE_AGSL`) on Android 13+ (API 33+).
2. **`ProgressiveBlurModifier.kt`**: Custom Compose modifier applying a progressive edge blur shader (`PROGRESSIVE_BLUR_SKSL`) to bars and overlays.
3. **`WobbleHaptics.kt`**: Centralized haptic feedback helper for pull-down steps and overscroll ticks.
4. **`WobbleDeviceInfo.kt`**: `WobbleDeviceInfo` data model & `WobbleDeviceUtils` helper to fetch hardware (RAM, Storage) and system info (Android Version, SDK, Security Patch, Model, etc.).
5. **`WobbleDeviceImageMapper.kt`**: Utility mapping Android OS versions/codenames to Android logo drawables.
6. **`WobbleHomeScreen.kt`**: Home screen UI containing the expandable device hero card, search bar placeholder, settings list, and `PullToRefreshBox` with `liquidRipple` wobble animation.
7. **`WobbleAppInfoScreen.kt`**: Circle to Search app info screen — app icon/hero card, permission status rows (grant/revoke state), active launch-trigger statuses, package/build details, with elastic overscroll scaling and progressive blur.
8. **`WobbleDeviceInfoActivity.kt`**: Activity hosting `WobbleAppInfoScreen` (and `WobbleSetupWizard` in setup mode) with slide-in/slide-out activity transition animations.

---

## 🚀 How to Extract & Use in Another App

### Step 1: Copy Files
Copy the `wobblepulldown` package folder into your target app module (`com.yourpackage.wobblepulldown`). Update package declarations at the top of each file.

### Step 2: Add Required Dependencies
Ensure your target app's `build.gradle.kts` includes:
```kotlin
dependencies {
    implementation(platform(libs.androidx.compose.bom))
    implementation(libs.androidx.compose.ui)
    implementation(libs.androidx.compose.material3)
    implementation(libs.androidx.compose.material.icons.extended)
}
```

### Step 3: Add Transition Animations
Copy the following 3 XML animation files into `res/anim/`:
- `anim_slide_in_top.xml`:
  ```xml
  <?xml version="1.0" encoding="utf-8"?>
  <set xmlns:android="http://schemas.android.com/apk/res/android"
      android:interpolator="@android:anim/decelerate_interpolator">
      <translate android:duration="400" android:fromYDelta="-100%" android:toYDelta="0" />
  </set>
  ```
- `anim_slide_out_top.xml`:
  ```xml
  <?xml version="1.0" encoding="utf-8"?>
  <set xmlns:android="http://schemas.android.com/apk/res/android"
      android:interpolator="@android:anim/accelerate_interpolator">
      <translate android:duration="400" android:fromYDelta="0" android:toYDelta="-100%" />
  </set>
  ```
- `anim_stay.xml`:
  ```xml
  <?xml version="1.0" encoding="utf-8"?>
  <set xmlns:android="http://schemas.android.com/apk/res/android">
      <translate android:duration="400" android:fromYDelta="0" android:toYDelta="0" />
  </set>
  ```

### Step 4: Register Activity in `AndroidManifest.xml`
```xml
<activity
    android:name=".wobblepulldown.WobbleDeviceInfoActivity"
    android:exported="false"
    android:label="Device Info"
    android:theme="@style/Theme.YourApp" />
```

### Step 5: Embed `WobbleHomeScreen` in your App
```kotlin
@Composable
fun MainScreen() {
    val context = LocalContext.current

    WobbleHomeScreen(
        onOpenDeviceInfo = {
            val intent = Intent(context, WobbleDeviceInfoActivity::class.java)
            val options = androidx.core.app.ActivityOptionsCompat.makeCustomAnimation(
                context,
                R.anim.anim_slide_in_top,
                R.anim.anim_stay,
            )
            context.startActivity(intent, options.toBundle())
        }
    )
}
```
