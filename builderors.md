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
                                                                                                    
2025-11-25 00:57:53.165 15784-15784 AndroidRuntime          pid-15784                            E  FATAL EXCEPTION: main (Fix with AI)
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
                                                                                                    
2025-11-25 16:39:42.228  1969-2219  ClipboardService        system_server                        E  Denying clipboard access to com.akslabs.circletosearch, application is not in focus nor is it a system service for user 0
2025-11-25 16:39:42.341  1969-2379  ClipboardService        system_server                        E  Denying clipboard access to com.akslabs.circletosearch, application is not in focus nor is it a system service for user 0
2025-11-25 16:55:53.233  9677-9677  .circletosearch         com.akslabs.circletosearch           I  Late-enabling -Xcheck:jni
2025-11-25 16:55:53.301  9677-9677  .circletosearch         com.akslabs.circletosearch           I  Using CollectorTypeCC GC.
2025-11-25 16:55:53.385  9677-9677  .circletosearch         com.akslabs.circletosearch           W  Thread Pool max thread count is 0. Cannot cache binder as linkToDeath cannot be implemented. serviceName: activity
2025-11-25 16:55:53.441  9677-9677  nativeloader            com.akslabs.circletosearch           D  Load libframework-connectivity-tiramisu-jni.so using APEX ns com_android_tethering for caller /apex/com.android.tethering/javalib/framework-connectivity-t.jar: ok
2025-11-25 16:55:53.523  9677-9677  re-initialized>         com.akslabs.circletosearch           W  type=1400 audit(0.0:41340): avc:  granted  { execute } for  path="/data/data/com.akslabs.circletosearch/code_cache/startup_agents/43ec9884-agent.so" dev="mmcblk0p61" ino=803078 scontext=u:r:untrusted_app:s0:c216,c258,c512,c768 tcontext=u:object_r:app_data_file:s0:c216,c258,c512,c768 tclass=file app=com.akslabs.circletosearch
2025-11-25 16:55:53.580  9677-9677  nativeloader            com.akslabs.circletosearch           D  Load /data/user/0/com.akslabs.circletosearch/code_cache/startup_agents/43ec9884-agent.so using system ns (caller=<unknown>): ok
2025-11-25 16:55:53.606  9677-9677  .circletosearch         com.akslabs.circletosearch           W  hiddenapi: DexFile /data/data/com.akslabs.circletosearch/code_cache/.studio/instruments-aba82530.jar is in boot class path but is not in a known location
2025-11-25 16:55:53.858  9677-9677  .circletosearch         com.akslabs.circletosearch           W  Redefining intrinsic method java.lang.Thread java.lang.Thread.currentThread(). This may cause the unexpected use of the original definition of java.lang.Thread java.lang.Thread.currentThread()in methods that have already been compiled.
2025-11-25 16:55:53.858  9677-9677  .circletosearch         com.akslabs.circletosearch           W  Redefining intrinsic method boolean java.lang.Thread.interrupted(). This may cause the unexpected use of the original definition of boolean java.lang.Thread.interrupted()in methods that have already been compiled.
2025-11-25 16:55:54.939  9677-9677  nativeloader            com.akslabs.circletosearch           D  Configuring clns-9 for other apk /data/app/~~LHJHuqxyLCwL2Tvsi3pKHw==/com.akslabs.circletosearch-yWOoyCstXVO7qxxINy9eDA==/base.apk. target_sdk_version=36, uses_libraries=, library_path=/data/app/~~LHJHuqxyLCwL2Tvsi3pKHw==/com.akslabs.circletosearch-yWOoyCstXVO7qxxINy9eDA==/lib/arm64:/data/app/~~LHJHuqxyLCwL2Tvsi3pKHw==/com.akslabs.circletosearch-yWOoyCstXVO7qxxINy9eDA==/base.apk!/lib/arm64-v8a, permitted_path=/data:/mnt/expand:/data/user/0/com.akslabs.circletosearch
2025-11-25 16:55:54.955  9677-9677  .circletosearch         com.akslabs.circletosearch           I  AssetManager2(0x789a078b28) locale list changing from [] to [en-IN]
2025-11-25 16:55:54.959  9677-9677  .circletosearch         com.akslabs.circletosearch           I  AssetManager2(0x789a079428) locale list changing from [] to [en-IN]
2025-11-25 16:55:54.969  9677-9677  GraphicsEnvironment     com.akslabs.circletosearch           V  Currently set values for:
2025-11-25 16:55:54.969  9677-9677  GraphicsEnvironment     com.akslabs.circletosearch           V    angle_gl_driver_selection_pkgs=[com.android.angle, com.linecorp.b612.android, com.campmobile.snow, com.google.android.apps.tachyon]
2025-11-25 16:55:54.970  9677-9677  GraphicsEnvironment     com.akslabs.circletosearch           V    angle_gl_driver_selection_values=[angle, native, native, native]
2025-11-25 16:55:54.970  9677-9677  GraphicsEnvironment     com.akslabs.circletosearch           V  com.akslabs.circletosearch is not listed in per-application setting
2025-11-25 16:55:54.970  9677-9677  GraphicsEnvironment     com.akslabs.circletosearch           V  ANGLE allowlist from config: 
2025-11-25 16:55:54.970  9677-9677  GraphicsEnvironment     com.akslabs.circletosearch           V  com.akslabs.circletosearch is not listed in ANGLE allowlist or settings, returning default
2025-11-25 16:55:54.970  9677-9677  GraphicsEnvironment     com.akslabs.circletosearch           V  Neither updatable production driver nor prerelease driver is supported.
2025-11-25 16:55:55.076  9677-9677  .circletosearch         com.akslabs.circletosearch           I  AssetManager2(0x789a079728) locale list changing from [] to [en-IN]
2025-11-25 16:55:55.080  9677-12322 DisplayManager          com.akslabs.circletosearch           I  Choreographer implicitly registered for the refresh rate.
2025-11-25 16:55:55.098  9677-12322 AdrenoGLES-0            com.akslabs.circletosearch           I  QUALCOMM build                   : 95db91f, Ifbc588260a
                                                                                                    Build Date                       : 09/24/20
                                                                                                    OpenGL ES Shader Compiler Version: EV031.32.02.01
                                                                                                    Local Branch                     : mybrancheafe5b6d-fb5b-f1b0-b904-5cb90179c3e0
                                                                                                    Remote Branch                    : quic/gfx-adreno.lnx.1.0.r114-rel
                                                                                                    Remote Branch                    : NONE
                                                                                                    Reconstruct Branch               : NOTHING
2025-11-25 16:55:55.098  9677-12322 AdrenoGLES-0            com.akslabs.circletosearch           I  Build Config                     : S P 10.0.7 AArch64
2025-11-25 16:55:55.098  9677-12322 AdrenoGLES-0            com.akslabs.circletosearch           I  Driver Path                      : /vendor/lib64/egl/libGLESv2_adreno.so
2025-11-25 16:55:55.123  9677-12322 AdrenoGLES-0            com.akslabs.circletosearch           I  PFP: 0x016ee190, ME: 0x00000000
2025-11-25 16:55:55.144  9677-9677  ashmem                  com.akslabs.circletosearch           E  Pinning is deprecated since Android Q. Please use trim or other methods.
2025-11-25 16:55:55.159  1969-2008  NotificationService     system_server                        E  Suppressing toast from package com.akslabs.circletosearch by user request.
2025-11-25 16:55:55.193  9677-9677  CompatChangeReporter    com.akslabs.circletosearch           D  Compat change id reported: 377864165; UID 10728; state: ENABLED
2025-11-25 16:55:55.197  9677-9677  DesktopModeFlags        com.akslabs.circletosearch           D  Toggle override initialized to: OVERRIDE_UNSET
2025-11-25 16:55:55.315  9677-12325 Gralloc4                com.akslabs.circletosearch           I  mapper 4.x is not supported
2025-11-25 16:55:55.316  9677-12325 Gralloc3                com.akslabs.circletosearch           W  mapper 3.x is not supported
2025-11-25 16:55:55.862  9677-9677  .circletosearch         com.akslabs.circletosearch           I  hiddenapi: Accessing hidden method Landroid/os/SystemProperties;->addChangeCallback(Ljava/lang/Runnable;)V (runtime_flags=0, domain=platform, api=unsupported) from Landroidx/compose/ui/platform/AndroidComposeView$Companion; (domain=app) using reflection: allowed
2025-11-25 16:55:56.450  9677-12328 Gralloc2                com.akslabs.circletosearch           I  Adding additional valid usage bits: 0x8202000
2025-11-25 16:55:56.645  9677-9677  Choreographer           com.akslabs.circletosearch           I  Skipped 69 frames!  The application may be doing too much work on its main thread.
2025-11-25 16:56:00.599  9677-12446 ProfileInstaller        com.akslabs.circletosearch           D  Installing profile for com.akslabs.circletosearch
2025-11-25 16:56:03.468  9677-9677  InsetsController        com.akslabs.circletosearch           D  hide(ime(), fromIme=false)
2025-11-25 16:56:03.477  9677-9677  ImeTracker              com.akslabs.circletosearch           I  com.akslabs.circletosearch:17fb7ca1: onCancelled at PHASE_CLIENT_ALREADY_HIDDEN
2025-11-25 16:56:03.561  9677-12313 .circletosearch         com.akslabs.circletosearch           I  Background concurrent copying GC freed 8502KB AllocSpace bytes, 0(0B) LOS objects, 66% free, 4003KB/11MB, paused 1.197ms,26us total 256.494ms
2025-11-25 16:56:05.103  9677-9677  InsetsController        com.akslabs.circletosearch           D  hide(ime(), fromIme=false)
2025-11-25 16:56:05.103  9677-9677  ImeTracker              com.akslabs.circletosearch           I  com.akslabs.circletosearch:68eb1550: onCancelled at PHASE_CLIENT_ALREADY_HIDDEN
2025-11-25 16:56:09.777  9677-9677  VRI[MainActivity]       com.akslabs.circletosearch           D  visibilityChanged oldVisibility=true newVisibility=false
2025-11-25 16:56:13.353  9677-9677  ViewRootImpl            com.akslabs.circletosearch           D  Skipping stats log for color mode
2025-11-25 16:56:13.537  9677-9677  InsetsController        com.akslabs.circletosearch           D  hide(ime(), fromIme=false)
2025-11-25 16:56:13.537  9677-9677  ImeTracker              com.akslabs.circletosearch           I  com.akslabs.circletosearch:7a3c5264: onCancelled at PHASE_CLIENT_ALREADY_HIDDEN
2025-11-25 16:56:14.868  9677-9677  CircleToSearch          com.akslabs.circletosearch           D  OverlayActivity onCreate
2025-11-25 16:56:14.868  9677-9677  CircleToSearch          com.akslabs.circletosearch           D  Bitmap loaded from Repository. Size: 1080x2340
2025-11-25 16:56:15.382  9677-9677  InsetsController        com.akslabs.circletosearch           D  hide(ime(), fromIme=false)
2025-11-25 16:56:15.382  9677-9677  ImeTracker              com.akslabs.circletosearch           I  com.akslabs.circletosearch:27bc8413: onCancelled at PHASE_CLIENT_ALREADY_HIDDEN
2025-11-25 16:56:16.736  9677-9677  CircleToSearch          com.akslabs.circletosearch           D  Selection rect: 257,631,833,1264
2025-11-25 16:56:16.741  9677-9677  CircleToSearch          com.akslabs.circletosearch           D  Cropped bitmap size: 576x633
2025-11-25 16:56:18.135  9677-12640 ImageSearchUploader     com.akslabs.circletosearch           D  Google Upload Response Code: 302
2025-11-25 16:56:18.194  9677-9677  .circletosearch         com.akslabs.circletosearch           I  AssetManager2(0x78ad78eb28) locale list changing from [] to [en-IN]
2025-11-25 16:56:18.194  9677-9677  WebViewFactory          com.akslabs.circletosearch           I  Loading com.google.android.webview version 142.0.7444.102 (code 744410233)
2025-11-25 16:56:18.197  9677-9677  .circletosearch         com.akslabs.circletosearch           I  AssetManager2(0x78ad78f128) locale list changing from [] to [en-IN]
2025-11-25 16:56:18.200  9677-9677  .circletosearch         com.akslabs.circletosearch           I  AssetManager2(0x78ad78ee28) locale list changing from [] to [en-IN]
2025-11-25 16:56:18.203  9677-9677  .circletosearch         com.akslabs.circletosearch           I  AssetManager2(0x783c3eeb28) locale list changing from [] to [en-IN]
2025-11-25 16:56:18.204  9677-9677  ResourcesManager        com.akslabs.circletosearch           V  The following library key has been added: ResourcesKey{ mHash=aa84934e mResDir=null mSplitDirs=[] mOverlayDirs=[] mLibDirs=[/data/app/~~sjzTljojN_LVeFaP6KzdrA==/com.google.android.webview-bxztEjelqIwm_Zgxcbmy-w==/base.apk,/data/app/~~_Cvt6oKSXHQJX_50gqIKRQ==/com.google.android.trichromelibrary_744410233-Jj5siaXn3ezExguOmkFV_w==/base.apk,/system_ext/framework/androidx.window.extensions.jar] mDisplayId=0 mOverrideConfig=v36 mCompatInfo={420dpi always-compat} mLoaders=[]}
2025-11-25 16:56:18.209  9677-9677  .circletosearch         com.akslabs.circletosearch           W  Failed to find entry 'classes.dex': Entry not found
2025-11-25 16:56:18.212  9677-9677  nativeloader            com.akslabs.circletosearch           D  Configuring clns-10 for other apk /data/app/~~_Cvt6oKSXHQJX_50gqIKRQ==/com.google.android.trichromelibrary_744410233-Jj5siaXn3ezExguOmkFV_w==/base.apk. target_sdk_version=36, uses_libraries=ALL, library_path=/data/app/~~sjzTljojN_LVeFaP6KzdrA==/com.google.android.webview-bxztEjelqIwm_Zgxcbmy-w==/lib/arm64:/data/app/~~sjzTljojN_LVeFaP6KzdrA==/com.google.android.webview-bxztEjelqIwm_Zgxcbmy-w==/base.apk!/lib/arm64-v8a:/data/app/~~_Cvt6oKSXHQJX_50gqIKRQ==/com.google.android.trichromelibrary_744410233-Jj5siaXn3ezExguOmkFV_w==/base.apk!/lib/arm64-v8a, permitted_path=/data:/mnt/expand
2025-11-25 16:56:18.216  9677-9677  ApplicationLoaders      com.akslabs.circletosearch           D  Returning zygote-cached class loader: /system_ext/framework/androidx.window.extensions.jar
2025-11-25 16:56:18.219  9677-9677  .circletosearch         com.akslabs.circletosearch           W  Loading /data/app/~~sjzTljojN_LVeFaP6KzdrA==/com.google.android.webview-bxztEjelqIwm_Zgxcbmy-w==/oat/arm64/base.odex non-executable as it requires an image which we failed to load
2025-11-25 16:56:18.222  9677-9677  nativeloader            com.akslabs.circletosearch           D  Configuring clns-11 for other apk /data/app/~~sjzTljojN_LVeFaP6KzdrA==/com.google.android.webview-bxztEjelqIwm_Zgxcbmy-w==/base.apk. target_sdk_version=36, uses_libraries=, library_path=/data/app/~~sjzTljojN_LVeFaP6KzdrA==/com.google.android.webview-bxztEjelqIwm_Zgxcbmy-w==/lib/arm64:/data/app/~~sjzTljojN_LVeFaP6KzdrA==/com.google.android.webview-bxztEjelqIwm_Zgxcbmy-w==/base.apk!/lib/arm64-v8a:/data/app/~~_Cvt6oKSXHQJX_50gqIKRQ==/com.google.android.trichromelibrary_744410233-Jj5siaXn3ezExguOmkFV_w==/base.apk!/lib/arm64-v8a, permitted_path=/data:/mnt/expand
2025-11-25 16:56:18.349  9677-9677  .circletosearch         com.akslabs.circletosearch           I  AssetManager2(0x78a8cd1028) locale list changing from [] to [en-IN]
2025-11-25 16:56:18.406  9677-9677  cr_WVCFactoryProvider   com.akslabs.circletosearch           I  version=142.0.7444.102 (744410233) minSdkVersion=29 multiprocess=true packageId=127 splits=<none>
2025-11-25 16:56:18.434  9677-9677  nativeloader            com.akslabs.circletosearch           D  Load /data/app/~~_Cvt6oKSXHQJX_50gqIKRQ==/com.google.android.trichromelibrary_744410233-Jj5siaXn3ezExguOmkFV_w==/base.apk!/lib/arm64-v8a/libmonochrome_64.so using class loader ns clns-11 (caller=/data/app/~~sjzTljojN_LVeFaP6KzdrA==/com.google.android.webview-bxztEjelqIwm_Zgxcbmy-w==/base.apk): ok
2025-11-25 16:56:18.439  9677-9677  nativeloader            com.akslabs.circletosearch           D  Load /system/lib64/libwebviewchromium_plat_support.so using class loader ns clns-11 (caller=/data/app/~~sjzTljojN_LVeFaP6KzdrA==/com.google.android.webview-bxztEjelqIwm_Zgxcbmy-w==/base.apk): ok
2025-11-25 16:56:18.445  9677-9677  ExportedFlags           com.akslabs.circletosearch           E  android.os.flagging.AconfigStorageReadException: ERROR_PACKAGE_NOT_FOUND: package android.webkit cannot be found on the device
2025-11-25 16:56:18.452  9677-9677  .circletosearch         com.akslabs.circletosearch           I  AssetManager2(0x78a8cd1628) locale list changing from [] to [en-IN]
2025-11-25 16:56:18.455  9677-12654 chromium                com.akslabs.circletosearch           I  [1125/165618.447931:INFO:android_webview/browser/variations/variations_seed_loader.cc:67] Failed to open file for reading.: No such file or directory (2)
2025-11-25 16:56:18.463  9677-9677  cr_LibraryLoader        com.akslabs.circletosearch           I  Successfully loaded native library
2025-11-25 16:56:18.468  9677-9677  cr_CachingUmaRecorder   com.akslabs.circletosearch           I  Flushed 18 samples from 18 histograms, 0 samples were dropped.
2025-11-25 16:56:18.493  9677-9677  cr_VariationsSeedLoader com.akslabs.circletosearch           E  Failed loading variations seed. Variations disabled.
2025-11-25 16:56:18.497  9677-9677  cr_CombinedPProvider    com.akslabs.circletosearch           I  #registerProvider() provider:WV.hk@40c0a34 isPolicyCacheEnabled:false policyProvidersSize:0
2025-11-25 16:56:18.500  9677-9677  cr_PolicyProvider       com.akslabs.circletosearch           I  #setManagerAndSource() 0
2025-11-25 16:56:18.522  9677-9677  .circletosearch         com.akslabs.circletosearch           I  AssetManager2(0x78a8cd1928) locale list changing from [] to [en-IN]
2025-11-25 16:56:18.525  9677-9677  .circletosearch         com.akslabs.circletosearch           I  AssetManager2(0x78a8cd1c28) locale list changing from [] to [en-IN]
2025-11-25 16:56:18.628  9677-9677  cr_CombinedPProvider    com.akslabs.circletosearch           I  #linkNativeInternal() 1
2025-11-25 16:56:18.630  9677-9677  cr_AppResProvider       com.akslabs.circletosearch           I  #getApplicationRestrictionsFromUserManager() Bundle[EMPTY_PARCEL]
2025-11-25 16:56:18.631  9677-9677  cr_PolicyProvider       com.akslabs.circletosearch           I  #notifySettingsAvailable() 0
2025-11-25 16:56:18.631  9677-9677  cr_CombinedPProvider    com.akslabs.circletosearch           I  #onSettingsAvailable() 0
2025-11-25 16:56:18.631  9677-9677  cr_CombinedPProvider    com.akslabs.circletosearch           I  #flushPolicies()
2025-11-25 16:56:18.906  9677-12665 chromium                com.akslabs.circletosearch           W  [WARNING:net/dns/dns_config_service_android.cc:69] Failed to read DnsConfig.
2025-11-25 16:56:18.940  9677-9677  chromium                com.akslabs.circletosearch           W  [WARNING:android_webview/browser/network_service/net_helpers.cc:143] HTTP Cache size is: 20971520
2025-11-25 16:56:19.501  9677-12670 cr_media                com.akslabs.circletosearch           W  BLUETOOTH_CONNECT permission is missing.
2025-11-25 16:56:19.508  9677-12670 cr_media                com.akslabs.circletosearch           W  getBluetoothAdapter() requires BLUETOOTH permission
2025-11-25 16:56:19.508  9677-12670 cr_media                com.akslabs.circletosearch           W  registerBluetoothIntentsIfNeeded: Requires BLUETOOTH permission
2025-11-25 16:56:19.689  9677-12698 vulkan                  com.akslabs.circletosearch           D  searching for layers in '/data/app/~~LHJHuqxyLCwL2Tvsi3pKHw==/com.akslabs.circletosearch-yWOoyCstXVO7qxxINy9eDA==/lib/arm64'
2025-11-25 16:56:19.692  9677-12698 vulkan                  com.akslabs.circletosearch           D  searching for layers in '/data/app/~~LHJHuqxyLCwL2Tvsi3pKHw==/com.akslabs.circletosearch-yWOoyCstXVO7qxxINy9eDA==/base.apk!/lib/arm64-v8a'
2025-11-25 16:56:19.737  9677-12698 AdrenoVK-0              com.akslabs.circletosearch           I  ===== BEGIN DUMP OF OVERRIDDEN SETTINGS =====
2025-11-25 16:56:19.738  9677-12698 AdrenoVK-0              com.akslabs.circletosearch           I  ===== END DUMP OF OVERRIDDEN SETTINGS =====
2025-11-25 16:56:19.743  9677-12698 AdrenoVK-0              com.akslabs.circletosearch           I  QUALCOMM build          : 95db91f, Ifbc588260a
                                                                                                    Build Date              : 09/24/20
                                                                                                    Shader Compiler Version : EV031.32.02.01
                                                                                                    Local Branch            : mybrancheafe5b6d-fb5b-f1b0-b904-5cb90179c3e0
                                                                                                    Remote Branch           : quic/gfx-adreno.lnx.1.0.r114-rel
                                                                                                    Remote Branch           : NONE
                                                                                                    Reconstruct Branch      : NOTHING
2025-11-25 16:56:19.743  9677-12698 AdrenoVK-0              com.akslabs.circletosearch           I  Build Config            : S P 10.0.7 AArch64
2025-11-25 16:56:19.743  9677-12698 AdrenoVK-0              com.akslabs.circletosearch           I  Driver Path             : /vendor/lib64/hw/vulkan.adreno.so
2025-11-25 16:56:19.810  9677-9677  Choreographer           com.akslabs.circletosearch           I  Skipped 99 frames!  The application may be doing too much work on its main thread.
2025-11-25 16:56:19.815  9677-12702 CameraManagerGlobal     com.akslabs.circletosearch           I  Connecting to camera service
2025-11-25 16:56:19.833  9677-12702 CameraManagerGlobal     com.akslabs.circletosearch           W  Ignoring status update of camera 0
2025-11-25 16:56:19.833  9677-12702 CameraManagerGlobal     com.akslabs.circletosearch           W  Ignoring status update of camera 0
2025-11-25 16:56:19.836  9677-12422 CameraManagerGlobal     com.akslabs.circletosearch           W  ignore the torch status update of camera: 3
2025-11-25 16:56:20.191  9677-12680 VideoCapabilities       com.akslabs.circletosearch           W  could not parse integer range: 788-757
2025-11-25 16:56:20.191  9677-12680 VideoCapabilities       com.akslabs.circletosearch           W  could not parse integer range: 484-472
2025-11-25 16:56:20.196  9677-12680 VideoCapabilities       com.akslabs.circletosearch           W  Unrecognized profile/level 0/3 for video/mpeg2
2025-11-25 16:56:20.197  9677-12680 VideoCapabilities       com.akslabs.circletosearch           W  Unrecognized profile/level 0/3 for video/mpeg2
2025-11-25 16:56:20.221  9677-12680 Utils                   com.akslabs.circletosearch           W  could not parse long range '788-757'
2025-11-25 16:56:20.221  9677-12680 Utils                   com.akslabs.circletosearch           W  could not parse long range '484-472'
2025-11-25 16:56:20.223  9677-12680 VideoCapabilities       com.akslabs.circletosearch           W  Unrecognized profile/level 0/3 for video/mpeg2
2025-11-25 16:56:20.224  9677-12680 VideoCapabilities       com.akslabs.circletosearch           W  Unrecognized profile/level 0/3 for video/mpeg2
2025-11-25 16:56:27.686  9677-9677  chromium                com.akslabs.circletosearch           E  [ERROR:android_webview/browser/aw_browser_terminator.cc:165] Renderer process (12659) crash detected (code -1).
2025-11-25 16:56:27.771  9677-12640 ImageSearchUploader     com.akslabs.circletosearch           D  Bing Upload Response Code: 200
2025-11-25 16:56:28.184  9677-12640 ImageSearchUploader     com.akslabs.circletosearch           D  Bing 200 OK Body (First 1000 chars): <!DOCTYPE html><html dir="ltr" lang="en" xml:lang="en" xmlns="http://www.w3.org/1999/xhtml" xmlns:Web="http://schemas.live.com/Web/"><script type="text/javascript" nonce="RZUyGgpiM6Q//PPh3jWb9tqeKs4RwVs8bdkAjVCc5Yg=" >//<![CDATA[
                                                                                                    si_ST=new Date
                                                                                                    //]]></script><head><!--pc--><title>Search</title><meta name="twitter:card" content="summary_large_image" /><meta content="Find high-quality images, photos, and animated GIFS with Bing Images" name="description" /><meta content="Find high-quality images, photos, and animated GIFS with Bing Images" name="twitter:description" /><title>Images</title><meta content="text/html; charset=utf-8" http-equiv="content-type" /><meta name="referrer" content="origin-when-cross-origin" /><meta property="og:description" content="Intelligent search from Bing makes it easier to quickly find what you’re looking for and rewards you." /><meta property="og:site_name" content="Bing" /><meta property="og:title" content="Bing" /><meta property="og:url" content="https:/
2025-11-25 16:56:28.189  9677-12640 ImageSearchUploader     com.akslabs.circletosearch           D  Bing: Found URL in og:url: https://www.bing.com/images/search?view=detailv2&iss=sbi&form=SBIHMP
2025-11-25 16:56:28.211  9677-9677  .circletosearch         com.akslabs.circletosearch           I  AssetManager2(0x78a8f6f528) locale list changing from [] to [en-IN]
2025-11-25 16:56:29.180  9677-9677  chromium                com.akslabs.circletosearch           I  [INFO:CONSOLE:4] "Uncaught ReferenceError: define is not defined", source: https://www.bing.com/images?FORM=SBIRDI (4)
2025-11-25 16:56:29.719  9677-9677  chromium                com.akslabs.circletosearch           I  [INFO:CONSOLE:6] "Permissions policy violation: unload is not allowed in this document.", source: https://www.bing.com/images?FORM=SBIRDI (6)
2025-11-25 16:56:30.079  9677-9677  chromium                com.akslabs.circletosearch           I  [INFO:CONSOLE:6] "Permissions policy violation: unload is not allowed in this document.", source: https://www.bing.com/images?FORM=SBIRDI (6)
2025-11-25 16:56:30.207  9677-9677  chromium                com.akslabs.circletosearch           I  [INFO:CONSOLE:6] "Permissions policy violation: unload is not allowed in this document.", source: https://www.bing.com/images?FORM=SBIRDI (6)
2025-11-25 16:56:30.208  9677-9677  chromium                com.akslabs.circletosearch           I  [INFO:CONSOLE:6] "Permissions policy violation: unload is not allowed in this document.", source: https://www.bing.com/images?FORM=SBIRDI (6)
2025-11-25 16:56:30.208  9677-9677  chromium                com.akslabs.circletosearch           I  [INFO:CONSOLE:6] "Permissions policy violation: unload is not allowed in this document.", source: https://www.bing.com/images?FORM=SBIRDI (6)
2025-11-25 16:56:41.349  9677-9677  chromium                com.akslabs.circletosearch           E  [ERROR:android_webview/browser/aw_browser_terminator.cc:165] Renderer process (12743) crash detected (code -1).
2025-11-25 16:57:11.720  9677-12640 ImageSearchUploader     com.akslabs.circletosearch           E  Upload failed for https://graph.baidu.com/upload (Fix with AI)
                                                                                                    java.net.SocketException: Connection reset
                                                                                                    	at java.net.SocketInputStream.read(SocketInputStream.java:191)
                                                                                                    	at java.net.SocketInputStream.read(SocketInputStream.java:143)
                                                                                                    	at com.android.org.conscrypt.ConscryptEngineSocket$SSLInputStream.readFromSocket(ConscryptEngineSocket.java:994)
                                                                                                    	at com.android.org.conscrypt.ConscryptEngineSocket$SSLInputStream.processDataFromSocket(ConscryptEngineSocket.java:958)
                                                                                                    	at com.android.org.conscrypt.ConscryptEngineSocket$SSLInputStream.-$$Nest$mprocessDataFromSocket(Unknown Source:0)
                                                                                                    	at com.android.org.conscrypt.ConscryptEngineSocket.doHandshake(ConscryptEngineSocket.java:242)
                                                                                                    	at com.android.org.conscrypt.ConscryptEngineSocket.startHandshake(ConscryptEngineSocket.java:224)
                                                                                                    	at com.android.okhttp.internal.io.RealConnection.connectTls(RealConnection.java:196)
                                                                                                    	at com.android.okhttp.internal.io.RealConnection.connectSocket(RealConnection.java:153)
                                                                                                    	at com.android.okhttp.internal.io.RealConnection.connect(RealConnection.java:116)
                                                                                                    	at com.android.okhttp.internal.http.StreamAllocation.findConnection(StreamAllocation.java:186)
                                                                                                    	at com.android.okhttp.internal.http.StreamAllocation.findHealthyConnection(StreamAllocation.java:128)
                                                                                                    	at com.android.okhttp.internal.http.StreamAllocation.newStream(StreamAllocation.java:97)
                                                                                                    	at com.android.okhttp.internal.http.HttpEngine.connect(HttpEngine.java:289)
                                                                                                    	at com.android.okhttp.internal.http.HttpEngine.sendRequest(HttpEngine.java:232)
                                                                                                    	at com.android.okhttp.internal.huc.HttpURLConnectionImpl.execute(HttpURLConnectionImpl.java:465)
                                                                                                    	at com.android.okhttp.internal.huc.HttpURLConnectionImpl.connect(HttpURLConnectionImpl.java:131)
                                                                                                    	at com.android.okhttp.internal.huc.HttpURLConnectionImpl.getOutputStream(HttpURLConnectionImpl.java:262)
                                                                                                    	at com.android.okhttp.internal.huc.DelegatingHttpsURLConnection.getOutputStream(DelegatingHttpsURLConnection.java:219)
                                                                                                    	at com.android.okhttp.internal.huc.HttpsURLConnectionImpl.getOutputStream(HttpsURLConnectionImpl.java:30)
                                                                                                    	at com.akslabs.circletosearch.utils.ImageSearchUploader$performUpload$3.invokeSuspend(ImageSearchUploader.kt:55)
                                                                                                    	at kotlin.coroutines.jvm.internal.BaseContinuationImpl.resumeWith(ContinuationImpl.kt:33)
                                                                                                    	at kotlinx.coroutines.DispatchedTask.run(DispatchedTask.kt:101)
                                                                                                    	at kotlinx.coroutines.internal.LimitedDispatcher$Worker.run(LimitedDispatcher.kt:113)
                                                                                                    	at kotlinx.coroutines.scheduling.TaskImpl.run(Tasks.kt:89)
                                                                                                    	at kotlinx.coroutines.scheduling.CoroutineScheduler.runSafely(CoroutineScheduler.kt:589)
                                                                                                    	at kotlinx.coroutines.scheduling.CoroutineScheduler$Worker.executeTask(CoroutineScheduler.kt:823)
                                                                                                    	at kotlinx.coroutines.scheduling.CoroutineScheduler$Worker.runWorker(CoroutineScheduler.kt:720)
                                                                                                    	at kotlinx.coroutines.scheduling.CoroutineScheduler$Worker.run(CoroutineScheduler.kt:707)
2025-11-25 16:57:13.550  9677-9677  .circletosearch         com.akslabs.circletosearch           I  hiddenapi: Accessing hidden method Landroid/view/ViewTreeObserver;->dispatchOnScrollChanged()V (runtime_flags=0, domain=platform, api=unsupported) from Landroidx/compose/ui/platform/AndroidComposeView$Companion; (domain=app) using reflection: allowed
2025-11-25 16:57:15.267  9677-12640 ImageSearchUploader     com.akslabs.circletosearch           D  Google Upload Response Code: 302
2025-11-25 16:57:15.283  9677-9677  .circletosearch         com.akslabs.circletosearch           I  AssetManager2(0x78a8f70728) locale list changing from [] to [en-IN]
2025-11-25 16:57:18.223  9677-12640 ImageSearchUploader     com.akslabs.circletosearch           D  Bing Upload Response Code: 200
2025-11-25 16:57:18.592  9677-12640 ImageSearchUploader     com.akslabs.circletosearch           D  Bing 200 OK Body (First 1000 chars): <!DOCTYPE html><html dir="ltr" lang="en" xml:lang="en" xmlns="http://www.w3.org/1999/xhtml" xmlns:Web="http://schemas.live.com/Web/"><script type="text/javascript" nonce="77lBFScsNKShHWIm2tKb+ddF1VvXItu6sksK+J97oFY=" >//<![CDATA[
                                                                                                    si_ST=new Date
                                                                                                    //]]></script><head><!--pc--><title>Search</title><meta name="twitter:card" content="summary_large_image" /><meta content="Find high-quality images, photos, and animated GIFS with Bing Images" name="description" /><meta content="Find high-quality images, photos, and animated GIFS with Bing Images" name="twitter:description" /><title>Images</title><meta content="text/html; charset=utf-8" http-equiv="content-type" /><meta name="referrer" content="origin-when-cross-origin" /><meta property="og:description" content="Intelligent search from Bing makes it easier to quickly find what you’re looking for and rewards you." /><meta property="og:site_name" content="Bing" /><meta property="og:title" content="Bing" /><meta property="og:url" content="https:/
2025-11-25 16:57:18.596  9677-12640 ImageSearchUploader     com.akslabs.circletosearch           D  Bing: Found URL in og:url: https://www.bing.com/images/search?view=detailv2&iss=sbi&form=SBIHMP
2025-11-25 16:57:19.539  9677-9677  chromium                com.akslabs.circletosearch           I  [INFO:CONSOLE:4] "Uncaught ReferenceError: define is not defined", source: https://www.bing.com/images?FORM=SBIRDI (4)
2025-11-25 16:57:19.824  9677-9677  chromium                com.akslabs.circletosearch           I  [INFO:CONSOLE:6] "Permissions policy violation: unload is not allowed in this document.", source: https://www.bing.com/images?FORM=SBIRDI (6)
2025-11-25 16:57:20.024  9677-9677  chromium                com.akslabs.circletosearch           I  [INFO:CONSOLE:6] "Permissions policy violation: unload is not allowed in this document.", source: https://www.bing.com/images?FORM=SBIRDI (6)
2025-11-25 16:57:20.107  9677-9677  chromium                com.akslabs.circletosearch           I  [INFO:CONSOLE:6] "Permissions policy violation: unload is not allowed in this document.", source: https://www.bing.com/images?FORM=SBIRDI (6)
2025-11-25 16:57:20.108  9677-9677  chromium                com.akslabs.circletosearch           I  [INFO:CONSOLE:6] "Permissions policy violation: unload is not allowed in this document.", source: https://www.bing.com/images?FORM=SBIRDI (6)
2025-11-25 16:57:20.108  9677-9677  chromium                com.akslabs.circletosearch           I  [INFO:CONSOLE:6] "Permissions policy violation: unload is not allowed in this document.", source: https://www.bing.com/images?FORM=SBIRDI (6)
2025-11-25 16:57:24.761  9677-9677  CircleToSearch          com.akslabs.circletosearch           D  Selection rect: 371,651,780,1003
2025-11-25 16:57:24.763  9677-9677  CircleToSearch          com.akslabs.circletosearch           D  Cropped bitmap size: 409x352
2025-11-25 16:57:25.430  9677-12640 ImageSearchUploader     com.akslabs.circletosearch           D  Bing Upload Response Code: 200
2025-11-25 16:57:25.509  9677-9677  chromium                com.akslabs.circletosearch           E  [ERROR:android_webview/browser/aw_browser_terminator.cc:165] Renderer process (12806) crash detected (code -1).
2025-11-25 16:57:25.646  9677-12640 ImageSearchUploader     com.akslabs.circletosearch           D  Bing 200 OK Body (First 1000 chars): <!DOCTYPE html><html dir="ltr" lang="en" xml:lang="en" xmlns="http://www.w3.org/1999/xhtml" xmlns:Web="http://schemas.live.com/Web/"><script type="text/javascript" nonce="E8yMp07Ua4TjQuckLGrW1H0EL7BxlkJIuge6nXNetk8=" >//<![CDATA[
                                                                                                    si_ST=new Date
                                                                                                    //]]></script><head><!--pc--><title>Search</title><meta name="twitter:card" content="summary_large_image" /><meta content="Find high-quality images, photos, and animated GIFS with Bing Images" name="description" /><meta content="Find high-quality images, photos, and animated GIFS with Bing Images" name="twitter:description" /><title>Images</title><meta content="text/html; charset=utf-8" http-equiv="content-type" /><meta name="referrer" content="origin-when-cross-origin" /><meta property="og:description" content="Intelligent search from Bing makes it easier to quickly find what you’re looking for and rewards you." /><meta property="og:site_name" content="Bing" /><meta property="og:title" content="Bing" /><meta property="og:url" content="https:/
2025-11-25 16:57:25.649  9677-12640 ImageSearchUploader     com.akslabs.circletosearch           D  Bing: Found URL in og:url: https://www.bing.com/images/search?view=detailv2&iss=sbi&form=SBIHMP
2025-11-25 16:57:25.667  9677-9677  .circletosearch         com.akslabs.circletosearch           I  AssetManager2(0x78a903a028) locale list changing from [] to [en-IN]
2025-11-25 16:57:26.847  9677-9677  chromium                com.akslabs.circletosearch           I  [INFO:CONSOLE:4] "Uncaught ReferenceError: define is not defined", source: https://www.bing.com/images?FORM=SBIRDI (4)
2025-11-25 16:57:27.040  9677-9677  chromium                com.akslabs.circletosearch           I  [INFO:CONSOLE:6] "Permissions policy violation: unload is not allowed in this document.", source: https://www.bing.com/images?FORM=SBIRDI (6)
2025-11-25 16:57:27.214  9677-9677  chromium                com.akslabs.circletosearch           I  [INFO:CONSOLE:6] "Permissions policy violation: unload is not allowed in this document.", source: https://www.bing.com/images?FORM=SBIRDI (6)
2025-11-25 16:57:27.280  9677-9677  chromium                com.akslabs.circletosearch           I  [INFO:CONSOLE:6] "Permissions policy violation: unload is not allowed in this document.", source: https://www.bing.com/images?FORM=SBIRDI (6)
2025-11-25 16:57:27.280  9677-9677  chromium                com.akslabs.circletosearch           I  [INFO:CONSOLE:6] "Permissions policy violation: unload is not allowed in this document.", source: https://www.bing.com/images?FORM=SBIRDI (6)
2025-11-25 16:57:27.280  9677-9677  chromium                com.akslabs.circletosearch           I  [INFO:CONSOLE:6] "Permissions policy violation: unload is not allowed in this document.", source: https://www.bing.com/images?FORM=SBIRDI (6)
