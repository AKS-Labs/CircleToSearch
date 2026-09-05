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

package com.akslabs.circletosearch.utils

import android.content.Context
import android.content.SharedPreferences
import com.akslabs.circletosearch.R

class FriendlyMessageManager(context: Context) {
    private val prefs: SharedPreferences = context.getSharedPreferences("friendly_msg_prefs", Context.MODE_PRIVATE)
    private val messages = listOf(
        context.getString(R.string.friendly_msg_1),
        context.getString(R.string.friendly_msg_2),
        context.getString(R.string.friendly_msg_3),
        context.getString(R.string.friendly_msg_4),
        context.getString(R.string.friendly_msg_5),
        context.getString(R.string.friendly_msg_6),
        context.getString(R.string.friendly_msg_7),
        context.getString(R.string.friendly_msg_8),
        context.getString(R.string.friendly_msg_9),
        context.getString(R.string.friendly_msg_10),
        context.getString(R.string.friendly_msg_11),
        context.getString(R.string.friendly_msg_12),
        context.getString(R.string.friendly_msg_13),
        context.getString(R.string.friendly_msg_14),
        context.getString(R.string.friendly_msg_15),
        context.getString(R.string.friendly_msg_16),
        context.getString(R.string.friendly_msg_17),
        context.getString(R.string.friendly_msg_18),
        context.getString(R.string.friendly_msg_19),
        context.getString(R.string.friendly_msg_20),
        context.getString(R.string.friendly_msg_21),
        context.getString(R.string.friendly_msg_22),
        context.getString(R.string.friendly_msg_23),
        context.getString(R.string.friendly_msg_24),
        context.getString(R.string.friendly_msg_25)
    )

    fun getNextMessage(): String {
        val seenIndices = getSeenIndices()
        
        // Find available indices
        val allIndices = messages.indices.toSet()
        val availableIndices = allIndices.subtract(seenIndices).toList()

        if (availableIndices.isEmpty()) {
            // Reset if all seen
            clearSeenIndices()
            val newRandomIndex = messages.indices.random()
            markIndexSeen(newRandomIndex)
            return messages[newRandomIndex]
        }
        
        // Pick random from available
        val pickedIndex = availableIndices.random()
        markIndexSeen(pickedIndex)
        return messages[pickedIndex]
    }

    private fun getSeenIndices(): Set<Int> {
        val seenString = prefs.getString("seen_indices", "") ?: ""
        if (seenString.isEmpty()) return emptySet()
        return seenString.split(",").mapNotNull { it.toIntOrNull() }.toSet()
    }

    private fun markIndexSeen(index: Int) {
        val currentSeen = getSeenIndices().toMutableSet()
        currentSeen.add(index)
        prefs.edit().putString("seen_indices", currentSeen.joinToString(",")).apply()
    }

    private fun clearSeenIndices() {
        prefs.edit().remove("seen_indices").apply()
    }
}
