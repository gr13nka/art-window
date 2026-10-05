# r/gnome

Read the subreddit's rules on self-promotion before posting; they were not checked when
this was drafted. The download link goes straight to GitHub Releases: the site
asks for an email before it sends one, which a Reddit reader will not sit through. Attach one screenshot: the GTK window beside a desktop with the
painting hung whole.

A commenter on an earlier r/gnome launch brought up Flathub's rule about AI-made
software. Decide how you answer that question before posting; it will be asked.

## Title

I made a GTK app that sets a different museum painting as the GNOME wallpaper each day

## Body

I'm the author. Art Window is open source (MIT or Apache-2.0), and the daily painting
costs nothing.

**What it does**

- Sets one public-domain painting a day, with nothing to search or pick.
- Shows the painting whole, fitted to the screen over black. It sets the light and the
  dark wallpaper together.
- Draws from about 18,000 paintings: The Met, the National Gallery of Art, Cleveland,
  SMK, the Rijksmuseum and the Getty.
- Keeps favourites, and opens any painting's museum page.
- Notices the machine waking, so the picture is right when you open the lid on a new day.
- No account, no ads, no analytics.

**How it sits in GNOME**

It is an application, not a shell extension. One GTK window is the whole interface. A
panel icon appears only if you already have an AppIndicator extension; stock GNOME works
without one.

**Install**

A tarball with a prebuilt x86_64 binary and an `install.sh` that puts it in
`~/.local/bin` with a launcher and an icon. There is no Flatpak yet.

**Pricing**

The daily painting costs nothing. A paid licence for extra filters and placement styles
is planned; nothing is locked today.

Download: https://github.com/gr13nka/art-window/releases/latest
Site: https://artwindow.alps-project.online/
Source: https://github.com/gr13nka/art-window

It has only been tested on GNOME. I'd like to hear what happens elsewhere.
