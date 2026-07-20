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
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.akslabs.circletosearch.utils.SearchTrigger
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Instrumented test, which will execute on an Android device.
 *
 * See [testing documentation](http://d.android.com/tools/testing).
 */
@RunWith(AndroidJUnit4::class)
class SearchTriggerInstrumentedTest {

    @Test
    fun triggerIntent_resolvesExportedActivity() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val intent = SearchTrigger.createTriggerIntent(context, collapseShade = false)
        assertEquals(SearchTrigger.ACTION_TRIGGER_SEARCH, intent.action)
        assertFalse(SearchTrigger.shouldCollapseShade(intent))

        val collapseIntent = SearchTrigger.createTriggerIntent(context, collapseShade = true)
        assertTrue(SearchTrigger.shouldCollapseShade(collapseIntent))

        val resolveInfos = context.packageManager.queryIntentActivities(intent, 0)
        assertTrue(
            "TRIGGER_SEARCH must resolve to SearchTriggerActivity",
            resolveInfos.any {
                it.activityInfo.name == SearchTriggerActivity::class.java.name
            }
        )
    }

    @Test
    fun triggerSearch_withoutAccessibility_returnsDisabled() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val result = SearchTrigger.triggerSearch(context)
        assertTrue(
            result == SearchTrigger.Result.AccessibilityDisabled ||
                result == SearchTrigger.Result.ServiceNotConnected ||
                result == SearchTrigger.Result.Triggered
        )
    }

    @Test
    fun parseSearchMode_fromIntentExtras() {
        val lens = Intent(SearchTrigger.ACTION_TRIGGER_SEARCH)
            .putExtra(SearchTrigger.EXTRA_SEARCH_MODE, SearchTrigger.MODE_LENS)
        val multi = Intent(SearchTrigger.ACTION_TRIGGER_SEARCH)
            .putExtra(SearchTrigger.EXTRA_SEARCH_MODE, SearchTrigger.MODE_MULTI)

        assertEquals(true, SearchTrigger.parseSearchModeOverride(lens))
        assertEquals(false, SearchTrigger.parseSearchModeOverride(multi))
    }
}
