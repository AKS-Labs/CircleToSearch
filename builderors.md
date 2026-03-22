--------- beginning of main
03-22 11:59:01.311 30753 30753 W main    : type=1400 audit(0.0:128525): avc:  denied  { search } for  name="app" dev="mmcblk0p61" ino=1343489 scontext=u:r:zygote:s0 tcontext=u:object_r:apk_data_file:s0 tclass=dir permissive=0 app=net.one97.paytm
03-22 11:59:01.311 30753 30753 W main    : type=1400 audit(0.0:128526): avc:  denied  { search } for  name="app" dev="mmcblk0p61" ino=1343489 scontext=u:r:zygote:s0 tcontext=u:object_r:apk_data_file:s0 tclass=dir permissive=0 app=net.one97.paytm
03-22 11:59:01.311 30753 30753 W main    : type=1400 audit(0.0:128527): avc:  denied  { search } for  name="app" dev="mmcblk0p61" ino=1343489 scontext=u:r:zygote:s0 tcontext=u:object_r:apk_data_file:s0 tclass=dir permissive=0 app=net.one97.paytm
03-22 11:59:01.311 30753 30753 W main    : type=1400 audit(0.0:128528): avc:  denied  { search } for  name="app" dev="mmcblk0p61" ino=1343489 scontext=u:r:zygote:s0 tcontext=u:object_r:apk_data_file:s0 tclass=dir permissive=0 app=net.one97.paytm
03-22 11:59:01.338 30753 30753 I .circletosearch: Late-enabling -Xcheck:jni
03-22 11:59:01.456 30753 30753 I .circletosearch: Using CollectorTypeCC GC.
03-22 11:59:01.581 30753 30753 D nativeloader: Load libframework-connectivity-tiramisu-jni.so using APEX ns com_android_tethering for caller /apex/com.android.tethering/javalib/framework-connectivity-t.jar: ok
03-22 11:59:01.751 30753 30753 W re-initialized>: type=1400 audit(0.0:128529): avc:  granted  { execute } for  path="/data/data/com.akslabs.circletosearch/code_cache/startup_agents/6097f3dc-agent.so" dev="mmcblk0p61" ino=417800 scontext=u:r:untrusted_app:s0:c167,c259,c512,c768 tcontext=u:object_r:app_data_file:s0:c167,c259,c512,c768 tclass=file app=net.one97.paytm
03-22 11:59:01.775 30753 30753 D nativeloader: Load /data/user/0/com.akslabs.circletosearch/code_cache/startup_agents/6097f3dc-agent.so using system ns (caller=<unknown>): ok
03-22 11:59:01.778 30753 30753 V studio.deploy: Startup agent attached to VM
03-22 11:59:01.780 30753 30753 V studio.deploy: No existing instrumentation found. Loading instrumentation from instruments-6a1a1143.jar
03-22 11:59:01.799 30753 30753 W .circletosearch: DexFile /data/data/com.akslabs.circletosearch/code_cache/.studio/instruments-6a1a1143.jar is in boot class path but is not in a known location
03-22 11:59:01.813 30753 30753 V studio.deploy: Applying transforms with cached classes
03-22 11:59:02.022 30753 30753 W .circletosearch: Redefining intrinsic method java.lang.Thread java.lang.Thread.currentThread(). This may cause the unexpected use of the original definition of java.lang.Thread java.lang.Thread.currentThread()in methods that have already been compiled.
03-22 11:59:02.022 30753 30753 W .circletosearch: Redefining intrinsic method boolean java.lang.Thread.interrupted(). This may cause the unexpected use of the original definition of boolean java.lang.Thread.interrupted()in methods that have already been compiled.
03-22 11:59:02.027 30753 30753 I studio.deploy: Registering DispatchJNI
03-22 11:59:02.028 30753 30753 I studio.deploy: Found com/android/tools/deploy/interpreter/JNI
03-22 11:59:02.120 30753 30753 W ziparchive: Unable to open '/data/data/com.akslabs.circletosearch/code_cache/.overlay/base.apk/classes.dm': No such file or directory
03-22 11:59:02.304 30753 30753 W ziparchive: Unable to open '/data/app/~~UXQwZLMrccRuoxuIEEn2ZA==/com.akslabs.circletosearch-yRdMISAYBcNVWRam0Tii2Q==/base.dm': No such file or directory
03-22 11:59:02.304 30753 30753 W ziparchive: Unable to open '/data/app/~~UXQwZLMrccRuoxuIEEn2ZA==/com.akslabs.circletosearch-yRdMISAYBcNVWRam0Tii2Q==/base.dm': No such file or directory
03-22 11:59:02.435 30753 30753 D nativeloader: Configuring clns-7 for other apk /data/app/~~UXQwZLMrccRuoxuIEEn2ZA==/com.akslabs.circletosearch-yRdMISAYBcNVWRam0Tii2Q==/base.apk. target_sdk_version=36, uses_libraries=, library_path=/data/app/~~UXQwZLMrccRuoxuIEEn2ZA==/com.akslabs.circletosearch-yRdMISAYBcNVWRam0Tii2Q==/lib/arm64:/data/app/~~UXQwZLMrccRuoxuIEEn2ZA==/com.akslabs.circletosearch-yRdMISAYBcNVWRam0Tii2Q==/base.apk!/lib/arm64-v8a, permitted_path=/data:/mnt/expand:/data/user/0/com.akslabs.circletosearch
03-22 11:59:02.462 30753 30753 I .circletosearch: AssetManager2(0x7318c2a4b8) locale list changing from [] to [en-IN]
03-22 11:59:02.466 30753 30753 I .circletosearch: AssetManager2(0x7318c243d8) locale list changing from [] to [en-IN]
03-22 11:59:02.492 30753 30753 V GraphicsEnvironment: Currently set values for:
03-22 11:59:02.492 30753 30753 V GraphicsEnvironment:   angle_gl_driver_selection_pkgs=[com.android.angle, com.linecorp.b612.android, com.campmobile.snow, com.google.android.apps.tachyon]
03-22 11:59:02.492 30753 30753 V GraphicsEnvironment:   angle_gl_driver_selection_values=[angle, native, native, native]
03-22 11:59:02.492 30753 30753 V GraphicsEnvironment: com.akslabs.circletosearch is not listed in per-application setting
03-22 11:59:02.493 30753 30753 V GraphicsEnvironment: Neither updatable production driver nor prerelease driver is supported.
03-22 11:59:02.617 30753 30753 I .circletosearch: AssetManager2(0x7318c1d358) locale list changing from [] to [en-IN]
03-22 11:59:02.740 30753 30753 D DesktopModeFlagsUtil: Toggle override initialized to: OVERRIDE_UNSET
03-22 11:59:03.424 30753 30753 W .circletosearch: Accessing hidden method Landroid/os/SystemProperties;->addChangeCallback(Ljava/lang/Runnable;)V (unsupported, reflection, allowed)
03-22 11:59:03.837 30753 30753 W HWUI    : Image decoding logging dropped!
03-22 11:59:04.146 30753 30786 I AdrenoGLES-0: QUALCOMM build                   : 95db91f, Ifbc588260a
03-22 11:59:04.146 30753 30786 I AdrenoGLES-0: Build Date                       : 09/24/20
03-22 11:59:04.146 30753 30786 I AdrenoGLES-0: OpenGL ES Shader Compiler Version: EV031.32.02.01
03-22 11:59:04.146 30753 30786 I AdrenoGLES-0: Local Branch                     : mybrancheafe5b6d-fb5b-f1b0-b904-5cb90179c3e0
03-22 11:59:04.146 30753 30786 I AdrenoGLES-0: Remote Branch                    : quic/gfx-adreno.lnx.1.0.r114-rel
03-22 11:59:04.146 30753 30786 I AdrenoGLES-0: Remote Branch                    : NONE
03-22 11:59:04.146 30753 30786 I AdrenoGLES-0: Reconstruct Branch               : NOTHING
03-22 11:59:04.146 30753 30786 I AdrenoGLES-0: Build Config                     : S P 10.0.7 AArch64
03-22 11:59:04.146 30753 30786 I AdrenoGLES-0: Driver Path                      : /vendor/lib64/egl/libGLESv2_adreno.so
03-22 11:59:04.163 30753 30786 I AdrenoGLES-0: PFP: 0x016ee190, ME: 0x00000000
03-22 11:59:04.240 30753 30801 I Gralloc4: mapper 4.x is not supported
03-22 11:59:04.241 30753 30801 W Gralloc3: mapper 3.x is not supported
03-22 11:59:04.250 30753 30801 I Gralloc2: Adding additional valid usage bits: 0x8202000
03-22 11:59:04.367 30753 30764 I HWUI    : Davey! duration=1446ms; Flags=1, FrameTimelineVsyncId=59465236, IntendedVsync=75778795778634, Vsync=75778812445285, InputEventId=0, HandleInputStart=75778813026039, AnimationStart=75778813030310, PerformTraversalsStart=75778813032601, DrawStart=75780137228174, FrameDeadline=75778812445300, FrameInterval=75778812955466, FrameStartTime=16666651, SyncQueued=75780191665569, SyncStart=75780191862601, IssueDrawCommandsStart=75780193072340, SwapBuffers=75780234770153, FrameCompleted=75780242905361, DequeueBufferDuration=16666, QueueBufferDuration=309375, GpuCompleted=75780242905361, SwapBuffersCompleted=75780235698538, DisplayPresentTime=0, CommandSubmissionCompleted=75780234770153, 
03-22 11:59:04.452 30753 30753 I Choreographer: Skipped 81 frames!  The application may be doing too much work on its main thread.
03-22 11:59:04.473 30753 30766 I HWUI    : Davey! duration=1361ms; Flags=0, FrameTimelineVsyncId=59465618, IntendedVsync=75778995781318, Vsync=75780345793819, InputEventId=0, HandleInputStart=75780347537809, AnimationStart=75780347541559, PerformTraversalsStart=75780347542757, DrawStart=75780348802705, FrameDeadline=75780262443937, FrameInterval=75780346471142, FrameStartTime=16666821, SyncQueued=75780349536611, SyncStart=75780349791819, IssueDrawCommandsStart=75780349898174, SwapBuffers=75780352163747, FrameCompleted=75780357048069, DequeueBufferDuration=16510, QueueBufferDuration=561250, GpuCompleted=75780357048069, SwapBuffersCompleted=75780354418434, DisplayPresentTime=0, CommandSubmissionCompleted=75780352163747, 
03-22 11:59:04.650 30753 30764 I HWUI    : Davey! duration=1523ms; Flags=1, FrameTimelineVsyncId=59465618, IntendedVsync=75778995781318, Vsync=75780345793819, InputEventId=0, HandleInputStart=75780347537809, AnimationStart=75780347541559, PerformTraversalsStart=75780347542757, DrawStart=75780498748486, FrameDeadline=75779012447984, FrameInterval=75780346471142, FrameStartTime=16666821, SyncQueued=75780507257184, SyncStart=75780507416403, IssueDrawCommandsStart=75780507562601, SwapBuffers=75780517272809, FrameCompleted=75780519717757, DequeueBufferDuration=16146, QueueBufferDuration=392083, GpuCompleted=75780519717757, SwapBuffersCompleted=75780519044476, DisplayPresentTime=0, CommandSubmissionCompleted=75780517272809, 
03-22 11:59:04.731 30753 30753 D InsetsController: hide(ime(), fromIme=false)
03-22 11:59:04.732 30753 30753 I ImeTracker: com.akslabs.circletosearch:9eb1d189: onCancelled at PHASE_CLIENT_ALREADY_HIDDEN
03-22 11:59:07.661 30753 30811 D ProfileInstaller: Installing profile for com.akslabs.circletosearch
03-22 11:59:09.632 30753 30753 W WindowOnBackDispatcher: sendCancelIfRunning: isInProgress=false callback=androidx.navigationevent.OnBackInvokedInput$createOnBackAnimationCallback$1@baca4b6
03-22 11:59:09.693 30753 30753 D InsetsController: hide(ime(), fromIme=false)
03-22 11:59:09.693 30753 30753 I ImeTracker: com.akslabs.circletosearch:8c312b7c: onCancelled at PHASE_CLIENT_ALREADY_HIDDEN
03-22 11:59:14.473 30753 30759 W .circletosearch: Cleared Reference was only reachable from finalizer (only reported once)
03-22 11:59:14.482 30753 30759 I .circletosearch: Background concurrent copying GC freed 6467KB AllocSpace bytes, 5(100KB) LOS objects, 49% free, 5109KB/10219KB, paused 81us,21us total 115.589ms
03-22 11:59:16.157 30753 30753 D VRI[MainActivity]: visibilityChanged oldVisibility=true newVisibility=false
03-22 11:59:26.134 30753 30753 D CircleToSearch: performCapture called. hasWindowManager=true
03-22 11:59:26.290 30753 30879 D CircleToSearchAccess: AccessibilityService launching OverlayActivity
03-22 11:59:26.607 30753 30753 D CircleToSearch: OverlayActivity onCreate
03-22 11:59:26.607 30753 30753 D CircleToSearch: Bitmap loaded from Repository. Size: 1080x2340
03-22 11:59:27.383 30753 30764 I HWUI    : Davey! duration=734ms; Flags=1, FrameTimelineVsyncId=59492836, IntendedVsync=75802512435439, Vsync=75802529101932, InputEventId=0, HandleInputStart=75802537466186, AnimationStart=75802537468582, PerformTraversalsStart=75802537469623, DrawStart=75803169531029, FrameDeadline=75802529102105, FrameInterval=75802537457905, FrameStartTime=16666493, SyncQueued=75803205949779, SyncStart=75803206087852, IssueDrawCommandsStart=75803206253634, SwapBuffers=75803236669936, FrameCompleted=75803247509154, DequeueBufferDuration=13386, QueueBufferDuration=277187, GpuCompleted=75803247509154, SwapBuffersCompleted=75803237696707, DisplayPresentTime=0, CommandSubmissionCompleted=75803236669936, 
03-22 11:59:27.421 30753 30753 I Choreographer: Skipped 46 frames!  The application may be doing too much work on its main thread.
03-22 11:59:27.536 30753 30753 W HWUI    : Image decoding logging dropped!
03-22 11:59:27.576 30753 30753 W HWUI    : Image decoding logging dropped!
03-22 11:59:27.958 30753 30889 I HWUI    : Davey! duration=1293ms; Flags=0, FrameTimelineVsyncId=59492894, IntendedVsync=75802545768207, Vsync=75803312426379, InputEventId=0, HandleInputStart=75803316575248, AnimationStart=75803316580040, PerformTraversalsStart=75803637078113, DrawStart=75803782574623, FrameDeadline=75803279094290, FrameInterval=75803315908842, FrameStartTime=16666482, SyncQueued=75803825303738, SyncStart=75803825398321, IssueDrawCommandsStart=75803825730248, SwapBuffers=75803828598738, FrameCompleted=75803839195300, DequeueBufferDuration=24688, QueueBufferDuration=435990, GpuCompleted=75803839195300, SwapBuffersCompleted=75803830271967, DisplayPresentTime=0, CommandSubmissionCompleted=75803828598738, 
03-22 11:59:28.126 30753 30753 D InsetsController: hide(ime(), fromIme=false)
03-22 11:59:28.127 30753 30753 I ImeTracker: com.akslabs.circletosearch:bfb28632: onCancelled at PHASE_CLIENT_ALREADY_HIDDEN
03-22 11:59:53.784 30753 30753 W .circletosearch: Accessing hidden method Landroid/view/ViewTreeObserver;->dispatchOnScrollChanged()V (unsupported, reflection, allowed)
03-22 12:00:18.124 30753 30753 D VRI[OverlayActivity]: visibilityChanged oldVisibility=true newVisibility=false
03-22 12:00:18.149 30753 30753 W WindowOnBackDispatcher: sendCancelIfRunning: isInProgress=false callback=androidx.navigationevent.OnBackInvokedInput$createOnBackAnimationCallback$1@f9797ed
03-22 12:00:18.320 30753 30753 W WindowOnBackDispatcher: sendCancelIfRunning: isInProgress=false callback=android.app.Activity$$ExternalSyntheticLambda0@a436fa6
03-22 12:00:18.326 30753 30753 D ViewRootImpl: Skipping stats log for color mode
