package dev.artwindow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import java.io.File

class MetNamesTest {

    @Test
    fun `reads the object id back out of a name Met wrote`() {
        assertEquals(436535L, idOf(File("met-436535.jpg")))
    }

    @Test
    fun `rejects a name this source never wrote`() {
        assertNull(idOf(File("IMG_001.jpg")))
    }

    @Test
    fun `rejects a met- name whose remainder is not an id`() {
        assertNull(idOf(File("met-abc.jpg")))
    }
}
