I have extracted the exact home screen wobble pull-down gesture and Device Info screen animations into a self-contained package called com.sameerasw.essentials.wobblepulldown.
ðŸ“‚ Package Structure
All required components are organized in: ðŸ“ app/src/main/java/com/sameerasw/essentials/wobblepulldown/
File
Description
LiquidRippleModifier.kt
Custom Compose modifier applying the AGSL liquid wave ripple shader (LIQUID_RIPPLE_AGSL) on pull-down.
ProgressiveBlurModifier.kt
AGSL progressive edge blur shader (PROGRESSIVE_BLUR_SKSL) for top/bottom translucent overlays.
WobbleHaptics.kt
Centralized haptic feedback helper for pull-down steps and overscroll ticks.
WobbleDeviceInfo.kt
Hardware & system spec model and utilities (RAM, Storage, Android Version, API level, etc.).
WobbleDeviceImageMapper.kt
Maps Android OS versions to Android version logo drawables.
WobbleHomeScreen.kt
Home Screen UI Component: Features top expandable hero header card, search bar placeholder, settings categories, and PullToRefreshBox with liquidRipple wobble animation.
WobbleAppInfoScreen.kt
App Info Screen UI Component: Features app icon hero card with elastic overscroll scaling (overscrollOffset), permission status rows, active launch-trigger statuses, package/build list, and progressive edge blur.
WobbleDeviceInfoActivity.kt
Activity host with top-slide enter/exit activity transition animations.
README.md
Package documentation & setup summary.
ðŸ“– Step-by-Step Integration Guide for Other Apps
Follow these steps to extract and use this exact wobble pull-down animation and App Info screen in any other Android application:
Step 1: Copy the wobblepulldown Package
Copy the entire folder wobblepulldown into your target project's source directory (e.g., com.yourdomain.yourapp.wobblepulldown). Update the package statement at the top of each file to match your target package name.
Step 2: Ensure Material3 & Compose Dependencies
Make sure your target app's build.gradle.kts has Jetpack Compose and Material 3 enabled:
kotlin
dependencies {
implementation(platform(libs.androidx.compose.bom))
implementation(libs.androidx.compose.ui)
implementation(libs.androidx.compose.material3)
implementation(libs.androidx.compose.material.icons.extended)
}
Step 3: Add the Activity Slide Transition Animations
Copy the following three XML animation files into res/anim/ in your target app:
1.
Â res/anim/anim_slide_in_top.xml:
xml
<?xml version="1.0" encoding="utf-8"?>
<set xmlns:android="http://schemas.android.com/apk/res/android"
android:interpolator="@android:anim/decelerate_interpolator">
<translate
android:duration="400"
android:fromYDelta="-100%"
android:toYDelta="0" />
</set>
2.
Â res/anim/anim_slide_out_top.xml:
xml
<?xml version="1.0" encoding="utf-8"?>
<set xmlns:android="http://schemas.android.com/apk/res/android"
android:interpolator="@android:anim/accelerate_interpolator">
<translate
android:duration="400"
android:fromYDelta="0"
android:toYDelta="-100%" />
</set>
3.
Â res/anim/anim_stay.xml:
xml
<?xml version="1.0" encoding="utf-8"?>
<set xmlns:android="http://schemas.android.com/apk/res/android">
    <translate
        android:duration="400"
        android:fromYDelta="0"
        android:toYDelta="0" />
</set>
Step 4: Register WobbleDeviceInfoActivity in Â AndroidManifest.xml
Add the activity declaration to your Â AndroidManifest.xml:
xml
<activity
    android:name=".wobblepulldown.WobbleDeviceInfoActivity"
    android:exported="false"
    android:label="Device Info"
    android:theme="@style/Theme.YourApp" />
Step 5: Render WobbleHomeScreen in Your MainActivity or Composable Flow
kotlin
import com.yourdomain.yourapp.wobblepulldown.WobbleHomeScreen
import com.yourdomain.yourapp.wobblepulldown.WobbleDeviceInfoActivity

@Composable
fun AppHomeScreen() {
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
ðŸŽ¨ How the Animations Work
1.
Pull-Down Wobble Shader:
â—¦
PullToRefreshBox tracks distanceFraction.
â—¦
When the user pulls down completely (distanceFraction >= 1f), WobbleHaptics performs haptic feedback, and Modifier.liquidRipple() executes an AGSL runtime shader wave distortion centered at the device header text.
2.
Card & Font Morphing:
â—¦
Top hero card expands in height (64.dp + statusBarPadding + cardExpansion).
â—¦
Title text increases weight and width via variable font settings and scales up seamlessly.
3.
Transition to Device Info Screen:
â—¦
Upon gesture release when threshold is met, onOpenDeviceInfo() launches WobbleDeviceInfoActivity with custom top slide-in (anim_slide_in_top) activity options.
4.
Elastic Overscroll Scaling:
â—¦
WobbleAppInfoScreen uses a NestedScrollConnection that captures overscroll drag when scrolled to top, scaling up the app icon smoothly based on drag distance (overscrollOffset). On gesture release, it springs smoothly back to 0.