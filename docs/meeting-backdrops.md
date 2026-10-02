# Paintings as meeting backgrounds

`src/backdrop/` keeps a painting nobody has seen yet as the user's virtual
background in a meeting app, so every meeting opens with a new one. Two apps are
reached, both on macOS only: the Zoom desktop client, and Google Meet in Firefox.

Neither has an API for this. Everything below is behaviour observed from outside
on 2026-10-02, on Zoom Workplace 7.2.2, Firefox 156 and macOS 12.7. An update to
either may change any of it, and nothing in the repo would notice.

## The rule

**The slot always holds a painting no meeting has shown, and one meeting shows one
painting.**

A painting is *shown* once the slot has been read since it was written and a
meeting is live. It is replaced when that meeting ends: a spare, already
downloaded and rendered in the form the app wants, is moved into the slot, and the
next spare is prepared. While a meeting is live the loop looks twice a second, and
replacing is a rename, so the new painting is in place before anyone can start the
next meeting.

Replacing a painting the moment it was shown, without waiting for the meeting to
end, was the first version and is wrong for Zoom: the swap landed between the
preview's read and the meeting's, so the meeting opened with a different painting
and spent two.

Both halves of "shown" are needed. The access time alone would count anything
else reading the file as a meeting, and each false alarm would cost a download.
The live check alone says a meeting is running but not that it used the picture.
The live check is only asked once the access time has moved, so nothing is scanned
while the apps are idle.

**The read is seen in the file's access time.** APFS moves the access time on a
read only when it is not already later than the modification time. A file written
with its access time set earlier than its modification time therefore shows the
first read after it, and nothing after that. Hanging a painting sets the times
that way round. Anything that reads the slot to look at it, a `cp` or a `file`,
trips the same wire; `stat` does not.

The worker is written once. What differs between apps is a `Stage`, one per
submodule: where the background lives, what form it must take, and what "live"
means. Each enabled app has its own thread, spare and cache directory, so the two
show different paintings and share nothing.

Being switched on *is* having a slot: `state.json` holds one slot per enabled app
and there is no separate flag.

## Zoom

- **The background is a file.** A picture added under *Settings → Background &
  effects* is copied to
  `~/Library/Application Support/zoom.us/data/VirtualBkgnd_Custom/` as a PNG with a
  UUID for a name and no extension. Which one is selected is recorded in
  `zoomus.enc.db`, which is encrypted and not ours to read.
- **It is read when video starts, every time.** Zoom does not need restarting.
- **One meeting reads it twice.** Once for the preview before joining, and again
  about a second after the meeting itself begins.
- **It is not read again once the meeting is under way.** A file swapped fifteen
  seconds into a call left the live background alone, and the next meeting showed
  the new one.
- **Live means `CptHost` is running.** That process exists for exactly as long as
  a meeting does. `zoom.us`, `caphost` and `aomhost` run whether or not there is
  one, and `caphost`, lower case, is a different process.

The slot is the newest file in `VirtualBkgnd_Custom` at the moment the row is
ticked, and it is overwritten with a 1920×1080 PNG. That is why the user has to
add a picture in Zoom, and select it, first: Art Window cannot register a
background with Zoom, only replace one Zoom already knows. If the user deletes
that background in Zoom, hanging fails and says so in the log; the file is not
recreated, because Zoom would no longer know it.

## Google Meet in Firefox

- **The background is two files in the Firefox profile.** Meet keeps an uploaded
  background in IndexedDB (`meet_fx_db`), and Firefox stores its blobs as plain
  files:
  `~/Library/Application Support/Firefox/Profiles/<profile>/storage/default/https+++meet.google.com/idb/191533160mbede_tx_f.files/<n>`.
  One upload writes an image and a thumbnail at the same moment. In a Firefox
  container the origin directory carries a `^userContextId=…` suffix.
- **A replacement must be exactly as long as the file it replaces.** The length
  of each blob is recorded in the database, which is not ours to edit while
  Firefox runs.
- **Firefox decodes by content.** Meet writes lossless WebP and records
  `image/webp`, but a JPEG in that file is shown. So a painting is encoded as a
  JPEG that fits and padded to the exact length with `COM` segments straight after
  the SOI marker. That placement is the one that was verified.
- **It can be replaced while Firefox runs.**
- **Meet reads it once per call**, a few seconds after the camera comes on, and
  never again in that call: not when the camera is turned off and on, and a file
  swapped mid-call does not change the live background.
- **Live means a camera is in use**, by anything. A browser has no process that
  lasts exactly as long as a call. The access time alone is not enough: the file
  was once seen being read with no call in progress.

The slot is the two newest files in that directory when the row is ticked, the
larger being the image. Of several profiles or containers, the one whose image is
newest wins. The user has to upload a picture as their background in Meet, and
select it, first.

**The size of that upload is the budget for every painting after it.** The image
is tried at 1920×1080, then 1280×720, then 960×540, at falling JPEG quality, until
it fits. A 234 KB upload gets paintings at about 1280×720. An upload under 100 KB
is refused when the row is ticked, with a message asking for a larger one.

If the user changes their background in Meet, the lengths stop matching, hanging
refuses, the spare is thrown away and the next one is rendered for the new
lengths.

Turning the camera off in the middle of a call counts as the call ending and
replaces the stored painting. Nobody sees that until the next call.

## What it leaves alone

- **The wallpaper and the day.** A meeting background never touches
  `state.shown`, `state.fetched` or the clock.
- **The wallpaper's cache.** Sources are reused as they are, but handed
  `cache/backdrop/<app>/` as their directory, so their downloads and their own
  sweep stay in there.
- **The event loop's clock.** Each app runs on one thread of its own and the tray
  only starts and stops it. A failure cools off for fifteen minutes of wall-clock
  time, for the same reason the rotation's failures do.

## Not yet verified

- Windows and Linux, for either app. Zoom's folders are known
  (`%APPDATA%\Zoom\data\VirtualBkgnd_Custom`, `~/.zoom/data/VirtualBkgnd_Custom`)
  but nobody has watched Zoom read them, and NTFS and ext4 do not treat access
  times the way APFS does. The menu rows are hidden there.
- Chrome, and every other browser. Each stores IndexedDB blobs its own way.
- What read Meet's file once with no call in progress. The camera check is there
  so that it does not matter.
- A Zoom meeting joined with video off, and video turned on later in the same
  meeting.
- A meeting that starts and ends while the next spare is still downloading: its
  painting is not replaced until the meeting after.
- An external camera. The camera check asks CoreMediaIO about every device, but
  only the built-in one was tried.
