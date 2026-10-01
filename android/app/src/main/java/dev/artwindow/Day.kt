package dev.artwindow

import java.time.LocalDate
import java.time.LocalDateTime

/**
 * Which day it is, for the rotation: the whole calendar this app has.
 *
 * A day begins at five in the morning on the wall clock, not at midnight — the same
 * rule as `day::DAY_BEGINS` on the desktop. Midnight is often still last night: a
 * painting fetched at half past twelve would be the "old" one waiting in the morning.
 * Everything that asks [State.isDue] or stamps a day asks here for the date, so the
 * hour lives in one place.
 */
object Day {
    const val BEGINS_AT_HOUR = 5L

    /** The date of the day [now] falls in: before five, still yesterday's. */
    fun today(now: LocalDateTime = LocalDateTime.now()): LocalDate =
        now.minusHours(BEGINS_AT_HOUR).toLocalDate()
}
