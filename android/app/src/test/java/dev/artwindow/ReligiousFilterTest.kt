package dev.artwindow

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ReligiousFilterTest {

    @Test
    fun `titles naming a religious scene are caught`() {
        assertTrue(isReligious("The Crucifixion", emptyList()))
        assertTrue(isReligious("Madonna and Child", emptyList()))
        assertTrue(isReligious("Christ Crowned with Thorns", emptyList()))
    }

    @Test
    fun `a tag alone is enough to catch a religious scene`() {
        assertTrue(isReligious("Untitled", listOf("Saints")))
    }

    @Test
    fun `secular titles are left alone`() {
        assertFalse(isReligious("View of St. Petersburg", emptyList()))
        assertFalse(isReligious("Still Life with Flowers", emptyList()))
        assertFalse(isReligious("Harbor at Honfleur", emptyList()))
    }

    @Test
    fun `whole-word matching means Christmas is not mistaken for Christ`() {
        assertFalse(isReligious("Winter Evening before Christmas", emptyList()))
    }

    @Test
    fun `Danish religious titles are caught the same way as their English equivalents`() {
        assertTrue(isReligious("Kristus på korset", emptyList()))
        assertTrue(isReligious("Jomfru Maria med Barnet", emptyList()))
        assertTrue(isReligious("En helgen i landskab", emptyList()))
        assertTrue(isReligious("Apostel Peter", emptyList()))
        assertTrue(isReligious("En engel synger", emptyList()))
        assertTrue(isReligious("Korsfæstelse", emptyList()))
    }

    @Test
    fun `madonna already matches a Danish title without a separate Danish entry`() {
        assertTrue(isReligious("Madonna med Barnet", emptyList()))
    }
}
