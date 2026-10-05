# Data safety form and permissions

## What was verified

Network access is in one place. The only `HttpURLConnection` in the app is `java/dev/artwindow/Museums.kt:193-196` (`openConnection`), which GETs `entry.imageUrl` and sets only a `User-Agent` header (`Museums.kt:199`, value at `Museums.kt:248`). It sends no body, no cookies of its own making, no identifier and no user data. A grep for `URL(`, `openConnection` and `https?://` across `java/` found no other request. All paths below are under `android/app/src/main/`.

- Hosts contacted are the image hosts named in `catalogue/dist/paintings.tsv`: images.metmuseum.org, openaccess-cdn.clevelandart.org, www.nga.gov / api.nga.gov, iip.smk.dk, iiif.micr.io (the Rijksmuseum's image host) and media.getty.edu (counts taken from the file's URL columns).
- The only other outward step is `MainActivity.kt:371`, which hands a museum page URL to the browser with `Intent.ACTION_VIEW` when the user taps "See it at ...". The app itself makes no request there.
- No analytics, ads or crash-reporting SDK: `app/build.gradle.kts` dependencies are AndroidX, Compose, coroutines and JUnit only.
- Local storage: two private `SharedPreferences` files (`State.kt:17`, `WallpaperPreferences.kt:113`) hold the shown painting, the day and the user's settings. Favourites are files in the app's own storage. None of it is sent anywhere.
- `android:allowBackup="true"` (`AndroidManifest.xml:17`). Android may back this local data up to the user's own Google account. That is the platform's backup, not data the developer collects.

Not verified: what the museums' servers log on their side (IP address, `User-Agent`). The developer receives none of it.

## Data safety form answers

| Question | Answer |
|---|---|
| Does the app collect or share any of the required user data types? | No |
| Is all user data encrypted in transit? | Not applicable. All downloads use https (check: no `http://` URL in the catalogue). Answer "Yes" if the form requires one. |
| Can users request data deletion? | Not applicable. No data is collected. Uninstalling removes local data. |
| Data collected, every category | None |
| Data shared, every category | None |
| Account creation | None |
| Ads | None (Console "Contains ads": No) |

## Permissions rationale

| Permission | Why | Where |
|---|---|---|
| `INTERNET` | Downloads the day's painting from the museum that holds it. | `Museums.kt:193` |
| `ACCESS_NETWORK_STATE` | The daily jobs carry a network constraint, and Android 16 refuses to schedule one without it. | `AndroidManifest.xml:8`, `RotationJob.kt:52` |
| `SET_WALLPAPER` | Sets the painting as the home and lock screen wallpaper. | `Wallpaper.kt:215-219` |
| `RECEIVE_BOOT_COMPLETED` | Lets the hourly watcher survive a restart (`setPersisted(true)`), so a new painting still arrives after a reboot. | `RotationJob.kt:53` |
| `BIND_DREAM_SERVICE` (service guard) | Lets only the system start the TV screensaver. Not a permission the app requests. | `AndroidManifest.xml:49` |
| `BIND_JOB_SERVICE` (service guard) | Lets only the system start the rotation job. Not requested by the app. | `AndroidManifest.xml:63` |

No sensitive permission (location, contacts, storage, camera, microphone, notifications, accessibility) is declared, so no permission declaration form should be needed.

## Privacy policy

Play needs a policy URL. The site has one at artwindow.alps-project.online (the `#privacy` sheet in `site/index.html`). Check its text matches the above before pasting the link.
