
--------- beginning of main
03-23 01:08:50.094  1450  1450 W main    : type=1400 audit(0.0:316629): avc:  denied  { search } for  name="app" dev="mmcblk0p61" ino=1343489 scontext=u:r:zygote:s0 tcontext=u:object_r:apk_data_file:s0 tclass=dir permissive=0 app=com.akslabs.circletosearch
03-23 01:08:50.094  1450  1450 W main    : type=1400 audit(0.0:316630): avc:  denied  { search } for  name="app" dev="mmcblk0p61" ino=1343489 scontext=u:r:zygote:s0 tcontext=u:object_r:apk_data_file:s0 tclass=dir permissive=0 app=com.akslabs.circletosearch
03-23 01:08:50.094  1450  1450 W main    : type=1400 audit(0.0:316631): avc:  denied  { search } for  name="app" dev="mmcblk0p61" ino=1343489 scontext=u:r:zygote:s0 tcontext=u:object_r:apk_data_file:s0 tclass=dir permissive=0 app=com.akslabs.circletosearch
03-23 01:08:50.094  1450  1450 W main    : type=1400 audit(0.0:316632): avc:  denied  { search } for  name="app" dev="mmcblk0p61" ino=1343489 scontext=u:r:zygote:s0 tcontext=u:object_r:apk_data_file:s0 tclass=dir permissive=0 app=com.akslabs.circletosearch
03-23 01:08:50.114  1450  1450 I .circletosearch: Late-enabling -Xcheck:jni
03-23 01:08:50.211  1450  1450 I .circletosearch: Using CollectorTypeCC GC.
03-23 01:08:50.305  1450  1450 D nativeloader: Load libframework-connectivity-tiramisu-jni.so using APEX ns com_android_tethering for caller /apex/com.android.tethering/javalib/framework-connectivity-t.jar: ok
03-23 01:08:50.394  1450  1450 W re-initialized>: type=1400 audit(0.0:316633): avc:  granted  { execute } for  path="/data/data/com.akslabs.circletosearch/code_cache/startup_agents/6097f3dc-agent.so" dev="mmcblk0p61" ino=417800 scontext=u:r:untrusted_app:s0:c167,c259,c512,c768 tcontext=u:object_r:app_data_file:s0:c167,c259,c512,c768 tclass=file app=com.akslabs.circletosearch
03-23 01:08:50.429  1450  1450 D nativeloader: Load /data/user/0/com.akslabs.circletosearch/code_cache/startup_agents/6097f3dc-agent.so using system ns (caller=<unknown>): ok
03-23 01:08:50.429  1450  1450 V studio.deploy: Startup agent attached to VM
03-23 01:08:50.435  1450  1450 V studio.deploy: No existing instrumentation found. Loading instrumentation from instruments-6a1a1143.jar
03-23 01:08:50.491  1450  1450 W .circletosearch: DexFile /data/data/com.akslabs.circletosearch/code_cache/.studio/instruments-6a1a1143.jar is in boot class path but is not in a known location
03-23 01:08:50.689  1450  1450 V studio.deploy: Applying transforms with cached classes
03-23 01:08:50.865  1450  1450 W .circletosearch: Redefining intrinsic method java.lang.Thread java.lang.Thread.currentThread(). This may cause the unexpected use of the original definition of java.lang.Thread java.lang.Thread.currentThread()in methods that have already been compiled.
03-23 01:08:50.865  1450  1450 W .circletosearch: Redefining intrinsic method boolean java.lang.Thread.interrupted(). This may cause the unexpected use of the original definition of boolean java.lang.Thread.interrupted()in methods that have already been compiled.
03-23 01:08:50.868  1450  1450 I studio.deploy: Registering DispatchJNI
03-23 01:08:50.868  1450  1450 I studio.deploy: Found com/android/tools/deploy/interpreter/JNI
03-23 01:08:51.025  1450  1450 W ziparchive: Unable to open '/data/data/com.akslabs.circletosearch/code_cache/.overlay/base.apk/classes.dm': No such file or directory
03-23 01:08:51.192  1450  1450 W .circletosearch: ClassLoaderContext classpath size mismatch. expected=0, found=1 (PCL[] | PCL[/data/data/com.akslabs.circletosearch/code_cache/.overlay/base.apk/classes.dex*4268598632])
03-23 01:08:51.198  1450  1450 D nativeloader: Configuring clns-7 for other apk /data/app/~~UXQwZLMrccRuoxuIEEn2ZA==/com.akslabs.circletosearch-yRdMISAYBcNVWRam0Tii2Q==/base.apk. target_sdk_version=36, uses_libraries=, library_path=/data/app/~~UXQwZLMrccRuoxuIEEn2ZA==/com.akslabs.circletosearch-yRdMISAYBcNVWRam0Tii2Q==/lib/arm64:/data/app/~~UXQwZLMrccRuoxuIEEn2ZA==/com.akslabs.circletosearch-yRdMISAYBcNVWRam0Tii2Q==/base.apk!/lib/arm64-v8a, permitted_path=/data:/mnt/expand:/data/user/0/com.akslabs.circletosearch
03-23 01:08:51.206  1450  1450 I .circletosearch: AssetManager2(0x7318c278f8) locale list changing from [] to [en-IN]
03-23 01:08:51.212  1450  1450 I .circletosearch: AssetManager2(0x7318c21818) locale list changing from [] to [en-IN]
03-23 01:08:51.228  1450  1450 V GraphicsEnvironment: Currently set values for:
03-23 01:08:51.228  1450  1450 V GraphicsEnvironment:   angle_gl_driver_selection_pkgs=[com.android.angle, com.linecorp.b612.android, com.campmobile.snow, com.google.android.apps.tachyon]
03-23 01:08:51.228  1450  1450 V GraphicsEnvironment:   angle_gl_driver_selection_values=[angle, native, native, native]
03-23 01:08:51.228  1450  1450 V GraphicsEnvironment: com.akslabs.circletosearch is not listed in per-application setting
03-23 01:08:51.228  1450  1450 V GraphicsEnvironment: Neither updatable production driver nor prerelease driver is supported.
03-23 01:08:51.343  1450  1450 I .circletosearch: AssetManager2(0x7318c1d358) locale list changing from [] to [en-IN]
03-23 01:08:51.450  1450  1450 D DesktopModeFlagsUtil: Toggle override initialized to: OVERRIDE_UNSET
03-23 01:08:51.975  1450  1450 W .circletosearch: Accessing hidden method Landroid/os/SystemProperties;->addChangeCallback(Ljava/lang/Runnable;)V (unsupported, reflection, allowed)
03-23 01:08:52.470  1450  1450 W HWUI    : Image decoding logging dropped!
03-23 01:08:52.804  1450  1536 I AdrenoGLES-0: QUALCOMM build                   : 95db91f, Ifbc588260a
03-23 01:08:52.804  1450  1536 I AdrenoGLES-0: Build Date                       : 09/24/20
03-23 01:08:52.804  1450  1536 I AdrenoGLES-0: OpenGL ES Shader Compiler Version: EV031.32.02.01
03-23 01:08:52.804  1450  1536 I AdrenoGLES-0: Local Branch                     : mybrancheafe5b6d-fb5b-f1b0-b904-5cb90179c3e0
03-23 01:08:52.804  1450  1536 I AdrenoGLES-0: Remote Branch                    : quic/gfx-adreno.lnx.1.0.r114-rel
03-23 01:08:52.804  1450  1536 I AdrenoGLES-0: Remote Branch                    : NONE
03-23 01:08:52.804  1450  1536 I AdrenoGLES-0: Reconstruct Branch               : NOTHING
03-23 01:08:52.804  1450  1536 I AdrenoGLES-0: Build Config                     : S P 10.0.7 AArch64
03-23 01:08:52.804  1450  1536 I AdrenoGLES-0: Driver Path                      : /vendor/lib64/egl/libGLESv2_adreno.so
03-23 01:08:52.821  1450  1536 I AdrenoGLES-0: PFP: 0x016ee190, ME: 0x00000000
03-23 01:08:52.930  1450  1559 I Gralloc4: mapper 4.x is not supported
03-23 01:08:52.930  1450  1559 W Gralloc3: mapper 3.x is not supported
03-23 01:08:52.939  1450  1559 I Gralloc2: Adding additional valid usage bits: 0x8202000
03-23 01:08:53.026  1450  1469 I HWUI    : Davey! duration=1697ms; Flags=1, FrameTimelineVsyncId=96236844, IntendedVsync=122869838241344, Vsync=122870121580361, InputEventId=0, HandleInputStart=122870134057033, AnimationStart=122870134068439, PerformTraversalsStart=122870134269064, DrawStart=122871435255522, FrameDeadline=122869859574676, FrameInterval=122870133683752, FrameStartTime=16667001, SyncQueued=122871489577241, SyncStart=122871489711043, IssueDrawCommandsStart=122871490430783, SwapBuffers=122871532341668, FrameCompleted=122871536251303, DequeueBufferDuration=14687, QueueBufferDuration=654480, GpuCompleted=122871536251303, SwapBuffersCompleted=122871534357866, DisplayPresentTime=0, CommandSubmissionCompleted=122871532341668, 
03-23 01:08:53.114  1450  1450 I Choreographer: Skipped 83 frames!  The application may be doing too much work on its main thread.
03-23 01:08:53.130  1450  1469 I HWUI    : Davey! duration=1399ms; Flags=0, FrameTimelineVsyncId=96237256, IntendedVsync=122870257575722, Vsync=122871640984696, InputEventId=0, HandleInputStart=122871649082658, AnimationStart=122871649086147, PerformTraversalsStart=122871649087085, DrawStart=122871650298074, FrameDeadline=122871554942112, FrameInterval=122871648320210, FrameStartTime=16667578, SyncQueued=122871650961147, SyncStart=122871651172814, IssueDrawCommandsStart=122871651266251, SwapBuffers=122871653528960, FrameCompleted=122871657284220, DequeueBufferDuration=15573, QueueBufferDuration=259583, GpuCompleted=122871657284220, SwapBuffersCompleted=122871654519845, DisplayPresentTime=0, CommandSubmissionCompleted=122871653528960, 
03-23 01:08:53.306  1450  1466 I HWUI    : Davey! duration=1563ms; Flags=1, FrameTimelineVsyncId=96237256, IntendedVsync=122870257575722, Vsync=122871640984696, InputEventId=0, HandleInputStart=122871649082658, AnimationStart=122871649086147, PerformTraversalsStart=122871649087085, DrawStart=122871801525105, FrameDeadline=122870274242388, FrameInterval=122871648320210, FrameStartTime=16667578, SyncQueued=122871812772553, SyncStart=122871812914949, IssueDrawCommandsStart=122871813002137, SwapBuffers=122871819126043, FrameCompleted=122871821033230, DequeueBufferDuration=12969, QueueBufferDuration=250677, GpuCompleted=122871821033230, SwapBuffersCompleted=122871819915574, DisplayPresentTime=0, CommandSubmissionCompleted=122871819126043, 
03-23 01:08:53.387  1450  1450 D InsetsController: hide(ime(), fromIme=false)
03-23 01:08:53.387  1450  1450 I ImeTracker: com.akslabs.circletosearch:47e3d2e0: onCancelled at PHASE_CLIENT_ALREADY_HIDDEN
03-23 01:08:54.659  1450  1450 W WindowOnBackDispatcher: sendCancelIfRunning: isInProgress=false callback=androidx.navigationevent.OnBackInvokedInput$createOnBackAnimationCallback$1@baca4b6
03-23 01:08:54.683  1450  1536 D HWUI    : endAllActiveAnimators on 0x7218c6d7c0 (UnprojectedRipple) with handle 0x72c8cad250
03-23 01:08:54.733  1450  1450 D InsetsController: hide(ime(), fromIme=false)
03-23 01:08:54.733  1450  1450 I ImeTracker: com.akslabs.circletosearch:8ea12879: onCancelled at PHASE_CLIENT_ALREADY_HIDDEN
03-23 01:08:56.873  1450  1802 D ProfileInstaller: Installing profile for com.akslabs.circletosearch
03-23 01:08:58.751  1450  1450 D VRI[MainActivity]: visibilityChanged oldVisibility=true newVisibility=false
03-23 01:09:04.409  1450  1461 W .circletosearch: Cleared Reference was only reachable from finalizer (only reported once)
03-23 01:09:04.429  1450  1461 I .circletosearch: Background concurrent copying GC freed 10056KB AllocSpace bytes, 7(140KB) LOS objects, 49% free, 5103KB/10206KB, paused 65us,22us total 102.452ms
03-23 01:09:06.498  1450  1450 D CircleToSearch: performCapture called. hasWindowManager=true
03-23 01:09:06.612  1450  2402 D CircleToSearchAccess: AccessibilityService launching OverlayActivity
03-23 01:09:06.820  1450  1450 D CircleToSearch: OverlayActivity onCreate
03-23 01:09:06.820  1450  1450 D CircleToSearch: Bitmap loaded from Repository. Size: 1080x2340
03-23 01:09:07.889  1450  1571 I HWUI    : Davey! duration=1033ms; Flags=1, FrameTimelineVsyncId=96256380, IntendedVsync=122885358073707, Vsync=122885391407653, InputEventId=0, HandleInputStart=122885399394579, AnimationStart=122885399396819, PerformTraversalsStart=122885399397757, DrawStart=122886312984319, FrameDeadline=122885374740373, FrameInterval=122885399385413, FrameStartTime=16666973, SyncQueued=122886331237444, SyncStart=122886331530360, IssueDrawCommandsStart=122886331688017, SwapBuffers=122886381614944, FrameCompleted=122886392183381, DequeueBufferDuration=13333, QueueBufferDuration=701823, GpuCompleted=122886392183381, SwapBuffersCompleted=122886383234944, DisplayPresentTime=122871839892814, CommandSubmissionCompleted=122886381614944, 
03-23 01:09:07.906  1450  1450 D CircleToSearch: QR Scan starting - Bitmap: 1080x2340
03-23 01:09:07.932  1450  1450 I Choreographer: Skipped 63 frames!  The application may be doing too much work on its main thread.
03-23 01:09:08.012  1450  1450 W HWUI    : Image decoding logging dropped!
03-23 01:09:08.093  1450  1450 W HWUI    : Image decoding logging dropped!
03-23 01:09:08.479  1450  2429 I HWUI    : Davey! duration=1596ms; Flags=0, FrameTimelineVsyncId=96256491, IntendedVsync=122885408072041, Vsync=122886458088631, InputEventId=0, HandleInputStart=122886466590152, AnimationStart=122886466593798, PerformTraversalsStart=122886799270204, DrawStart=122886939968121, FrameDeadline=122886424759672, FrameInterval=122886466186871, FrameStartTime=16666930, SyncQueued=122886990125620, SyncStart=122886990507860, IssueDrawCommandsStart=122886990867391, SwapBuffers=122886993833381, FrameCompleted=122887004494110, DequeueBufferDuration=23958, QueueBufferDuration=509531, GpuCompleted=122887004494110, SwapBuffersCompleted=122886995426193, DisplayPresentTime=122871906560626, CommandSubmissionCompleted=122886993833381, 
03-23 01:09:08.647  1450  1450 D InsetsController: hide(ime(), fromIme=false)
03-23 01:09:08.647  1450  1450 I ImeTracker: com.akslabs.circletosearch:8121a967: onCancelled at PHASE_CLIENT_ALREADY_HIDDEN
03-23 01:09:10.444  1450  1450 D CircleToSearch: QR Scan finished - Found: 0 codes
03-23 01:09:19.411  1450  1450 D VRI[OverlayActivity]: visibilityChanged oldVisibility=true newVisibility=false
03-23 01:09:19.439  1450  1450 W WindowOnBackDispatcher: sendCancelIfRunning: isInProgress=false callback=androidx.navigationevent.OnBackInvokedInput$createOnBackAnimationCallback$1@f22adfc
03-23 01:09:19.561  1450  1450 W WindowOnBackDispatcher: sendCancelIfRunning: isInProgress=false callback=android.app.Activity$$ExternalSyntheticLambda0@56af219
03-23 01:09:19.566  1450  1450 D ViewRootImpl: Skipping stats log for color mode
03-23 01:09:50.001  1450  1450 D CircleToSearch: performCapture called. hasWindowManager=true
03-23 01:09:50.059  1450  2402 D CircleToSearchAccess: AccessibilityService launching OverlayActivity
03-23 01:09:50.226  1450  1450 D CircleToSearch: OverlayActivity onCreate
03-23 01:09:50.226  1450  1450 D CircleToSearch: Bitmap loaded from Repository. Size: 1080x2340
03-23 01:09:50.852  1450  1450 D CircleToSearch: QR Scan starting - Bitmap: 1080x2340
03-23 01:09:50.865  1450  1450 I Choreographer: Skipped 35 frames!  The application may be doing too much work on its main thread.
03-23 01:09:50.913  1450  1450 W HWUI    : Image decoding logging dropped!
03-23 01:09:50.941  1450  1450 W HWUI    : Image decoding logging dropped!
03-23 01:09:51.245  1450  1466 I HWUI    : Davey! duration=956ms; Flags=0, FrameTimelineVsyncId=96308265, IntendedVsync=122928808061365, Vsync=122929391387325, InputEventId=0, HandleInputStart=122929399749615, AnimationStart=122929399753208, PerformTraversalsStart=122929597769719, DrawStart=122929712644146, FrameDeadline=122929391378856, FrameInterval=122929399224615, FrameStartTime=16666456, SyncQueued=122929752165865, SyncStart=122929752234979, IssueDrawCommandsStart=122929752406125, SwapBuffers=122929753767063, FrameCompleted=122929764206542, DequeueBufferDuration=15730, QueueBufferDuration=532657, GpuCompleted=122929764206542, SwapBuffersCompleted=122929755043156, DisplayPresentTime=-69972189863528145, CommandSubmissionCompleted=122929753767063, 
03-23 01:09:51.266  1450  1450 D InsetsController: hide(ime(), fromIme=false)
03-23 01:09:51.266  1450  1450 I ImeTracker: com.akslabs.circletosearch:89ad9691: onCancelled at PHASE_CLIENT_ALREADY_HIDDEN
03-23 01:09:51.705  1450  1450 D CircleToSearch: QR Scan finished - Found: 0 codes
03-23 01:10:07.779  1450  1450 D VRI[OverlayActivity]: visibilityChanged oldVisibility=true newVisibility=false
03-23 01:10:07.812  1450  1450 W WindowOnBackDispatcher: sendCancelIfRunning: isInProgress=false callback=androidx.navigationevent.OnBackInvokedInput$createOnBackAnimationCallback$1@2665684
03-23 01:10:07.945  1450  1450 W WindowOnBackDispatcher: sendCancelIfRunning: isInProgress=false callback=android.app.Activity$$ExternalSyntheticLambda0@78f1e62
03-23 01:10:07.949  1450  1450 D ViewRootImpl: Skipping stats log for color mode
03-23 01:10:10.795  1450  1450 D CircleToSearch: performCapture called. hasWindowManager=true
03-23 01:10:10.864  1450  2402 D CircleToSearchAccess: AccessibilityService launching OverlayActivity
03-23 01:10:11.105  1450  1450 D CircleToSearch: OverlayActivity onCreate
03-23 01:10:11.105  1450  1450 D CircleToSearch: Bitmap loaded from Repository. Size: 1080x2340
03-23 01:10:11.383  1450  1457 I .circletosearch: Compiler allocated 5163KB to compile void com.akslabs.circletosearch.ui.CircleToSearchScreenKt$CircleToSearchScreen$13$2.invoke(androidx.compose.foundation.layout.PaddingValues, androidx.compose.runtime.Composer, int)
03-23 01:10:11.598  1450  1450 D CircleToSearch: QR Scan starting - Bitmap: 1080x2340
03-23 01:10:11.688  1450  1450 W HWUI    : Image decoding logging dropped!
03-23 01:10:11.724  1450  1450 W HWUI    : Image decoding logging dropped!
03-23 01:10:11.785  1450  1535 I .circletosearch: Waiting for a blocking GC ProfileSaver
03-23 01:10:11.803  1450  1461 I .circletosearch: Background concurrent copying GC freed 3776KB AllocSpace bytes, 13(31MB) LOS objects, 50% free, 19MB/39MB, paused 106us,49us total 199.866ms
03-23 01:10:11.804  1450  1535 I .circletosearch: WaitForGcToComplete blocked ProfileSaver on Background for 18.859ms
03-23 01:10:12.002  1450  1467 I HWUI    : Davey! duration=850ms; Flags=0, FrameTimelineVsyncId=96334223, IntendedVsync=122949674733337, Vsync=122950158067028, InputEventId=0, HandleInputStart=122950172554451, AnimationStart=122950172558096, PerformTraversalsStart=122950375630492, DrawStart=122950469565128, FrameDeadline=122950108060013, FrameInterval=122950172148617, FrameStartTime=16666679, SyncQueued=122950512905648, SyncStart=122950512985596, IssueDrawCommandsStart=122950513161430, SwapBuffers=122950514883669, FrameCompleted=122950525548617, DequeueBufferDuration=15729, QueueBufferDuration=1063177, GpuCompleted=122950525548617, SwapBuffersCompleted=122950516516742, DisplayPresentTime=0, CommandSubmissionCompleted=122950514883669, 
03-23 01:10:12.028  1450  1450 D InsetsController: hide(ime(), fromIme=false)
03-23 01:10:12.028  1450  1450 I ImeTracker: com.akslabs.circletosearch:b7697256: onCancelled at PHASE_CLIENT_ALREADY_HIDDEN
03-23 01:10:12.421  1450  1450 D CircleToSearch: QR Scan finished - Found: 0 codes
03-23 01:10:21.749  1450  2425 D TesseractEngine: Initializing Tesseract with dataPath=/data/user/0/com.akslabs.circletosearch/files
03-23 01:10:21.764  1450  2425 D nativeloader: Load /data/app/~~UXQwZLMrccRuoxuIEEn2ZA==/com.akslabs.circletosearch-yRdMISAYBcNVWRam0Tii2Q==/lib/arm64/libjpeg.so using class loader ns clns-7 (caller=/data/data/com.akslabs.circletosearch/code_cache/.overlay/base.apk/classes.dex): ok
03-23 01:10:21.770  1450  2425 D nativeloader: Load /data/app/~~UXQwZLMrccRuoxuIEEn2ZA==/com.akslabs.circletosearch-yRdMISAYBcNVWRam0Tii2Q==/lib/arm64/libpngx.so using class loader ns clns-7 (caller=/data/data/com.akslabs.circletosearch/code_cache/.overlay/base.apk/classes.dex): ok
03-23 01:10:21.782  1450  2425 D nativeloader: Load /data/app/~~UXQwZLMrccRuoxuIEEn2ZA==/com.akslabs.circletosearch-yRdMISAYBcNVWRam0Tii2Q==/lib/arm64/libleptonica.so using class loader ns clns-7 (caller=/data/data/com.akslabs.circletosearch/code_cache/.overlay/base.apk/classes.dex): ok
03-23 01:10:21.835  1450  2425 D nativeloader: Load /data/app/~~UXQwZLMrccRuoxuIEEn2ZA==/com.akslabs.circletosearch-yRdMISAYBcNVWRam0Tii2Q==/lib/arm64/libtesseract.so using class loader ns clns-7 (caller=/data/data/com.akslabs.circletosearch/code_cache/.overlay/base.apk/classes.dex): ok
03-23 01:10:22.254  1450  2425 I Tesseract(native): Initialized Tesseract API with language=eng
03-23 01:10:22.258  1450  2425 W .circletosearch: Accessing hidden field Landroid/graphics/Bitmap;->mNativePtr:J (unsupported, JNI, allowed)
03-23 01:10:24.934  1450  2425 D TesseractEngine: Tesseract recognition complete. Full text length: 1070
03-23 01:10:25.017  1450  2425 D TesseractEngine: Tesseract successfully extracted 59 text nodes.
03-23 01:10:25.022  1450  1450 D CopyTextOverlay: OCR complete: 59 nodes, 255 words
03-23 01:10:27.089  1450  1450 W HWUI    : Image decoding logging dropped!
03-23 01:10:27.105  1450  1450 W HWUI    : Image decoding logging dropped!
