# r/macapps

Read the subreddit's rules on self-promotion before posting; they were not checked when
this was drafted. The download link goes straight to GitHub Releases: the site
asks for an email before it sends one, which a Reddit reader will not sit through. Attach one screenshot: a desktop with the painting hung whole and the
menu open.

## Title

I made a menu-bar app that hangs a different museum painting on every Space each day

## Body

I'm the author. Art Window is open source, and the daily painting costs nothing.

**What problem it solves**

I wanted a painting on my desktop that changed by itself, and two things kept going
wrong with what I tried. Paintings were cropped to fill the screen, which cuts off the
part the painter put at the edge. And macOS keeps a separate wallpaper for every Space
on every display, so a new picture only reached the Space I happened to be on.

**What it does**

- Sets one public-domain painting a day, with nothing to search or pick.
- Shows the painting whole, fitted to the screen over black.
- Reaches every Space on every display, not just the one in front of you.
- Draws from about 18,000 paintings: The Met, the National Gallery of Art, Cleveland,
  SMK, the Rijksmuseum and the Getty.
- Keeps favourites, and opens any painting's museum page.
- No account, no ads, no analytics. It downloads the painting and sends nothing.

**Comparison**

- *macOS itself* can fit a picture and show it on all Spaces, but you choose each
  picture by hand.
- *Irvue* and *24 Hour Wallpaper* handle all Spaces too. They show photographs rather
  than paintings.
- *Artpaper* is the closest: hand-picked paintings, about 1,000 of them. It needs
  macOS 26 now; Art Window runs on macOS 11 and later.

**Pricing**

The daily painting costs nothing. A paid licence for extra filters and placement styles
is planned; nothing is locked today.

**Honest caveats**

- The DMG is ad-hoc signed, not notarised, so the first launch needs right-click → Open.
- Reaching every Space means restarting the Dock, which blanks the desktops for a
  moment. It waits for a wake or a login to do that rather than interrupting you.

Download: https://github.com/gr13nka/art-window/releases/latest
Site: https://artwindow.alps-project.online/
Source: https://github.com/gr13nka/art-window

There are also Windows, GNOME and Android versions. I'd like to hear where it breaks.
