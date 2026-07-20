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

import com.akslabs.circletosearch.utils.SearchTrigger
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * Local unit tests for SearchTrigger mode parsing and result resolution.
 *
 * See [testing documentation](http://d.android.com/tools/testing).
 */
class SearchTriggerTest {

    @Test
    fun parseSearchModeOverride_mapsLensMultiAndAuto() {
        assertEquals(true, SearchTrigger.parseSearchModeOverride("lens"))
        assertEquals(true, SearchTrigger.parseSearchModeOverride("LENS"))
        assertEquals(true, SearchTrigger.parseSearchModeOverride(" lens "))
        assertEquals(false, SearchTrigger.parseSearchModeOverride("multi"))
        assertEquals(false, SearchTrigger.parseSearchModeOverride("MULTI"))
        assertNull(SearchTrigger.parseSearchModeOverride("auto"))
        assertNull(SearchTrigger.parseSearchModeOverride(null))
        assertNull(SearchTrigger.parseSearchModeOverride(""))
        assertNull(SearchTrigger.parseSearchModeOverride("unknown"))
    }

    @Test
    fun resolveTriggerResult_coversAllBranches() {
        assertEquals(
            SearchTrigger.Result.AccessibilityDisabled,
            SearchTrigger.resolveTriggerResult(
                accessibilityEnabled = false,
                serviceConnected = false
            )
        )
        assertEquals(
            SearchTrigger.Result.AccessibilityDisabled,
            SearchTrigger.resolveTriggerResult(
                accessibilityEnabled = false,
                serviceConnected = true
            )
        )
        assertEquals(
            SearchTrigger.Result.ServiceNotConnected,
            SearchTrigger.resolveTriggerResult(
                accessibilityEnabled = true,
                serviceConnected = false
            )
        )
        assertEquals(
            SearchTrigger.Result.Triggered,
            SearchTrigger.resolveTriggerResult(
                accessibilityEnabled = true,
                serviceConnected = true
            )
        )
    }

    @Test
    fun publicContractConstants_areStable() {
        assertEquals(
            "com.akslabs.circletosearch.action.TRIGGER_SEARCH",
            SearchTrigger.ACTION_TRIGGER_SEARCH
        )
        assertEquals("search_mode", SearchTrigger.EXTRA_SEARCH_MODE)
        assertEquals("collapse_shade", SearchTrigger.EXTRA_COLLAPSE_SHADE)
        assertEquals("auto", SearchTrigger.MODE_AUTO)
        assertEquals("lens", SearchTrigger.MODE_LENS)
        assertEquals("multi", SearchTrigger.MODE_MULTI)
    }
}
