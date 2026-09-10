time: 1789069896381
device fp:realme/RMX1851/RMX1851:11/RKQ1.201217.002/1623376276806:user/release-keys
product board:sdm710
everest vers:
msg: android.view.WindowManager$BadTokenException: Unable to add window -- token null is not valid; is your activity running?
stacktrace: java.lang.RuntimeException: Unable to start service dev.davidv.translator.screenTranslate.ScreenTranslateService@50bed5e with Intent { xflg=0x4 cmp=com.akslabs.circletosearch/dev.davidv.translator.screenTranslate.ScreenTranslateService (has extras) }
at android.app.ActivityThread.handleServiceArgs(ActivityThread.java:5740)
at android.app.ActivityThread.-$$Nest$mhandleServiceArgs(Unknown Source:0)
at android.app.ActivityThread$H.handleMessage(ActivityThread.java:2788)
at android.os.Handler.dispatchMessage(Handler.java:132)
at android.os.Looper.dispatchMessage(Looper.java:333)
at android.os.Looper.loopOnce(Looper.java:263)
at android.os.Looper.loop(Looper.java:367)
at android.app.ActivityThread.main(ActivityThread.java:9317)
at java.lang.reflect.Method.invoke(Native Method)
at com.android.internal.os.RuntimeInit$MethodAndArgsCaller.run(RuntimeInit.java:569)
at com.android.internal.os.ZygoteInit.main(ZygoteInit.java:966)
Caused by: android.view.WindowManager$BadTokenException: Unable to add window -- token null is not valid; is your activity running?
at android.view.ViewRootImpl.setView(ViewRootImpl.java:1695)
at android.view.WindowManagerGlobal.addView(WindowManagerGlobal.java:500)
at android.view.WindowManagerImpl.addView(WindowManagerImpl.java:167)
at dev.davidv.translator.screenTranslate.ScreenTranslateService.setupOverlayAndCapture(ScreenTranslateService.kt:298)
at dev.davidv.translator.screenTranslate.ScreenTranslateService.onStartCommand(ScreenTranslateService.kt:204)
at android.app.ActivityThread.handleServiceArgs(ActivityThread.java:5724)
... 10 more