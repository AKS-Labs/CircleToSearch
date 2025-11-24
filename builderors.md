 I  Driver Path                      : /vendor/lib64/egl/libGLESv2_adreno.so
2025-11-25 00:56:59.092 15784-19355 AdrenoGLES-0            com.akslabs.circletosearch           I  PFP: 0x016ee190, ME: 0x00000000
2025-11-25 00:56:59.093 15784-15784 ashmem                  com.akslabs.circletosearch           E  Pinning is deprecated since Android Q. Please use trim or other methods.
2025-11-25 00:56:59.119 15784-15784 CompatChangeReporter    com.akslabs.circletosearch           D  Compat change id reported: 377864165; UID 10727; state: ENABLED
2025-11-25 00:56:59.123 15784-15784 DesktopModeFlags        com.akslabs.circletosearch           D  Toggle override initialized to: OVERRIDE_UNSET
2025-11-25 00:56:59.188 15784-19358 Gralloc4                com.akslabs.circletosearch           I  mapper 4.x is not supported
2025-11-25 00:56:59.189 15784-19358 Gralloc3                com.akslabs.circletosearch           W  mapper 3.x is not supported
2025-11-25 00:56:59.449 15784-15784 .circletosearch         com.akslabs.circletosearch           I  hiddenapi: Accessing hidden method Landroid/os/SystemProperties;->addChangeCallback(Ljava/lang/Runnable;)V (runtime_flags=0, domain=platform, api=unsupported) from Landroidx/compose/ui/platform/AndroidComposeView$Companion; (domain=app) using reflection: allowed
2025-11-25 00:56:59.709 15784-19361 Gralloc2                com.akslabs.circletosearch           I  Adding additional valid usage bits: 0x8202000
2025-11-25 00:56:59.797 15784-15784 Choreographer           com.akslabs.circletosearch           I  Skipped 31 frames!  The application may be doing too much work on its main thread.
2025-11-25 00:56:59.832 15784-15784 InsetsController        com.akslabs.circletosearch           D  hide(ime(), fromIme=false)
2025-11-25 00:56:59.832 15784-15784 ImeTracker              com.akslabs.circletosearch           I  com.akslabs.circletosearch:9a70c17a: onCancelled at PHASE_CLIENT_ALREADY_HIDDEN
2025-11-25 00:57:04.613 15784-19419 ProfileInstaller        com.akslabs.circletosearch           D  Installing profile for com.akslabs.circletosearch
2025-11-25 00:57:20.162 15784-15784 InsetsController        com.akslabs.circletosearch           D  hide(ime(), fromIme=false)
2025-11-25 00:57:20.162 15784-15784 ImeTracker              com.akslabs.circletosearch           I  com.akslabs.circletosearch:3d237781: onCancelled at PHASE_CLIENT_ALREADY_HIDDEN
2025-11-25 00:57:32.587 15784-15784 VRI[MainActivity]       com.akslabs.circletosearch           D  visibilityChanged oldVisibility=true newVisibility=false
2025-11-25 00:57:41.409 15784-15784 ViewRootImpl            com.akslabs.circletosearch           D  Skipping stats log for color mode
2025-11-25 00:57:41.644 15784-15784 InsetsController        com.akslabs.circletosearch           D  hide(ime(), fromIme=false)
2025-11-25 00:57:41.644 15784-15784 ImeTracker              com.akslabs.circletosearch           I  com.akslabs.circletosearch:9f55d8d8: onCancelled at PHASE_CLIENT_ALREADY_HIDDEN
2025-11-25 00:57:43.171 15784-15784 VRI[MainActivity]       com.akslabs.circletosearch           D  visibilityChanged oldVisibility=true newVisibility=false
2025-11-25 00:57:47.964 15784-15784 ViewRootImpl            com.akslabs.circletosearch           D  Skipping stats log for color mode
2025-11-25 00:57:48.144 15784-15784 InsetsController        com.akslabs.circletosearch           D  hide(ime(), fromIme=false)
2025-11-25 00:57:48.144 15784-15784 ImeTracker              com.akslabs.circletosearch           I  com.akslabs.circletosearch:79f5751f: onCancelled at PHASE_CLIENT_ALREADY_HIDDEN
2025-11-25 00:57:53.148 15784-15784 InputEventReceiver      com.akslabs.circletosearch           E  Exception dispatching input event.
2025-11-25 00:57:53.148 15784-15784 MessageQueue-JNI        com.akslabs.circletosearch           E  Exception in MessageQueue callback: handleReceiveCallback
2025-11-25 00:57:53.160 15784-15784 MessageQueue-JNI        com.akslabs.circletosearch           E  java.lang.SecurityException: Services don't have the capability of taking the screenshot. (Fix with AI)
                                                                                                    	at android.os.Parcel.createExceptionOrNull(Parcel.java:3344)
                                                                                                    	at android.os.Parcel.createException(Parcel.java:3328)
                                                                                                    	at android.os.Parcel.readException(Parcel.java:3311)
                                                                                                    	at android.os.Parcel.readException(Parcel.java:3253)
                                                                                                    	at android.accessibilityservice.IAccessibilityServiceConnection$Stub$Proxy.takeScreenshot(IAccessibilityServiceConnection.java:1854)
                                                                                                    	at android.accessibilityservice.AccessibilityService.takeScreenshot(AccessibilityService.java:2716)
                                                                                                    	at com.akslabs.circletosearch.CircleToSearchService.performCapture(CircleToSearchService.kt:76)
                                                                                                    	at com.akslabs.circletosearch.CircleToSearchService.access$performCapture(CircleToSearchService.kt:20)
                                                                                                    	at com.akslabs.circletosearch.CircleToSearchService$setupTriggerOverlay$gestureDetector$1.onDoubleTap(CircleToSearchService.kt:49)
                                                                                                    	at android.view.GestureDetector.onTouchEvent(GestureDetector.java:652)
                                                                                                    	at com.akslabs.circletosearch.CircleToSearchService.setupTriggerOverlay$lambda$0(CircleToSearchService.kt:55)
                                                                                                    	at com.akslabs.circletosearch.CircleToSearchService.$r8$lambda$6NeB463SgkxISw5kz6MKMb42ffc(Unknown Source:0)
                                                                                                    	at com.akslabs.circletosearch.CircleToSearchService$$ExternalSyntheticLambda0.onTouch(D8$$SyntheticClass:0)
                                                                                                    	at android.view.View.performOnTouchCallback(View.java:16635)
                                                                                                    	at android.view.View.dispatchTouchEvent(View.java:16598)
                                                                                                    	at android.view.View.dispatchPointerEvent(View.java:16938)
                                                                                                    	at android.view.ViewRootImpl$ViewPostImeInputStage.processPointerEvent(ViewRootImpl.java:8241)
                                                                                                    	at android.view.ViewRootImpl$ViewPostImeInputStage.onProcess(ViewRootImpl.java:8000)
                                                                                                    	at android.view.ViewRootImpl$InputStage.deliver(ViewRootImpl.java:7378)
                                                                                                    	at android.view.ViewRootImpl$InputStage.onDeliverToNext(ViewRootImpl.java:7435)
                                                                                                    	at android.view.ViewRootImpl$InputStage.forward(ViewRootImpl.java:7401)
                                                                                                    	at android.view.ViewRootImpl$AsyncInputStage.forward(ViewRootImpl.java:7567)
                                                                                                    	at android.view.ViewRootImpl$InputStage.apply(ViewRootImpl.java:7409)
                                                                                                    	at android.view.ViewRootImpl$AsyncInputStage.apply(ViewRootImpl.java:7624)
                                                                                                    	at android.view.ViewRootImpl$InputStage.deliver(ViewRootImpl.java:7382)
                                                                                                    	at android.view.ViewRootImpl$InputStage.onDeliverToNext(ViewRootImpl.java:7435)
                                                                                                    	at android.view.ViewRootImpl$InputStage.forward(ViewRootImpl.java:7401)
                                                                                                    	at android.view.ViewRootImpl$InputStage.apply(ViewRootImpl.java:7409)
                                                                                                    	at android.view.ViewRootImpl$InputStage.deliver(ViewRootImpl.java:7382)
                                                                                                    	at android.view.ViewRootImpl.deliverInputEvent(ViewRootImpl.java:10545)
                                                                                                    	at android.view.ViewRootImpl.doProcessInputEvents(ViewRootImpl.java:10496)
                                                                                                    	at android.view.ViewRootImpl.enqueueInputEvent(ViewRootImpl.java:10464)
                                                                                                    	at android.view.ViewRootImpl.processRawInputEvent(ViewRootImpl.java:10909)
                                                                                                    	at android.view.ViewRootImpl$WindowInputEventReceiver.onInputEvent(ViewRootImpl.java:10667)
                                                                                                    	at android.view.InputEventReceiver.dispatchInputEvent(InputEventReceiver.java:306)
                                                                                                    	at android.os.MessageQueue.nativePollOnce(Native Method)
                                                                                                    	at android.os.MessageQueue.nextLegacy(MessageQueue.java:913)
                                                                                                    	at android.os.MessageQueue.next(MessageQueue.java:1025)
                                                                                                    	at android.os.Looper.loopOnce(Looper.java:196)
                                                                                                    	at android.os.Looper.loop(Looper.java:338)
                                                                                                    	at android.app.ActivityThread.main(ActivityThread.java:9073)
                                                                                                    	at java.lang.reflect.Method.invoke(Native Method)
                                                                                                    	at com.android.internal.os.RuntimeInit$MethodAndArgsCaller.run(RuntimeInit.java:596)
                                                                                                    	at com.android.internal.os.ZygoteInit.main(ZygoteInit.java:932)
                                                                                                    Caused by: android.os.RemoteException: Remote stack trace:
                                                                                                    	at com.android.server.accessibility.AbstractAccessibilityServiceConnection.takeScreenshot(AbstractAccessibilityServiceConnection.java:1529)
                                                                                                    	at android.accessibilityservice.IAccessibilityServiceConnection$Stub.onTransact(IAccessibilityServiceConnection.java:940)
                                                                                                    	at android.os.Binder.execTransactInternal(Binder.java:1421)
                                                                                                    	at android.os.Binder.execTransact(Binder.java:1365)
                                                                                                    
2025-11-25 00:57:53.160 15784-15784 AndroidRuntime          com.akslabs.circletosearch           D  Shutting down VM
2025-11-25 00:57:53.165 15784-15784 AndroidRuntime          com.akslabs.circletosearch           E  FATAL EXCEPTION: main (Fix with AI)
                                                                                                    Process: com.akslabs.circletosearch, PID: 15784
                                                                                                    java.lang.SecurityException: Services don't have the capability of taking the screenshot.
                                                                                                    	at android.os.Parcel.createExceptionOrNull(Parcel.java:3344)
                                                                                                    	at android.os.Parcel.createException(Parcel.java:3328)
                                                                                                    	at android.os.Parcel.readException(Parcel.java:3311)
                                                                                                    	at android.os.Parcel.readException(Parcel.java:3253)
                                                                                                    	at android.accessibilityservice.IAccessibilityServiceConnection$Stub$Proxy.takeScreenshot(IAccessibilityServiceConnection.java:1854)
                                                                                                    	at android.accessibilityservice.AccessibilityService.takeScreenshot(AccessibilityService.java:2716)
                                                                                                    	at com.akslabs.circletosearch.CircleToSearchService.performCapture(CircleToSearchService.kt:76)
                                                                                                    	at com.akslabs.circletosearch.CircleToSearchService.access$performCapture(CircleToSearchService.kt:20)
                                                                                                    	at com.akslabs.circletosearch.CircleToSearchService$setupTriggerOverlay$gestureDetector$1.onDoubleTap(CircleToSearchService.kt:49)
                                                                                                    	at android.view.GestureDetector.onTouchEvent(GestureDetector.java:652)
                                                                                                    	at com.akslabs.circletosearch.CircleToSearchService.setupTriggerOverlay$lambda$0(CircleToSearchService.kt:55)
                                                                                                    	at com.akslabs.circletosearch.CircleToSearchService.$r8$lambda$6NeB463SgkxISw5kz6MKMb42ffc(Unknown Source:0)
                                                                                                    	at com.akslabs.circletosearch.CircleToSearchService$$ExternalSyntheticLambda0.onTouch(D8$$SyntheticClass:0)
                                                                                                    	at android.view.View.performOnTouchCallback(View.java:16635)
                                                                                                    	at android.view.View.dispatchTouchEvent(View.java:16598)
                                                                                                    	at android.view.View.dispatchPointerEvent(View.java:16938)
                                                                                                    	at android.view.ViewRootImpl$ViewPostImeInputStage.processPointerEvent(ViewRootImpl.java:8241)
                                                                                                    	at android.view.ViewRootImpl$ViewPostImeInputStage.onProcess(ViewRootImpl.java:8000)
                                                                                                    	at android.view.ViewRootImpl$InputStage.deliver(ViewRootImpl.java:7378)
                                                                                                    	at android.view.ViewRootImpl$InputStage.onDeliverToNext(ViewRootImpl.java:7435)
                                                                                                    	at android.view.ViewRootImpl$InputStage.forward(ViewRootImpl.java:7401)
                                                                                                    	at android.view.ViewRootImpl$AsyncInputStage.forward(ViewRootImpl.java:7567)
                                                                                                    	at android.view.ViewRootImpl$InputStage.apply(ViewRootImpl.java:7409)
                                                                                                    	at android.view.ViewRootImpl$AsyncInputStage.apply(ViewRootImpl.java:7624)
                                                                                                    	at android.view.ViewRootImpl$InputStage.deliver(ViewRootImpl.java:7382)
                                                                                                    	at android.view.ViewRootImpl$InputStage.onDeliverToNext(ViewRootImpl.java:7435)
                                                                                                    	at android.view.ViewRootImpl$InputStage.forward(ViewRootImpl.java:7401)
                                                                                                    	at android.view.ViewRootImpl$InputStage.apply(ViewRootImpl.java:7409)
                                                                                                    	at android.view.ViewRootImpl$InputStage.deliver(ViewRootImpl.java:7382)
                                                                                                    	at android.view.ViewRootImpl.deliverInputEvent(ViewRootImpl.java:10545)
                                                                                                    	at android.view.ViewRootImpl.doProcessInputEvents(ViewRootImpl.java:10496)
                                                                                                    	at android.view.ViewRootImpl.enqueueInputEvent(ViewRootImpl.java:10464)
                                                                                                    	at android.view.ViewRootImpl.processRawInputEvent(ViewRootImpl.java:10909)
                                                                                                    	at android.view.ViewRootImpl$WindowInputEventReceiver.onInputEvent(ViewRootImpl.java:10667)
                                                                                                    	at android.view.InputEventReceiver.dispatchInputEvent(InputEventReceiver.java:306)
                                                                                                    	at android.os.MessageQueue.nativePollOnce(Native Method)
                                                                                                    	at android.os.MessageQueue.nextLegacy(MessageQueue.java:913)
                                                                                                    	at android.os.MessageQueue.next(MessageQueue.java:1025)
                                                                                                    	at android.os.Looper.loopOnce(Looper.java:196)
                                                                                                    	at android.os.Looper.loop(Looper.java:338)
                                                                                                    	at android.app.ActivityThread.main(ActivityThread.java:9073)
                                                                                                    	at java.lang.reflect.Method.invoke(Native Method)
                                                                                                    	at com.android.internal.os.RuntimeInit$MethodAndArgsCaller.run(RuntimeInit.java:596)
                                                                                                    	at com.android.internal.os.ZygoteInit.main(ZygoteInit.java:932)
                                                                                                    Caused by: android.os.RemoteException: Remote stack trace:
                                                                                                    	at com.android.server.accessibility.AbstractAccessibilityServiceConnection.takeScreenshot(AbstractAccessibilityServiceConnection.java:1529)
                                                                                                    	at android.accessibilityservice.IAccessibilityServiceConnection$Stub.onTransact(IAccessibilityServiceConnection.java:940)
                                                                                                    	at android.os.Binder.execTransactInternal(Binder.java:1421)
                                                                                                    	at android.os.Binder.execTransact(Binder.java:1365)
                                                                                                    
2025-11-25 00:57:58.187 15784-19339 .circletosearch         com.akslabs.circletosearch           I  Thread[2,tid=19339,WaitingInMainSignalCatcherLoop,Thread*=0x78a840b000,peer=0x2f401c8,"Signal Catcher"]: reacting to signal 3
2025-11-25 00:57:58.187 15784-19339 .circletosearch         com.akslabs.circletosearch           I  
2025-11-25 00:57:58.498 15784-19339 .circletosearch         com.akslabs.circletosearch           I  Wrote stack traces to tombstoned
