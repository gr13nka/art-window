package dev.artwindow

import android.app.job.JobInfo
import android.app.job.JobParameters
import android.app.job.JobScheduler
import android.app.job.JobService
import android.content.ComponentName
import android.content.Context
import android.os.Build
import android.os.PersistableBundle
import kotlin.concurrent.thread

/**
 * Runs one [Rotation.turn] in the background, on the OS's own schedule, so a
 * picture can arrive without the app ever being opened.
 *
 * Two job ids, because they answer different questions. [DAILY_JOB_ID] just asks "is
 * a picture owed yet" once an hour on Wi-Fi — cheap, because [Rotation.turn] itself
 * does nothing on the hours a picture is not due. [NOW_JOB_ID] is *Next picture*, or
 * a picture already due at launch, jumping the queue on whatever network is at hand.
 */
class RotationJob : JobService() {
    override fun onStartJob(params: JobParameters): Boolean {
        val force = params.extras.getBoolean(EXTRA_FORCE, false)
        thread(name = "art-window-rotation") {
            try {
                Rotation.turn(applicationContext, force)
            } finally {
                jobFinished(params, false)
            }
        }
        return true
    }

    // The turn cannot be interrupted mid-download and carries on regardless, so there is
    // nothing to reschedule: `false` declines a retry. A turn that then fails leaves the
    // day unspent, and the next hourly run asks again.
    override fun onStopJob(params: JobParameters): Boolean = false

    companion object {
        private const val DAILY_JOB_ID = 1
        private const val NOW_JOB_ID = 2
        private const val EXTRA_FORCE = "force"
        private const val DAILY_INTERVAL_MS = 60 * 60 * 1000L

        /** Ensures the hourly watcher exists. Safe to call on every launch: a job already pending is left alone. */
        fun scheduleDaily(context: Context) {
            val scheduler = context.getSystemService(JobScheduler::class.java)
            if (scheduler.getPendingJob(DAILY_JOB_ID) != null) return
            val job = JobInfo.Builder(DAILY_JOB_ID, ComponentName(context, RotationJob::class.java))
                .setPeriodic(DAILY_INTERVAL_MS)
                .setRequiredNetworkType(JobInfo.NETWORK_TYPE_UNMETERED)
                .setPersisted(true)
                .build()
            scheduler.schedule(job)
        }

        /** Jumps the queue for *Next picture*, or a picture already due at launch. Replaces any pending Now job. */
        fun scheduleNow(context: Context, force: Boolean) {
            val scheduler = context.getSystemService(JobScheduler::class.java)
            val extras = PersistableBundle().apply { putBoolean(EXTRA_FORCE, force) }
            val builder = JobInfo.Builder(NOW_JOB_ID, ComponentName(context, RotationJob::class.java))
                .setRequiredNetworkType(JobInfo.NETWORK_TYPE_ANY)
                .setExtras(extras)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                builder.setExpedited(true)
            } else {
                // No expedited jobs below API 31; the shortest deadline a plain job accepts
                // asks the scheduler to run it as close to immediately as it will allow.
                builder.setOverrideDeadline(0)
            }
            scheduler.schedule(builder.build())
        }
    }
}
