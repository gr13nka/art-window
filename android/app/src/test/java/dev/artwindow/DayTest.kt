package dev.artwindow

import java.time.LocalDate
import java.time.LocalDateTime
import org.junit.Assert.assertEquals
import org.junit.Test

class DayTest {
    @Test
    fun aDayTurnsOverAtFiveInTheMorningNotAtMidnight() {
        val tenth = LocalDate.of(2026, 6, 10)
        assertEquals(tenth, Day.today(LocalDateTime.of(2026, 6, 10, 23, 59)))
        assertEquals(tenth, Day.today(LocalDateTime.of(2026, 6, 11, 0, 30)))
        assertEquals(tenth, Day.today(LocalDateTime.of(2026, 6, 11, 4, 59)))
        assertEquals(tenth.plusDays(1), Day.today(LocalDateTime.of(2026, 6, 11, 5, 0)))
    }
}
