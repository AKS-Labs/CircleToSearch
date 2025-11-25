2025-11-25 00:56:38.169 13612-13612 AndroidRuntime          pid-13612                            E  FATAL EXCEPTION: main (Fix with AI)
                                                                                                    Process: com.akslabs.circletosearch, PID: 13612
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
                                                                                                    
2025-11-25 00:57:53.165 15784-15784 AndroidRuntime          com.instagram.android                E  FATAL EXCEPTION: main (Fix with AI)
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
                                                                                                    
2025-11-25 01:01:43.136 19332-19332 AndroidRuntime          pid-19332                            E  FATAL EXCEPTION: main (Fix with AI)
                                                                                                    Process: com.akslabs.circletosearch, PID: 19332
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
                                                                                                    
2025-11-25 11:20:29.732 28623-28623 AndroidRuntime          pid-28623                            E  FATAL EXCEPTION: main (Fix with AI)
                                                                                                    Process: com.akslabs.circletosearch, PID: 28623
                                                                                                    java.lang.SecurityException: vibrate: Neither user 10727 nor current process has android.permission.VIBRATE.
                                                                                                    	at android.os.Parcel.createExceptionOrNull(Parcel.java:3344)
                                                                                                    	at android.os.Parcel.createException(Parcel.java:3328)
                                                                                                    	at android.os.Parcel.readException(Parcel.java:3311)
                                                                                                    	at android.os.Parcel.readException(Parcel.java:3253)
                                                                                                    	at android.os.IVibratorManagerService$Stub$Proxy.vibrate(IVibratorManagerService.java:526)
                                                                                                    	at android.os.SystemVibratorManager.vibrate(SystemVibratorManager.java:156)
                                                                                                    	at android.os.SystemVibrator.vibrate(SystemVibrator.java:225)
                                                                                                    	at android.os.Vibrator.vibrate(Vibrator.java:623)
                                                                                                    	at android.os.Vibrator.vibrate(Vibrator.java:603)
                                                                                                    	at android.os.Vibrator.vibrate(Vibrator.java:565)
                                                                                                    	at com.akslabs.circletosearch.CircleToSearchService.performCapture(CircleToSearchService.kt:135)
                                                                                                    	at com.akslabs.circletosearch.CircleToSearchService.access$performCapture(CircleToSearchService.kt:22)
                                                                                                    	at com.akslabs.circletosearch.CircleToSearchService$setupTriggerOverlay$gestureDetector$1.onDoubleTap(CircleToSearchService.kt:113)
                                                                                                    	at android.view.GestureDetector.onTouchEvent(GestureDetector.java:652)
                                                                                                    	at com.akslabs.circletosearch.CircleToSearchService.setupTriggerOverlay$lambda$5(CircleToSearchService.kt:119)
                                                                                                    	at com.akslabs.circletosearch.CircleToSearchService.$r8$lambda$y347tI3LZwxCcjkGTROUVOT0PYs(Unknown Source:0)
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
                                                                                                    	at android.app.ContextImpl.enforce(ContextImpl.java:2440)
                                                                                                    	at android.app.ContextImpl.enforceCallingOrSelfPermission(ContextImpl.java:2468)
2025-11-25 11:20:39.022  1265-1265  AndroidRuntime          pid-1265                             E  FATAL EXCEPTION: main (Fix with AI)
                                                                                                    Process: com.akslabs.circletosearch, PID: 1265
                                                                                                    java.lang.SecurityException: vibrate: Neither user 10727 nor current process has android.permission.VIBRATE.
                                                                                                    	at android.os.Parcel.createExceptionOrNull(Parcel.java:3344)
                                                                                                    	at android.os.Parcel.createException(Parcel.java:3328)
                                                                                                    	at android.os.Parcel.readException(Parcel.java:3311)
                                                                                                    	at android.os.Parcel.readException(Parcel.java:3253)
                                                                                                    	at android.os.IVibratorManagerService$Stub$Proxy.vibrate(IVibratorManagerService.java:526)
                                                                                                    	at android.os.SystemVibratorManager.vibrate(SystemVibratorManager.java:156)
                                                                                                    	at android.os.SystemVibrator.vibrate(SystemVibrator.java:225)
                                                                                                    	at android.os.Vibrator.vibrate(Vibrator.java:623)
                                                                                                    	at android.os.Vibrator.vibrate(Vibrator.java:603)
                                                                                                    	at android.os.Vibrator.vibrate(Vibrator.java:565)
                                                                                                    	at com.akslabs.circletosearch.CircleToSearchService.performCapture(CircleToSearchService.kt:135)
                                                                                                    	at com.akslabs.circletosearch.CircleToSearchService.access$performCapture(CircleToSearchService.kt:22)
                                                                                                    	at com.akslabs.circletosearch.CircleToSearchService$setupTriggerOverlay$gestureDetector$1.onDoubleTap(CircleToSearchService.kt:113)
                                                                                                    	at android.view.GestureDetector.onTouchEvent(GestureDetector.java:652)
                                                                                                    	at com.akslabs.circletosearch.CircleToSearchService.setupTriggerOverlay$lambda$5(CircleToSearchService.kt:119)
                                                                                                    	at com.akslabs.circletosearch.CircleToSearchService.$r8$lambda$y347tI3LZwxCcjkGTROUVOT0PYs(Unknown Source:0)
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
                                                                                                    	at android.app.ContextImpl.enforce(ContextImpl.java:2440)
                                                                                                    	at android.app.ContextImpl.enforceCallingOrSelfPermission(ContextImpl.java:2468)
---------------------------- PROCESS STARTED (11611) for package com.akslabs.circletosearch ----------------------------
2025-11-25 11:21:35.829  2211-2211  AndroidRuntime          pid-2211                             E  FATAL EXCEPTION: main (Fix with AI)
                                                                                                    Process: com.akslabs.circletosearch, PID: 2211
                                                                                                    java.lang.SecurityException: vibrate: Neither user 10727 nor current process has android.permission.VIBRATE.
                                                                                                    	at android.os.Parcel.createExceptionOrNull(Parcel.java:3344)
                                                                                                    	at android.os.Parcel.createException(Parcel.java:3328)
                                                                                                    	at android.os.Parcel.readException(Parcel.java:3311)
                                                                                                    	at android.os.Parcel.readException(Parcel.java:3253)
                                                                                                    	at android.os.IVibratorManagerService$Stub$Proxy.vibrate(IVibratorManagerService.java:526)
                                                                                                    	at android.os.SystemVibratorManager.vibrate(SystemVibratorManager.java:156)
                                                                                                    	at android.os.SystemVibrator.vibrate(SystemVibrator.java:225)
                                                                                                    	at android.os.Vibrator.vibrate(Vibrator.java:623)
                                                                                                    	at android.os.Vibrator.vibrate(Vibrator.java:603)
                                                                                                    	at android.os.Vibrator.vibrate(Vibrator.java:565)
                                                                                                    	at com.akslabs.circletosearch.CircleToSearchService.performCapture(CircleToSearchService.kt:135)
                                                                                                    	at com.akslabs.circletosearch.CircleToSearchService.showBubble$lambda$4$lambda$3(CircleToSearchService.kt:62)
                                                                                                    	at com.akslabs.circletosearch.CircleToSearchService.$r8$lambda$Yf89tfvH_qT0R5YCDfjJkVxeZBQ(Unknown Source:0)
                                                                                                    	at com.akslabs.circletosearch.CircleToSearchService$$ExternalSyntheticLambda1.onClick(D8$$SyntheticClass:0)
                                                                                                    	at android.view.View.performClick(View.java:8083)
                                                                                                    	at android.view.View.performClickInternal(View.java:8060)
                                                                                                    	at android.view.View.-$$Nest$mperformClickInternal(Unknown Source:0)
                                                                                                    	at android.view.View$PerformClick.run(View.java:31549)
                                                                                                    	at android.os.Handler.handleCallback(Handler.java:995)
                                                                                                    	at android.os.Handler.dispatchMessage(Handler.java:103)
                                                                                                    	at android.os.Looper.loopOnce(Looper.java:248)
                                                                                                    	at android.os.Looper.loop(Looper.java:338)
                                                                                                    	at android.app.ActivityThread.main(ActivityThread.java:9073)
                                                                                                    	at java.lang.reflect.Method.invoke(Native Method)
                                                                                                    	at com.android.internal.os.RuntimeInit$MethodAndArgsCaller.run(RuntimeInit.java:596)
                                                                                                    	at com.android.internal.os.ZygoteInit.main(ZygoteInit.java:932)
                                                                                                    Caused by: android.os.RemoteException: Remote stack trace:
                                                                                                    	at android.app.ContextImpl.enforce(ContextImpl.java:2440)
                                                                                                    	at android.app.ContextImpl.enforceCallingOrSelfPermission(ContextImpl.java:2468)
                                                                                                    	at com.android.server.vibrator.VibratorManagerService.vibrateWithPermissionCheck(VibratorManagerService.java:597)
                                                                                                    	at com.android.server.vibrator.VibratorManagerService.vibrate(VibratorManagerService.java:486)
                                                                                                    	at android.os.IVibratorManagerService$Stub.onTransact(IVibratorManagerService.java:279)
                                                                                                    
2025-11-25 11:22:10.029  5015-5015  AndroidRuntime          pid-5015                             E  FATAL EXCEPTION: main (Fix with AI)
                                                                                                    Process: com.akslabs.circletosearch, PID: 5015
                                                                                                    java.lang.SecurityException: vibrate: Neither user 10727 nor current process has android.permission.VIBRATE.
                                                                                                    	at android.os.Parcel.createExceptionOrNull(Parcel.java:3344)
                                                                                                    	at android.os.Parcel.createException(Parcel.java:3328)
                                                                                                    	at android.os.Parcel.readException(Parcel.java:3311)
                                                                                                    	at android.os.Parcel.readException(Parcel.java:3253)
                                                                                                    	at android.os.IVibratorManagerService$Stub$Proxy.vibrate(IVibratorManagerService.java:526)
                                                                                                    	at android.os.SystemVibratorManager.vibrate(SystemVibratorManager.java:156)
                                                                                                    	at android.os.SystemVibrator.vibrate(SystemVibrator.java:225)
                                                                                                    	at android.os.Vibrator.vibrate(Vibrator.java:623)
                                                                                                    	at android.os.Vibrator.vibrate(Vibrator.java:603)
                                                                                                    	at android.os.Vibrator.vibrate(Vibrator.java:565)
                                                                                                    	at com.akslabs.circletosearch.CircleToSearchService.performCapture(CircleToSearchService.kt:135)
                                                                                                    	at com.akslabs.circletosearch.CircleToSearchService.showBubble$lambda$4$lambda$3(CircleToSearchService.kt:62)
                                                                                                    	at com.akslabs.circletosearch.CircleToSearchService.$r8$lambda$Yf89tfvH_qT0R5YCDfjJkVxeZBQ(Unknown Source:0)
                                                                                                    	at com.akslabs.circletosearch.CircleToSearchService$$ExternalSyntheticLambda1.onClick(D8$$SyntheticClass:0)
                                                                                                    	at android.view.View.performClick(View.java:8083)
                                                                                                    	at android.view.View.performClickInternal(View.java:8060)
                                                                                                    	at android.view.View.-$$Nest$mperformClickInternal(Unknown Source:0)
                                                                                                    	at android.view.View$PerformClick.run(View.java:31549)
                                                                                                    	at android.os.Handler.handleCallback(Handler.java:995)
                                                                                                    	at android.os.Handler.dispatchMessage(Handler.java:103)
                                                                                                    	at android.os.Looper.loopOnce(Looper.java:248)
                                                                                                    	at android.os.Looper.loop(Looper.java:338)
                                                                                                    	at android.app.ActivityThread.main(ActivityThread.java:9073)
                                                                                                    	at java.lang.reflect.Method.invoke(Native Method)
                                                                                                    	at com.android.internal.os.RuntimeInit$MethodAndArgsCaller.run(RuntimeInit.java:596)
                                                                                                    	at com.android.internal.os.ZygoteInit.main(ZygoteInit.java:932)
                                                                                                    Caused by: android.os.RemoteException: Remote stack trace:
                                                                                                    	at android.app.ContextImpl.enforce(ContextImpl.java:2440)
                                                                                                    	at android.app.ContextImpl.enforceCallingOrSelfPermission(ContextImpl.java:2468)
                                                                                                    	at com.android.server.vibrator.VibratorManagerService.vibrateWithPermissionCheck(VibratorManagerService.java:597)
                                                                                                    	at com.android.server.vibrator.VibratorManagerService.vibrate(VibratorManagerService.java:486)
                                                                                                    	at android.os.IVibratorManagerService$Stub.onTransact(IVibratorManagerService.java:279)
                                                                                                    
2025-11-25 11:26:23.731  1969-2124  VerityUtils             system_server                        E  Failed to check whether fs-verity is enabled, errno 38: /data/app/~~O8GjuuZK0Z5p3hHs5LwjVg==/com.akslabs.circletosearch-ZQNKEQeYG0-mGHygAuLvQQ==/base.apk
2025-11-25 11:26:24.442  1969-2124  VerityUtils             system_server                        E  Failed to check whether fs-verity is enabled, errno 38: /data/app/~~O8GjuuZK0Z5p3hHs5LwjVg==/com.akslabs.circletosearch-ZQNKEQeYG0-mGHygAuLvQQ==/base.apk
2025-11-25 11:38:30.912 10204-14203 DisplayManager          com.akslabs.circletosearch           I  Choreographer implicitly registered for the refresh rate.
2025-11-25 11:38:49.714 10204-10204 CircleToSearch          com.akslabs.circletosearch           D  OverlayActivity onCreate
2025-11-25 11:38:49.715 10204-10204 CircleToSearch          com.akslabs.circletosearch           D  Bitmap loaded from Repository. Size: 1080x2340
2025-11-25 11:38:49.898 10204-10204 .circletosearch         com.akslabs.circletosearch           I  AssetManager2(0x78ad733528) locale list changing from [] to [en-IN]
2025-11-25 11:38:49.898 10204-10204 WebViewFactory          com.akslabs.circletosearch           I  Loading com.google.android.webview version 142.0.7444.102 (code 744410233)
2025-11-25 11:38:49.900 10204-10204 .circletosearch         com.akslabs.circletosearch           I  AssetManager2(0x78ad733b28) locale list changing from [] to [en-IN]
2025-11-25 11:38:49.902 10204-10204 .circletosearch         com.akslabs.circletosearch           I  AssetManager2(0x78ad733828) locale list changing from [] to [en-IN]
2025-11-25 11:38:49.903 10204-10204 .circletosearch         com.akslabs.circletosearch           I  AssetManager2(0x78ad733e28) locale list changing from [] to [en-IN]
2025-11-25 11:38:49.904 10204-10204 ResourcesManager        com.akslabs.circletosearch           V  The following library key has been added: ResourcesKey{ mHash=aa84934e mResDir=null mSplitDirs=[] mOverlayDirs=[] mLibDirs=[/data/app/~~sjzTljojN_LVeFaP6KzdrA==/com.google.android.webview-bxztEjelqIwm_Zgxcbmy-w==/base.apk,/data/app/~~_Cvt6oKSXHQJX_50gqIKRQ==/com.google.android.trichromelibrary_744410233-Jj5siaXn3ezExguOmkFV_w==/base.apk,/system_ext/framework/androidx.window.extensions.jar] mDisplayId=0 mOverrideConfig=v36 mCompatInfo={420dpi always-compat} mLoaders=[]}
2025-11-25 11:38:49.931 10204-10204 .circletosearch         com.akslabs.circletosearch           W  Failed to find entry 'classes.dex': Entry not found
2025-11-25 11:38:49.933 10204-10204 nativeloader            com.akslabs.circletosearch           D  Configuring clns-10 for other apk /data/app/~~_Cvt6oKSXHQJX_50gqIKRQ==/com.google.android.trichromelibrary_744410233-Jj5siaXn3ezExguOmkFV_w==/base.apk. target_sdk_version=36, uses_libraries=ALL, library_path=/data/app/~~sjzTljojN_LVeFaP6KzdrA==/com.google.android.webview-bxztEjelqIwm_Zgxcbmy-w==/lib/arm64:/data/app/~~sjzTljojN_LVeFaP6KzdrA==/com.google.android.webview-bxztEjelqIwm_Zgxcbmy-w==/base.apk!/lib/arm64-v8a:/data/app/~~_Cvt6oKSXHQJX_50gqIKRQ==/com.google.android.trichromelibrary_744410233-Jj5siaXn3ezExguOmkFV_w==/base.apk!/lib/arm64-v8a, permitted_path=/data:/mnt/expand
2025-11-25 11:38:49.935 10204-10204 ApplicationLoaders      com.akslabs.circletosearch           D  Returning zygote-cached class loader: /system_ext/framework/androidx.window.extensions.jar
2025-11-25 11:38:49.937 10204-10204 .circletosearch         com.akslabs.circletosearch           W  Loading /data/app/~~sjzTljojN_LVeFaP6KzdrA==/com.google.android.webview-bxztEjelqIwm_Zgxcbmy-w==/oat/arm64/base.odex non-executable as it requires an image which we failed to load
2025-11-25 11:38:49.939 10204-10204 nativeloader            com.akslabs.circletosearch           D  Configuring clns-11 for other apk /data/app/~~sjzTljojN_LVeFaP6KzdrA==/com.google.android.webview-bxztEjelqIwm_Zgxcbmy-w==/base.apk. target_sdk_version=36, uses_libraries=, library_path=/data/app/~~sjzTljojN_LVeFaP6KzdrA==/com.google.android.webview-bxztEjelqIwm_Zgxcbmy-w==/lib/arm64:/data/app/~~sjzTljojN_LVeFaP6KzdrA==/com.google.android.webview-bxztEjelqIwm_Zgxcbmy-w==/base.apk!/lib/arm64-v8a:/data/app/~~_Cvt6oKSXHQJX_50gqIKRQ==/com.google.android.trichromelibrary_744410233-Jj5siaXn3ezExguOmkFV_w==/base.apk!/lib/arm64-v8a, permitted_path=/data:/mnt/expand
2025-11-25 11:38:50.010 10204-10204 .circletosearch         com.akslabs.circletosearch           I  AssetManager2(0x78ad734128) locale list changing from [] to [en-IN]
2025-11-25 11:38:50.012 10204-10204 cr_WVCFactoryProvider   com.akslabs.circletosearch           I  version=142.0.7444.102 (744410233) minSdkVersion=29 multiprocess=true packageId=127 splits=<none>
2025-11-25 11:38:50.044 10204-10204 nativeloader            com.akslabs.circletosearch           D  Load /data/app/~~_Cvt6oKSXHQJX_50gqIKRQ==/com.google.android.trichromelibrary_744410233-Jj5siaXn3ezExguOmkFV_w==/base.apk!/lib/arm64-v8a/libmonochrome_64.so using class loader ns clns-11 (caller=/data/app/~~sjzTljojN_LVeFaP6KzdrA==/com.google.android.webview-bxztEjelqIwm_Zgxcbmy-w==/base.apk): ok
2025-11-25 11:38:50.049 10204-10204 nativeloader            com.akslabs.circletosearch           D  Load /system/lib64/libwebviewchromium_plat_support.so using class loader ns clns-11 (caller=/data/app/~~sjzTljojN_LVeFaP6KzdrA==/com.google.android.webview-bxztEjelqIwm_Zgxcbmy-w==/base.apk): ok
2025-11-25 11:38:50.055 10204-10204 ExportedFlags           com.akslabs.circletosearch           E  android.os.flagging.AconfigStorageReadException: ERROR_PACKAGE_NOT_FOUND: package android.webkit cannot be found on the device
2025-11-25 11:38:50.059 10204-14519 chromium                com.akslabs.circletosearch           I  [1125/113850.055618:INFO:android_webview/browser/variations/variations_seed_loader.cc:67] Failed to open file for reading.: No such file or directory (2)
2025-11-25 11:38:50.060 10204-10204 .circletosearch         com.akslabs.circletosearch           I  AssetManager2(0x78ad734728) locale list changing from [] to [en-IN]
2025-11-25 11:38:50.077 10204-10204 cr_LibraryLoader        com.akslabs.circletosearch           I  Successfully loaded native library
2025-11-25 11:38:50.093 10204-10204 cr_CachingUmaRecorder   com.akslabs.circletosearch           I  Flushed 18 samples from 18 histograms, 0 samples were dropped.
2025-11-25 11:38:50.098 10204-10204 cr_CombinedPProvider    com.akslabs.circletosearch           I  #registerProvider() provider:WV.hk@d154a3a isPolicyCacheEnabled:false policyProvidersSize:0
2025-11-25 11:38:50.101 10204-10204 cr_PolicyProvider       com.akslabs.circletosearch           I  #setManagerAndSource() 0
2025-11-25 11:38:50.110 10204-10204 .circletosearch         com.akslabs.circletosearch           I  AssetManager2(0x78ad734a28) locale list changing from [] to [en-IN]
2025-11-25 11:38:50.115 10204-10204 .circletosearch         com.akslabs.circletosearch           I  AssetManager2(0x78ad734d28) locale list changing from [] to [en-IN]
2025-11-25 11:38:50.203 10204-10204 cr_CombinedPProvider    com.akslabs.circletosearch           I  #linkNativeInternal() 1
2025-11-25 11:38:50.206 10204-10204 cr_AppResProvider       com.akslabs.circletosearch           I  #getApplicationRestrictionsFromUserManager() Bundle[EMPTY_PARCEL]
2025-11-25 11:38:50.206 10204-10204 cr_PolicyProvider       com.akslabs.circletosearch           I  #notifySettingsAvailable() 0
2025-11-25 11:38:50.206 10204-10204 cr_CombinedPProvider    com.akslabs.circletosearch           I  #onSettingsAvailable() 0
2025-11-25 11:38:50.206 10204-10204 cr_CombinedPProvider    com.akslabs.circletosearch           I  #flushPolicies()
2025-11-25 11:38:50.374 10204-14544 chromium                com.akslabs.circletosearch           W  [WARNING:net/dns/dns_config_service_android.cc:69] Failed to read DnsConfig.
2025-11-25 11:38:50.424 10204-10204 chromium                com.akslabs.circletosearch           W  [WARNING:android_webview/browser/network_service/net_helpers.cc:143] HTTP Cache size is: 20971520
2025-11-25 11:38:51.461 10204-10204 vulkan                  com.akslabs.circletosearch           D  searching for layers in '/data/app/~~O8GjuuZK0Z5p3hHs5LwjVg==/com.akslabs.circletosearch-ZQNKEQeYG0-mGHygAuLvQQ==/lib/arm64'
2025-11-25 11:38:51.461 10204-10204 vulkan                  com.akslabs.circletosearch           D  searching for layers in '/data/app/~~O8GjuuZK0Z5p3hHs5LwjVg==/com.akslabs.circletosearch-ZQNKEQeYG0-mGHygAuLvQQ==/base.apk!/lib/arm64-v8a'
2025-11-25 11:38:51.465 10204-10204 AdrenoVK-0              com.akslabs.circletosearch           I  ===== BEGIN DUMP OF OVERRIDDEN SETTINGS =====
2025-11-25 11:38:51.465 10204-10204 AdrenoVK-0              com.akslabs.circletosearch           I  ===== END DUMP OF OVERRIDDEN SETTINGS =====
2025-11-25 11:38:51.466 10204-10204 AdrenoVK-0              com.akslabs.circletosearch           I  QUALCOMM build          : 95db91f, Ifbc588260a
                                                                                                    Build Date              : 09/24/20
                                                                                                    Shader Compiler Version : EV031.32.02.01
                                                                                                    Local Branch            : mybrancheafe5b6d-fb5b-f1b0-b904-5cb90179c3e0
                                                                                                    Remote Branch           : quic/gfx-adreno.lnx.1.0.r114-rel
                                                                                                    Remote Branch           : NONE
                                                                                                    Reconstruct Branch      : NOTHING
2025-11-25 11:38:51.466 10204-10204 AdrenoVK-0              com.akslabs.circletosearch           I  Build Config            : S P 10.0.7 AArch64
2025-11-25 11:38:51.466 10204-10204 AdrenoVK-0              com.akslabs.circletosearch           I  Driver Path             : /vendor/lib64/hw/vulkan.adreno.so
2025-11-25 11:38:51.802 10204-14585 CameraManagerGlobal     com.akslabs.circletosearch           I  Connecting to camera service
2025-11-25 11:38:51.813 10204-14585 CameraManagerGlobal     com.akslabs.circletosearch           W  Ignoring status update of camera 0
2025-11-25 11:38:51.813 10204-14585 CameraManagerGlobal     com.akslabs.circletosearch           W  Ignoring status update of camera 0
2025-11-25 11:38:51.816 10204-14165 CameraManagerGlobal     com.akslabs.circletosearch           W  ignore the torch status update of camera: 3
2025-11-25 11:38:51.835 10204-10204 Choreographer           com.akslabs.circletosearch           I  Skipped 125 frames!  The application may be doing too much work on its main thread.
2025-11-25 11:38:51.858 10204-10204 InsetsController        com.akslabs.circletosearch           D  hide(ime(), fromIme=false)
2025-11-25 11:38:51.862 10204-10204 ImeTracker              com.akslabs.circletosearch           I  com.akslabs.circletosearch:3c72e613: onCancelled at PHASE_CLIENT_ALREADY_HIDDEN
2025-11-25 11:38:57.714 10204-14547 cr_media                com.akslabs.circletosearch           W  BLUETOOTH_CONNECT permission is missing.
2025-11-25 11:38:57.716 10204-14547 cr_media                com.akslabs.circletosearch           W  getBluetoothAdapter() requires BLUETOOTH permission
2025-11-25 11:38:57.716 10204-14547 cr_media                com.akslabs.circletosearch           W  registerBluetoothIntentsIfNeeded: Requires BLUETOOTH permission
2025-11-25 11:38:58.185 10204-14558 VideoCapabilities       com.akslabs.circletosearch           W  could not parse integer range: 788-757
2025-11-25 11:38:58.186 10204-14558 VideoCapabilities       com.akslabs.circletosearch           W  could not parse integer range: 484-472
2025-11-25 11:38:58.186 10204-14558 VideoCapabilities       com.akslabs.circletosearch           W  Unrecognized profile/level 0/3 for video/mpeg2
2025-11-25 11:38:58.187 10204-14558 VideoCapabilities       com.akslabs.circletosearch           W  Unrecognized profile/level 0/3 for video/mpeg2
2025-11-25 11:38:58.206 10204-14558 Utils                   com.akslabs.circletosearch           W  could not parse long range '788-757'
2025-11-25 11:38:58.206 10204-14558 Utils                   com.akslabs.circletosearch           W  could not parse long range '484-472'
2025-11-25 11:38:58.208 10204-14558 VideoCapabilities       com.akslabs.circletosearch           W  Unrecognized profile/level 0/3 for video/mpeg2
2025-11-25 11:38:58.212 10204-14558 VideoCapabilities       com.akslabs.circletosearch           W  Unrecognized profile/level 0/3 for video/mpeg2
2025-11-25 11:39:00.167 10204-14161 .circletosearch         com.akslabs.circletosearch           I  Background young concurrent copying GC freed 4443KB AllocSpace bytes, 41(1876KB) LOS objects, 52% free, 5871KB/11MB, paused 715us,60us total 268.820ms
2025-11-25 11:39:33.182 10204-10204 chromium                com.akslabs.circletosearch           I  [INFO:CONSOLE:6] "Permissions policy violation: unload is not allowed in this document.", source: https://www.bing.com/images/search?q=Circle+to+Search+Demo (6)
2025-11-25 11:39:33.182 10204-10204 chromium                com.akslabs.circletosearch           I  [INFO:CONSOLE:6] "Permissions policy violation: unload is not allowed in this document.", source: https://www.bing.com/images/search?q=Circle+to+Search+Demo (6)
2025-11-25 11:39:33.182 10204-10204 chromium                com.akslabs.circletosearch           I  [INFO:CONSOLE:6] "Permissions policy violation: unload is not allowed in this document.", source: https://www.bing.com/images/search?q=Circle+to+Search+Demo (6)
2025-11-25 11:39:33.183 10204-10204 chromium                com.akslabs.circletosearch           I  [INFO:CONSOLE:6] "Permissions policy violation: unload is not allowed in this document.", source: https://www.bing.com/images/search?q=Circle+to+Search+Demo (6)
2025-11-25 11:39:35.254 10204-14547 AudioSystem             com.akslabs.circletosearch           D  onNewServiceWithAdapter: media.audio_flinger service obtained 0x7895d29d20
2025-11-25 11:39:35.254 10204-14547 AudioSystem             com.akslabs.circletosearch           D  getService: checking for service media.audio_flinger: 0x78a8e1f080
2025-11-25 11:39:35.388 10204-14547 AudioSystem             com.akslabs.circletosearch           D  onNewService: media.audio_policy service obtained 0x78ad8e1440
2025-11-25 11:39:35.389 10204-14547 AudioSystem             com.akslabs.circletosearch           D  getService: checking for service media.audio_policy: 0x78ad8e1440
2025-11-25 11:39:36.978 10204-10204 chromium                com.akslabs.circletosearch           I  [INFO:CONSOLE:0] "Error with Permissions-Policy header: Unrecognized feature: 'interest-cohort'.", source:  (0)
2025-11-25 11:39:37.883 10204-10204 chromium                com.akslabs.circletosearch           I  [INFO:CONSOLE:0] "Error with Permissions-Policy header: Unrecognized feature: 'interest-cohort'.", source:  (0)
2025-11-25 11:39:58.554 10204-10204 chromium                com.akslabs.circletosearch           I  [INFO:CONSOLE:6] "Permissions policy violation: unload is not allowed in this document.", source: https://www.bing.com/images/search?q=Circle+to+Search+Demo (6)
2025-11-25 11:39:58.559 10204-10204 chromium                com.akslabs.circletosearch           I  [INFO:CONSOLE:6] "Permissions policy violation: unload is not allowed in this document.", source: https://www.bing.com/images/search?q=Circle+to+Search+Demo (6)
2025-11-25 11:39:58.559 10204-10204 chromium                com.akslabs.circletosearch           I  [INFO:CONSOLE:6] "Permissions policy violation: unload is not allowed in this document.", source: https://www.bing.com/images/search?q=Circle+to+Search+Demo (6)
2025-11-25 11:39:58.560 10204-10204 chromium                com.akslabs.circletosearch           I  [INFO:CONSOLE:6] "Permissions policy violation: unload is not allowed in this document.", source: https://www.bing.com/images/search?q=Circle+to+Search+Demo (6)
2025-11-25 11:40:07.165 10204-10204 InsetsController        com.akslabs.circletosearch           D  hide(ime(), fromIme=false)
2025-11-25 11:40:07.165 10204-10204 ImeTracker              com.akslabs.circletosearch           I  com.akslabs.circletosearch:ef78d380: onCancelled at PHASE_CLIENT_ALREADY_HIDDEN
2025-11-25 11:40:07.292 10204-10204 VRI[OverlayActivity]    com.akslabs.circletosearch           D  visibilityChanged oldVisibility=true newVisibility=false
2025-11-25 11:40:07.310 10204-14203 HWUI                    com.akslabs.circletosearch           D  endAllActiveAnimators on 0x78a8f6c400 (UnprojectedRipple) with handle 0x78a8404720
2025-11-25 11:40:07.364 10204-10204 WindowOnBackDispatcher  com.akslabs.circletosearch           W  sendCancelIfRunning: isInProgress=false callback=android.app.Activity$$ExternalSyntheticLambda0@d238837
2025-11-25 11:40:07.370 10204-10204 ViewRootImpl            com.akslabs.circletosearch           D  Skipping stats log for color mode
2025-11-25 11:40:08.966 10204-10204 .circletosearch         com.akslabs.circletosearch           I  AssetManager2(0x78ad800428) locale list changing from [] to [en-IN]
2025-11-25 11:40:08.971 10204-10204 CircleToSearch          com.akslabs.circletosearch           D  OverlayActivity onCreate
2025-11-25 11:40:08.972 10204-10204 CircleToSearch          com.akslabs.circletosearch           D  Bitmap loaded from Repository. Size: 1080x2340
2025-11-25 11:40:08.975 10204-10204 .circletosearch         com.akslabs.circletosearch           I  AssetManager2(0x78ad800a28) locale list changing from [] to [en-IN]
2025-11-25 11:40:09.285 10204-10204 InsetsController        com.akslabs.circletosearch           D  hide(ime(), fromIme=false)
2025-11-25 11:40:09.285 10204-10204 ImeTracker              com.akslabs.circletosearch           I  com.akslabs.circletosearch:335d2748: onCancelled at PHASE_CLIENT_ALREADY_HIDDEN
2025-11-25 11:40:26.594 10204-14554 chromium                com.akslabs.circletosearch           W  [WARNING:net/spdy/spdy_session.cc:3071] Received HEADERS for invalid stream 1
2025-11-25 11:40:29.188 10204-10204 ImeTracker              com.akslabs.circletosearch           I  com.akslabs.circletosearch:8c0d0d94: onRequestShow at ORIGIN_CLIENT reason SHOW_SOFT_INPUT fromUser false
2025-11-25 11:40:29.189 10204-10204 InsetsController        com.akslabs.circletosearch           D  show(ime(), fromIme=false)
2025-11-25 11:40:29.190 10204-10204 InsetsController        com.akslabs.circletosearch           D  Setting requestedVisibleTypes to -1 (was -9)
2025-11-25 11:40:29.239 10204-10204 AssistStructure         com.akslabs.circletosearch           I  Flattened final assist data: 1772 bytes, containing 1 windows, 4 views
2025-11-25 11:40:30.178 10204-10204 InteractionJankMonitor  com.akslabs.circletosearch           W  Initializing without READ_DEVICE_CONFIG permission. enabled=false, interval=1, missedFrameThreshold=3, frameTimeThreshold=64, package=com.akslabs.circletosearch
2025-11-25 11:40:30.306 10204-10204 ImeTracker              com.akslabs.circletosearch           I  system_server:c9f465fc: onShown
2025-11-25 11:40:30.867 10204-10204 InsetsController        com.akslabs.circletosearch           D  hide(ime(), fromIme=false)
2025-11-25 11:40:30.868 10204-10204 WindowOnBackDispatcher  com.akslabs.circletosearch           W  sendCancelIfRunning: isInProgress=false callback=ImeCallback=ImeOnBackInvokedCallback@148091419 Callback=android.window.IOnBackInvokedCallback$Stub$Proxy@bf9f0de
2025-11-25 11:40:30.868 10204-10204 InsetsController        com.akslabs.circletosearch           D  Setting requestedVisibleTypes to -9 (was -1)
2025-11-25 11:40:30.870 10204-10204 InsetsController        com.akslabs.circletosearch           D  hide(ime(), fromIme=false)
2025-11-25 11:40:30.870 10204-10204 ImeTracker              com.akslabs.circletosearch           I  helium314.keyboard:7074e2e5: onCancelled at PHASE_CLIENT_ALREADY_HIDDEN
2025-11-25 11:40:31.058 10204-10204 ImeTracker              com.akslabs.circletosearch           I  system_server:a6cc4c04: onCancelled at PHASE_CLIENT_ON_CONTROLS_CHANGED
2025-11-25 11:40:35.644 10204-10204 ImeTracker              com.akslabs.circletosearch           I  com.akslabs.circletosearch:1f01a035: onRequestHide at ORIGIN_CLIENT reason HIDE_SOFT_INPUT fromUser false
2025-11-25 11:40:35.644 10204-10204 InsetsController        com.akslabs.circletosearch           D  hide(ime(), fromIme=false)
2025-11-25 11:40:35.644 10204-10204 ImeTracker              com.akslabs.circletosearch           I  com.akslabs.circletosearch:1f01a035: onCancelled at PHASE_CLIENT_ALREADY_HIDDEN
2025-11-25 11:40:35.645 10204-10204 CompatChangeReporter    com.akslabs.circletosearch           D  Compat change id reported: 395521150; UID 10727; state: ENABLED
2025-11-25 11:40:41.431 10204-10204 InsetsController        com.akslabs.circletosearch           D  hide(ime(), fromIme=false)
2025-11-25 11:40:41.432 10204-10204 ImeTracker              com.akslabs.circletosearch           I  com.akslabs.circletosearch:fff6ec03: onCancelled at PHASE_CLIENT_ALREADY_HIDDEN
2025-11-25 11:40:41.542 10204-10204 VRI[OverlayActivity]    com.akslabs.circletosearch           D  visibilityChanged oldVisibility=true newVisibility=false
2025-11-25 11:40:41.564 10204-14203 HWUI                    com.akslabs.circletosearch           D  endAllActiveAnimators on 0x783e76e000 (UnprojectedRipple) with handle 0x78a8488520
2025-11-25 11:40:41.573 10204-10204 AutofillManager         com.akslabs.circletosearch           I  onInvisibleForAutofill(): expiringResponse
2025-11-25 11:40:41.612 10204-10204 WindowOnBackDispatcher  com.akslabs.circletosearch           W  sendCancelIfRunning: isInProgress=false callback=android.app.Activity$$ExternalSyntheticLambda0@e0c43c0
2025-11-25 11:40:41.616 10204-10204 ViewRootImpl            com.akslabs.circletosearch           D  Skipping stats log for color mode
2025-11-25 11:40:46.395 10204-10204 CircleToSearch          com.akslabs.circletosearch           D  OverlayActivity onCreate
2025-11-25 11:40:46.395 10204-10204 CircleToSearch          com.akslabs.circletosearch           D  Bitmap loaded from Repository. Size: 1080x2340
2025-11-25 11:40:46.397 10204-10204 .circletosearch         com.akslabs.circletosearch           I  AssetManager2(0x78a8e31228) locale list changing from [] to [en-IN]
2025-11-25 11:40:46.489 10204-10204 .circletosearch         com.akslabs.circletosearch           I  AssetManager2(0x78a8e31528) locale list changing from [] to [en-IN]
2025-11-25 11:40:46.767 10204-10204 InsetsController        com.akslabs.circletosearch           D  hide(ime(), fromIme=false)
2025-11-25 11:40:46.768 10204-10204 ImeTracker              com.akslabs.circletosearch           I  com.akslabs.circletosearch:f389a06d: onCancelled at PHASE_CLIENT_ALREADY_HIDDEN
2025-11-25 11:41:03.658  1969-2279  InputDispatcher         system_server                        E  channel '3998a48 com.akslabs.circletosearch/com.akslabs.circletosearch.OverlayActivity' ~ Channel is unrecoverably broken and will be disposed!
2025-11-25 11:41:03.660  1969-2279  InputDispatcher         system_server                        E  channel 'e64845e com.akslabs.circletosearch/com.akslabs.circletosearch.MainActivity' ~ Channel is unrecoverably broken and will be disposed!
2025-11-25 11:41:09.002 11611-11611 .circletosearch         com.akslabs.circletosearch           I  Late-enabling -Xcheck:jni
2025-11-25 11:41:09.072 11611-11611 .circletosearch         com.akslabs.circletosearch           I  Using CollectorTypeCC GC.
2025-11-25 11:41:09.236 11611-11611 .circletosearch         com.akslabs.circletosearch           W  Thread Pool max thread count is 0. Cannot cache binder as linkToDeath cannot be implemented. serviceName: activity
2025-11-25 11:41:09.431 11611-11611 nativeloader            com.akslabs.circletosearch           D  Load libframework-connectivity-tiramisu-jni.so using APEX ns com_android_tethering for caller /apex/com.android.tethering/javalib/framework-connectivity-t.jar: ok
2025-11-25 11:41:09.533 11611-11611 re-initialized>         com.akslabs.circletosearch           W  type=1400 audit(0.0:34501): avc:  granted  { execute } for  path="/data/data/com.akslabs.circletosearch/code_cache/startup_agents/43ec9884-agent.so" dev="mmcblk0p61" ino=843940 scontext=u:r:untrusted_app:s0:c215,c258,c512,c768 tcontext=u:object_r:app_data_file:s0:c215,c258,c512,c768 tclass=file app=com.akslabs.circletosearch
2025-11-25 11:41:09.553 11611-11611 nativeloader            com.akslabs.circletosearch           D  Load /data/user/0/com.akslabs.circletosearch/code_cache/startup_agents/43ec9884-agent.so using system ns (caller=<unknown>): ok
2025-11-25 11:41:09.564 11611-11611 .circletosearch         com.akslabs.circletosearch           W  hiddenapi: DexFile /data/data/com.akslabs.circletosearch/code_cache/.studio/instruments-aba82530.jar is in boot class path but is not in a known location
2025-11-25 11:41:09.947 11611-11611 .circletosearch         com.akslabs.circletosearch           W  Redefining intrinsic method java.lang.Thread java.lang.Thread.currentThread(). This may cause the unexpected use of the original definition of java.lang.Thread java.lang.Thread.currentThread()in methods that have already been compiled.
2025-11-25 11:41:09.947 11611-11611 .circletosearch         com.akslabs.circletosearch           W  Redefining intrinsic method boolean java.lang.Thread.interrupted(). This may cause the unexpected use of the original definition of boolean java.lang.Thread.interrupted()in methods that have already been compiled.
2025-11-25 11:41:10.869 11611-11611 nativeloader            com.akslabs.circletosearch           D  Configuring clns-9 for other apk /data/app/~~O8GjuuZK0Z5p3hHs5LwjVg==/com.akslabs.circletosearch-ZQNKEQeYG0-mGHygAuLvQQ==/base.apk. target_sdk_version=36, uses_libraries=, library_path=/data/app/~~O8GjuuZK0Z5p3hHs5LwjVg==/com.akslabs.circletosearch-ZQNKEQeYG0-mGHygAuLvQQ==/lib/arm64:/data/app/~~O8GjuuZK0Z5p3hHs5LwjVg==/com.akslabs.circletosearch-ZQNKEQeYG0-mGHygAuLvQQ==/base.apk!/lib/arm64-v8a, permitted_path=/data:/mnt/expand:/data/user/0/com.akslabs.circletosearch
2025-11-25 11:41:10.889 11611-11611 .circletosearch         com.akslabs.circletosearch           I  AssetManager2(0x789a078b28) locale list changing from [] to [en-IN]
2025-11-25 11:41:10.894 11611-11611 .circletosearch         com.akslabs.circletosearch           I  AssetManager2(0x789a079428) locale list changing from [] to [en-IN]
2025-11-25 11:41:10.904 11611-11611 GraphicsEnvironment     com.akslabs.circletosearch           V  Currently set values for:
2025-11-25 11:41:10.904 11611-11611 GraphicsEnvironment     com.akslabs.circletosearch           V    angle_gl_driver_selection_pkgs=[com.android.angle, com.linecorp.b612.android, com.campmobile.snow, com.google.android.apps.tachyon]
2025-11-25 11:41:10.905 11611-11611 GraphicsEnvironment     com.akslabs.circletosearch           V    angle_gl_driver_selection_values=[angle, native, native, native]
2025-11-25 11:41:10.905 11611-11611 GraphicsEnvironment     com.akslabs.circletosearch           V  com.akslabs.circletosearch is not listed in per-application setting
2025-11-25 11:41:10.905 11611-11611 GraphicsEnvironment     com.akslabs.circletosearch           V  ANGLE allowlist from config: 
2025-11-25 11:41:10.905 11611-11611 GraphicsEnvironment     com.akslabs.circletosearch           V  com.akslabs.circletosearch is not listed in ANGLE allowlist or settings, returning default
2025-11-25 11:41:10.906 11611-11611 GraphicsEnvironment     com.akslabs.circletosearch           V  Neither updatable production driver nor prerelease driver is supported.
2025-11-25 11:41:11.053 11611-11611 .circletosearch         com.akslabs.circletosearch           I  AssetManager2(0x789a079728) locale list changing from [] to [en-IN]
2025-11-25 11:41:11.084 11611-16114 DisplayManager          com.akslabs.circletosearch           I  Choreographer implicitly registered for the refresh rate.
2025-11-25 11:41:11.088 11611-16114 AdrenoGLES-0            com.akslabs.circletosearch           I  QUALCOMM build                   : 95db91f, Ifbc588260a
                                                                                                    Build Date                       : 09/24/20
                                                                                                    OpenGL ES Shader Compiler Version: EV031.32.02.01
                                                                                                    Local Branch                     : mybrancheafe5b6d-fb5b-f1b0-b904-5cb90179c3e0
                                                                                                    Remote Branch                    : quic/gfx-adreno.lnx.1.0.r114-rel
                                                                                                    Remote Branch                    : NONE
                                                                                                    Reconstruct Branch               : NOTHING
2025-11-25 11:41:11.088 11611-16114 AdrenoGLES-0            com.akslabs.circletosearch           I  Build Config                     : S P 10.0.7 AArch64
2025-11-25 11:41:11.088 11611-16114 AdrenoGLES-0            com.akslabs.circletosearch           I  Driver Path                      : /vendor/lib64/egl/libGLESv2_adreno.so
2025-11-25 11:41:11.095 11611-16114 AdrenoGLES-0            com.akslabs.circletosearch           I  PFP: 0x016ee190, ME: 0x00000000
2025-11-25 11:41:11.102 11611-11611 ashmem                  com.akslabs.circletosearch           E  Pinning is deprecated since Android Q. Please use trim or other methods.
2025-11-25 11:41:11.162 11611-11611 CompatChangeReporter    com.akslabs.circletosearch           D  Compat change id reported: 377864165; UID 10727; state: ENABLED
2025-11-25 11:41:11.167 11611-11611 DesktopModeFlags        com.akslabs.circletosearch           D  Toggle override initialized to: OVERRIDE_UNSET
2025-11-25 11:41:11.293 11611-16125 Gralloc4                com.akslabs.circletosearch           I  mapper 4.x is not supported
2025-11-25 11:41:11.294 11611-16125 Gralloc3                com.akslabs.circletosearch           W  mapper 3.x is not supported
2025-11-25 11:41:11.637 11611-11611 .circletosearch         com.akslabs.circletosearch           I  hiddenapi: Accessing hidden method Landroid/os/SystemProperties;->addChangeCallback(Ljava/lang/Runnable;)V (runtime_flags=0, domain=platform, api=unsupported) from Landroidx/compose/ui/platform/AndroidComposeView$Companion; (domain=app) using reflection: allowed
2025-11-25 11:41:12.269 11611-16134 Gralloc2                com.akslabs.circletosearch           I  Adding additional valid usage bits: 0x8202000
2025-11-25 11:41:12.605 11611-11611 Choreographer           com.akslabs.circletosearch           I  Skipped 70 frames!  The application may be doing too much work on its main thread.
2025-11-25 11:41:12.658 11611-11611 InsetsController        com.akslabs.circletosearch           D  hide(ime(), fromIme=false)
2025-11-25 11:41:12.658 11611-11611 ImeTracker              com.akslabs.circletosearch           I  com.akslabs.circletosearch:d046e79: onCancelled at PHASE_CLIENT_ALREADY_HIDDEN
2025-11-25 11:41:16.057 11611-16167 ProfileInstaller        com.akslabs.circletosearch           D  Installing profile for com.akslabs.circletosearch
2025-11-25 11:41:20.179 11611-11611 VRI[MainActivity]       com.akslabs.circletosearch           D  visibilityChanged oldVisibility=true newVisibility=false
2025-11-25 11:41:26.948 11611-11611 ViewRootImpl            com.akslabs.circletosearch           D  Skipping stats log for color mode
2025-11-25 11:41:27.134 11611-11611 InsetsController        com.akslabs.circletosearch           D  hide(ime(), fromIme=false)
2025-11-25 11:41:27.134 11611-11611 ImeTracker              com.akslabs.circletosearch           I  com.akslabs.circletosearch:6786164f: onCancelled at PHASE_CLIENT_ALREADY_HIDDEN
2025-11-25 11:41:29.006 11611-11611 CircleToSearch          com.akslabs.circletosearch           D  OverlayActivity onCreate
2025-11-25 11:41:29.006 11611-11611 CircleToSearch          com.akslabs.circletosearch           D  Bitmap loaded from Repository. Size: 1080x2340
2025-11-25 11:41:29.191 11611-11611 .circletosearch         com.akslabs.circletosearch           I  AssetManager2(0x783a69c528) locale list changing from [] to [en-IN]
2025-11-25 11:41:29.191 11611-11611 WebViewFactory          com.akslabs.circletosearch           I  Loading com.google.android.webview version 142.0.7444.102 (code 744410233)
2025-11-25 11:41:29.193 11611-11611 .circletosearch         com.akslabs.circletosearch           I  AssetManager2(0x783a69cb28) locale list changing from [] to [en-IN]
2025-11-25 11:41:29.194 11611-11611 .circletosearch         com.akslabs.circletosearch           I  AssetManager2(0x783a69c828) locale list changing from [] to [en-IN]
2025-11-25 11:41:29.196 11611-11611 .circletosearch         com.akslabs.circletosearch           I  AssetManager2(0x783a69ce28) locale list changing from [] to [en-IN]
2025-11-25 11:41:29.196 11611-11611 ResourcesManager        com.akslabs.circletosearch           V  The following library key has been added: ResourcesKey{ mHash=aa84934e mResDir=null mSplitDirs=[] mOverlayDirs=[] mLibDirs=[/data/app/~~sjzTljojN_LVeFaP6KzdrA==/com.google.android.webview-bxztEjelqIwm_Zgxcbmy-w==/base.apk,/data/app/~~_Cvt6oKSXHQJX_50gqIKRQ==/com.google.android.trichromelibrary_744410233-Jj5siaXn3ezExguOmkFV_w==/base.apk,/system_ext/framework/androidx.window.extensions.jar] mDisplayId=0 mOverrideConfig=v36 mCompatInfo={420dpi always-compat} mLoaders=[]}
2025-11-25 11:41:29.199 11611-11611 .circletosearch         com.akslabs.circletosearch           W  Failed to find entry 'classes.dex': Entry not found
2025-11-25 11:41:29.201 11611-11611 nativeloader            com.akslabs.circletosearch           D  Configuring clns-10 for other apk /data/app/~~_Cvt6oKSXHQJX_50gqIKRQ==/com.google.android.trichromelibrary_744410233-Jj5siaXn3ezExguOmkFV_w==/base.apk. target_sdk_version=36, uses_libraries=ALL, library_path=/data/app/~~sjzTljojN_LVeFaP6KzdrA==/com.google.android.webview-bxztEjelqIwm_Zgxcbmy-w==/lib/arm64:/data/app/~~sjzTljojN_LVeFaP6KzdrA==/com.google.android.webview-bxztEjelqIwm_Zgxcbmy-w==/base.apk!/lib/arm64-v8a:/data/app/~~_Cvt6oKSXHQJX_50gqIKRQ==/com.google.android.trichromelibrary_744410233-Jj5siaXn3ezExguOmkFV_w==/base.apk!/lib/arm64-v8a, permitted_path=/data:/mnt/expand
2025-11-25 11:41:29.202 11611-11611 ApplicationLoaders      com.akslabs.circletosearch           D  Returning zygote-cached class loader: /system_ext/framework/androidx.window.extensions.jar
2025-11-25 11:41:29.217 11611-11611 .circletosearch         com.akslabs.circletosearch           W  Loading /data/app/~~sjzTljojN_LVeFaP6KzdrA==/com.google.android.webview-bxztEjelqIwm_Zgxcbmy-w==/oat/arm64/base.odex non-executable as it requires an image which we failed to load
2025-11-25 11:41:29.220 11611-11611 nativeloader            com.akslabs.circletosearch           D  Configuring clns-11 for other apk /data/app/~~sjzTljojN_LVeFaP6KzdrA==/com.google.android.webview-bxztEjelqIwm_Zgxcbmy-w==/base.apk. target_sdk_version=36, uses_libraries=, library_path=/data/app/~~sjzTljojN_LVeFaP6KzdrA==/com.google.android.webview-bxztEjelqIwm_Zgxcbmy-w==/lib/arm64:/data/app/~~sjzTljojN_LVeFaP6KzdrA==/com.google.android.webview-bxztEjelqIwm_Zgxcbmy-w==/base.apk!/lib/arm64-v8a:/data/app/~~_Cvt6oKSXHQJX_50gqIKRQ==/com.google.android.trichromelibrary_744410233-Jj5siaXn3ezExguOmkFV_w==/base.apk!/lib/arm64-v8a, permitted_path=/data:/mnt/expand
2025-11-25 11:41:29.295 11611-11611 .circletosearch         com.akslabs.circletosearch           I  AssetManager2(0x783a69d128) locale list changing from [] to [en-IN]
2025-11-25 11:41:29.299 11611-11611 cr_WVCFactoryProvider   com.akslabs.circletosearch           I  version=142.0.7444.102 (744410233) minSdkVersion=29 multiprocess=true packageId=127 splits=<none>
2025-11-25 11:41:29.333 11611-11611 nativeloader            com.akslabs.circletosearch           D  Load /data/app/~~_Cvt6oKSXHQJX_50gqIKRQ==/com.google.android.trichromelibrary_744410233-Jj5siaXn3ezExguOmkFV_w==/base.apk!/lib/arm64-v8a/libmonochrome_64.so using class loader ns clns-11 (caller=/data/app/~~sjzTljojN_LVeFaP6KzdrA==/com.google.android.webview-bxztEjelqIwm_Zgxcbmy-w==/base.apk): ok
2025-11-25 11:41:29.344 11611-11611 nativeloader            com.akslabs.circletosearch           D  Load /system/lib64/libwebviewchromium_plat_support.so using class loader ns clns-11 (caller=/data/app/~~sjzTljojN_LVeFaP6KzdrA==/com.google.android.webview-bxztEjelqIwm_Zgxcbmy-w==/base.apk): ok
2025-11-25 11:41:29.396 11611-11611 ExportedFlags           com.akslabs.circletosearch           E  android.os.flagging.AconfigStorageReadException: ERROR_PACKAGE_NOT_FOUND: package android.webkit cannot be found on the device
2025-11-25 11:41:29.405 11611-11611 .circletosearch         com.akslabs.circletosearch           I  AssetManager2(0x783a69d728) locale list changing from [] to [en-IN]
2025-11-25 11:41:29.406 11611-16266 chromium                com.akslabs.circletosearch           I  [1125/114129.399709:INFO:android_webview/browser/variations/variations_seed_loader.cc:67] Failed to open file for reading.: No such file or directory (2)
2025-11-25 11:41:29.459 11611-11611 cr_LibraryLoader        com.akslabs.circletosearch           I  Successfully loaded native library
2025-11-25 11:41:29.468 11611-11611 cr_CachingUmaRecorder   com.akslabs.circletosearch           I  Flushed 18 samples from 18 histograms, 0 samples were dropped.
2025-11-25 11:41:29.498 11611-11611 cr_VariationsSeedLoader com.akslabs.circletosearch           E  Failed loading variations seed. Variations disabled.
2025-11-25 11:41:29.500 11611-11611 cr_CombinedPProvider    com.akslabs.circletosearch           I  #registerProvider() provider:WV.hk@d154a3a isPolicyCacheEnabled:false policyProvidersSize:0
2025-11-25 11:41:29.503 11611-11611 cr_PolicyProvider       com.akslabs.circletosearch           I  #setManagerAndSource() 0
2025-11-25 11:41:29.523 11611-11611 .circletosearch         com.akslabs.circletosearch           I  AssetManager2(0x783a69da28) locale list changing from [] to [en-IN]
2025-11-25 11:41:29.526 11611-11611 .circletosearch         com.akslabs.circletosearch           I  AssetManager2(0x783a69dd28) locale list changing from [] to [en-IN]
2025-11-25 11:41:29.611 11611-11611 cr_CombinedPProvider    com.akslabs.circletosearch           I  #linkNativeInternal() 1
2025-11-25 11:41:29.614 11611-11611 cr_AppResProvider       com.akslabs.circletosearch           I  #getApplicationRestrictionsFromUserManager() Bundle[EMPTY_PARCEL]
2025-11-25 11:41:29.614 11611-11611 cr_PolicyProvider       com.akslabs.circletosearch           I  #notifySettingsAvailable() 0
2025-11-25 11:41:29.614 11611-11611 cr_CombinedPProvider    com.akslabs.circletosearch           I  #onSettingsAvailable() 0
2025-11-25 11:41:29.614 11611-11611 cr_CombinedPProvider    com.akslabs.circletosearch           I  #flushPolicies()
2025-11-25 11:41:30.151 11611-16292 chromium                com.akslabs.circletosearch           W  [WARNING:net/dns/dns_config_service_android.cc:69] Failed to read DnsConfig.
2025-11-25 11:41:30.192 11611-11611 chromium                com.akslabs.circletosearch           W  [WARNING:android_webview/browser/network_service/net_helpers.cc:143] HTTP Cache size is: 20971520
2025-11-25 11:41:30.832 11611-11611 vulkan                  com.akslabs.circletosearch           D  searching for layers in '/data/app/~~O8GjuuZK0Z5p3hHs5LwjVg==/com.akslabs.circletosearch-ZQNKEQeYG0-mGHygAuLvQQ==/lib/arm64'
2025-11-25 11:41:30.832 11611-11611 vulkan                  com.akslabs.circletosearch           D  searching for layers in '/data/app/~~O8GjuuZK0Z5p3hHs5LwjVg==/com.akslabs.circletosearch-ZQNKEQeYG0-mGHygAuLvQQ==/base.apk!/lib/arm64-v8a'
2025-11-25 11:41:30.836 11611-11611 AdrenoVK-0              com.akslabs.circletosearch           I  ===== BEGIN DUMP OF OVERRIDDEN SETTINGS =====
2025-11-25 11:41:30.836 11611-11611 AdrenoVK-0              com.akslabs.circletosearch           I  ===== END DUMP OF OVERRIDDEN SETTINGS =====
2025-11-25 11:41:30.837 11611-11611 AdrenoVK-0              com.akslabs.circletosearch           I  QUALCOMM build          : 95db91f, Ifbc588260a
                                                                                                    Build Date              : 09/24/20
                                                                                                    Shader Compiler Version : EV031.32.02.01
                                                                                                    Local Branch            : mybrancheafe5b6d-fb5b-f1b0-b904-5cb90179c3e0
                                                                                                    Remote Branch           : quic/gfx-adreno.lnx.1.0.r114-rel
                                                                                                    Remote Branch           : NONE
                                                                                                    Reconstruct Branch      : NOTHING
2025-11-25 11:41:30.837 11611-11611 AdrenoVK-0              com.akslabs.circletosearch           I  Build Config            : S P 10.0.7 AArch64
2025-11-25 11:41:30.837 11611-11611 AdrenoVK-0              com.akslabs.circletosearch           I  Driver Path             : /vendor/lib64/hw/vulkan.adreno.so
2025-11-25 11:41:30.913 11611-16341 CameraManagerGlobal     com.akslabs.circletosearch           I  Connecting to camera service
2025-11-25 11:41:30.927 11611-16341 CameraManagerGlobal     com.akslabs.circletosearch           W  Ignoring status update of camera 0
2025-11-25 11:41:30.928 11611-16341 CameraManagerGlobal     com.akslabs.circletosearch           W  Ignoring status update of camera 0
2025-11-25 11:41:30.930 11611-16152 CameraManagerGlobal     com.akslabs.circletosearch           W  ignore the torch status update of camera: 3
2025-11-25 11:41:31.044 11611-11611 Choreographer           com.akslabs.circletosearch           I  Skipped 119 frames!  The application may be doing too much work on its main thread.
2025-11-25 11:41:31.075 11611-11611 InsetsController        com.akslabs.circletosearch           D  hide(ime(), fromIme=false)
2025-11-25 11:41:31.075 11611-11611 ImeTracker              com.akslabs.circletosearch           I  com.akslabs.circletosearch:e1c3a949: onCancelled at PHASE_CLIENT_ALREADY_HIDDEN
2025-11-25 11:41:37.094 11611-16300 cr_media                com.akslabs.circletosearch           W  BLUETOOTH_CONNECT permission is missing.
2025-11-25 11:41:37.097 11611-16300 cr_media                com.akslabs.circletosearch           W  getBluetoothAdapter() requires BLUETOOTH permission
2025-11-25 11:41:37.097 11611-16300 cr_media                com.akslabs.circletosearch           W  registerBluetoothIntentsIfNeeded: Requires BLUETOOTH permission
2025-11-25 11:41:37.608 11611-16309 VideoCapabilities       com.akslabs.circletosearch           W  could not parse integer range: 788-757
2025-11-25 11:41:37.608 11611-16309 VideoCapabilities       com.akslabs.circletosearch           W  could not parse integer range: 484-472
2025-11-25 11:41:37.609 11611-16309 VideoCapabilities       com.akslabs.circletosearch           W  Unrecognized profile/level 0/3 for video/mpeg2
2025-11-25 11:41:37.610 11611-16309 VideoCapabilities       com.akslabs.circletosearch           W  Unrecognized profile/level 0/3 for video/mpeg2
2025-11-25 11:41:37.657 11611-16309 Utils                   com.akslabs.circletosearch           W  could not parse long range '788-757'
2025-11-25 11:41:37.657 11611-16309 Utils                   com.akslabs.circletosearch           W  could not parse long range '484-472'
2025-11-25 11:41:37.660 11611-16309 VideoCapabilities       com.akslabs.circletosearch           W  Unrecognized profile/level 0/3 for video/mpeg2
2025-11-25 11:41:37.661 11611-16309 VideoCapabilities       com.akslabs.circletosearch           W  Unrecognized profile/level 0/3 for video/mpeg2
2025-11-25 11:41:49.765 11611-11611 chromium                com.akslabs.circletosearch           I  [INFO:CONSOLE:6] "Permissions policy violation: unload is not allowed in this document.", source: https://www.bing.com/images/search?q=Circle+to+Search+Demo (6)
2025-11-25 11:41:49.767 11611-11611 chromium                com.akslabs.circletosearch           I  [INFO:CONSOLE:6] "Permissions policy violation: unload is not allowed in this document.", source: https://www.bing.com/images/search?q=Circle+to+Search+Demo (6)
2025-11-25 11:41:49.769 11611-11611 chromium                com.akslabs.circletosearch           I  [INFO:CONSOLE:6] "Permissions policy violation: unload is not allowed in this document.", source: https://www.bing.com/images/search?q=Circle+to+Search+Demo (6)
2025-11-25 11:41:49.772 11611-11611 chromium                com.akslabs.circletosearch           I  [INFO:CONSOLE:6] "Permissions policy violation: unload is not allowed in this document.", source: https://www.bing.com/images/search?q=Circle+to+Search+Demo (6)
2025-11-25 11:41:51.683 11611-16300 AudioSystem             com.akslabs.circletosearch           D  onNewServiceWithAdapter: media.audio_flinger service obtained 0x782c7fca20
2025-11-25 11:41:51.689 11611-16300 AudioSystem             com.akslabs.circletosearch           D  getService: checking for service media.audio_flinger: 0x781e03fe80
2025-11-25 11:41:51.697 11611-16300 AudioSystem             com.akslabs.circletosearch           D  onNewService: media.audio_policy service obtained 0x782c7fd240
2025-11-25 11:41:51.698 11611-16300 AudioSystem             com.akslabs.circletosearch           D  getService: checking for service media.audio_policy: 0x782c7fd240
