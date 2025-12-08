/*
 *
 *  * Copyright (C) 2025 AKS-Labs (original author)
 *  *
 *  * This program is free software: you can redistribute it and/or modify
 *  * it under the terms of the GNU General Public License as published by
 *  * the Free Software Foundation, either version 3 of the License, or
 *  * (at your option) any later version.
 *  *
 *  * This program is distributed in the hope that it will be useful,
 *  * but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  * GNU General Public License for more details.
 *  *
 *  * You should have received a copy of the GNU General Public License
 *  * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 *
 */

package com.akslabs.circletosearch

import android.service.voice.VoiceInteractionService

/**
 * Main VoiceInteractionService
 * This makes the app appear in Android's assistant selection settings.
 * The actual assist handling is done by AssistSessionService.
 */
class CircleToSearchVoiceService : VoiceInteractionService() {
    override fun onCreate() {
        super.onCreate()
        android.util.Log.d("CircleToSearchVoiceService", "VoiceService Created")
    }

    override fun onReady() {
        super.onReady()
        android.util.Log.d("CircleToSearchVoiceService", "VoiceService Ready")
    }
}
