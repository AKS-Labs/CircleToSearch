  I  com.akslabs.circletosearch:9a3f8c78: onCancelled at PHASE_CLIENT_ALREADY_HIDDEN
2026-01-12 02:17:16.624 29031-29031 HWUI                    com.akslabs.circletosearch           W  Image decoding logging dropped!
2026-01-12 02:17:25.214 29031-29031 CircleToSearchTile      com.akslabs.circletosearch           D  Tile clicked. Service enabled: false
2026-01-12 02:17:25.228 29031-29031 AndroidRuntime          com.akslabs.circletosearch           D  Shutting down VM
2026-01-12 02:17:25.241 29031-29031 AndroidRuntime          com.akslabs.circletosearch           E  FATAL EXCEPTION: main (Fix with AI)
                                                                                                    Process: com.akslabs.circletosearch, PID: 29031
                                                                                                    java.lang.UnsupportedOperationException: startActivityAndCollapse: Starting activity from TileService using an Intent is not allowed.
                                                                                                    	at android.service.quicksettings.TileService.startActivityAndCollapse(TileService.java:355)
                                                                                                    	at com.akslabs.circletosearch.CircleToSearchTileService.onClick(CircleToSearchTileService.kt:33)
                                                                                                    	at android.service.quicksettings.TileService$H.handleMessage(TileService.java:494)
                                                                                                    	at android.os.Handler.dispatchMessage(Handler.java:109)
                                                                                                    	at android.os.Looper.loopOnce(Looper.java:232)
                                                                                                    	at android.os.Looper.loop(Looper.java:317)
                                                                                                    	at android.app.ActivityThread.main(ActivityThread.java:8929)
                                                                                                    	at java.lang.reflect.Method.invoke(Native Method)
                                                                                                    	at com.android.internal.os.RuntimeInit$MethodAndArgsCaller.run(RuntimeInit.java:591)
                                                                                                    	at com.android.internal.os.ZygoteInit.main(ZygoteInit.java:911)
2026-01-12 02:17:25.271 29031-29031 Process                 com.akslabs.circletosearch           I  Sending signal. PID: 29031 SIG: 9
---------------------------- PROCESS ENDED (29031) for package com.akslabs.circletosearch ----------------------------
2026-01-12 02:17:25.288  1981-2383  InputDispatcher         system_server                        E  channel '3901bb2 com.akslabs.circletosearch' ~ Channel is unrecoverably broken and will be disposed!
---------------------------- PROCESS STARTED (29485) for package com.akslabs.circletosearch ----------------------------
2026-01-12 02:17:30.578 29485-29485 nativeloader            com.akslabs.circletosearch           D