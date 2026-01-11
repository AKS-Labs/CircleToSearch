  D  Installing profile for com.akslabs.circletosearch
2026-01-12 01:02:33.855 32197-32197 CircleToSearchTile      com.akslabs.circletosearch           D  Tile clicked. Service enabled: true
2026-01-12 01:02:33.857 32197-32197 AndroidRuntime          com.akslabs.circletosearch           D  Shutting down VM
2026-01-12 01:02:33.859 32197-32197 AndroidRuntime          com.akslabs.circletosearch           E  FATAL EXCEPTION: main (Fix with AI)
                                                                                                    Process: com.akslabs.circletosearch, PID: 32197
                                                                                                    java.lang.SecurityException: Permission Denial: android.intent.action.CLOSE_SYSTEM_DIALOGS broadcast from com.akslabs.circletosearch (pid=32197, uid=10422) requires android.permission.BROADCAST_CLOSE_SYSTEM_DIALOGS.
                                                                                                    	at android.os.Parcel.createExceptionOrNull(Parcel.java:3011)
                                                                                                    	at android.os.Parcel.createException(Parcel.java:2995)
                                                                                                    	at android.os.Parcel.readException(Parcel.java:2978)
                                                                                                    	at android.os.Parcel.readException(Parcel.java:2920)
                                                                                                    	at android.app.IActivityManager$Stub$Proxy.broadcastIntentWithFeature(IActivityManager.java:5135)
                                                                                                    	at android.app.ContextImpl.sendBroadcast(ContextImpl.java:1193)
                                                                                                    	at android.content.ContextWrapper.sendBroadcast(ContextWrapper.java:500)
                                                                                                    	at com.akslabs.circletosearch.CircleToSearchTileService.onClick(CircleToSearchTileService.kt:27)
                                                                                                    	at android.service.quicksettings.TileService$H.handleMessage(TileService.java:467)
                                                                                                    	at android.os.Handler.dispatchMessage(Handler.java:106)
                                                                                                    	at android.os.Looper.loopOnce(Looper.java:201)
                                                                                                    	at android.os.Looper.loop(Looper.java:288)
                                                                                                    	at android.app.ActivityThread.main(ActivityThread.java:7933)
                                                                                                    	at java.lang.reflect.Method.invoke(Native Method)
                                                                                                    	at com.android.internal.os.RuntimeInit$MethodAndArgsCaller.run(RuntimeInit.java:548)
                                                                                                    	at com.android.internal.os.ZygoteInit.main(ZygoteInit.java:843)
                                                                                                    Caused by: android.os.RemoteException: Remote stack trace:
                                                                                                    	at com.android.server.wm.ActivityTaskManagerService.checkCanCloseSystemDialogs(ActivityTaskManagerService.java:3136)
                                                                                                    	at com.android.server.wm.ActivityTaskManagerService$LocalService.checkCanCloseSystemDialogs(ActivityTaskManagerService.java:5717)
                                                                                                    	at com.android.server.am.ActivityManagerService.broadcastIntentLocked(ActivityManagerService.java:14098)
                                                                                                    	at com.android.server.am.ActivityManagerService.broadcastIntentLocked(ActivityManagerService.java:13641)
                                                                                                    	at com.android.server.am.ActivityManagerService.broadcastIntentWithFeature(ActivityManagerService.java:14519)
