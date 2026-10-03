"""The two small text rules every source and the writer share, so a byline or a
collapsed field reads the same whichever museum it came from."""


def collapse(text: str | None) -> str:
    """`text` with every run of whitespace — tabs and newlines included, which
    would otherwise break a TSV row — reduced to one space."""
    return " ".join((text or "").split())


def byline(artist: str, date: str) -> str:
    """"Gilbert Stuart, 1789", or whichever half there is."""
    artist, date = artist.strip(), date.strip()
    if artist and date:
        return f"{artist}, {date}"
    return artist or date
