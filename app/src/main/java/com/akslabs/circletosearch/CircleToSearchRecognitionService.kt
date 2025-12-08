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

import android.content.Intent
import android.speech.RecognitionService

class CircleToSearchRecognitionService : RecognitionService() {
    override fun onStartListening(intent: Intent?, callback: Callback?) {
        android.util.Log.d("CircleToSearchRecog", "onStartListening")
    }

    override fun onStopListening(callback: Callback?) {
        android.util.Log.d("CircleToSearchRecog", "onStopListening")
    }

    override fun onCancel(callback: Callback?) {
        android.util.Log.d("CircleToSearchRecog", "onCancel")
    }
}
