package dev.artwindow

/** What the phone's artwork screen and the television's overlay both say about [Rotation]'s progress. */
internal fun statusLine(status: Status, owed: Boolean): String = when (status) {
    is Status.Fetching -> fetchingStatusLine(status.progress)
    is Status.Applying -> "Applying wallpaper…"
    is Status.SavingFavourite -> "Updating favourites…"
    is Status.Failed -> status.message
    is Status.Idle -> if (owed) "Today's painting arrives on the next Wi-Fi check" else "A new painting arrives tomorrow"
}

internal fun fetchingStatusLine(progress: FetchProgress?): String = when (progress) {
    null -> "Fetching…"
    is FetchProgress.Searching -> "Choosing a painting…"
    is FetchProgress.Downloading -> {
        val done = progress.bytes / BYTES_PER_MB
        val total = progress.total
        if (total != null && total > 0) {
            "Downloading %.1f of %.1f MB…".format(done, total / BYTES_PER_MB)
        } else {
            "Downloading %.1f MB…".format(done)
        }
    }
}

/** True while [Rotation] holds the desktop-equivalent lock, so a second request would be refused. */
internal val Status.isBusy: Boolean
    get() = this is Status.Fetching || this is Status.Applying || this is Status.SavingFavourite

private const val BYTES_PER_MB = 1024.0 * 1024.0
