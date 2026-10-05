"""The small text rules every source and the writer share, so a byline, a
collapsed field or a maker's name reads the same whichever museum it came from."""

import re


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


# Words that make a maker string something other than "one person painted this":
# a hand that is only suggested, a studio, a school, a culture, an absence.
_NOT_A_SINGLE_PERSON = re.compile(
    r"\b(after|attributed|workshop|studio|follower|circle|school|manner|style|imitator|copy|"
    r"anonymous|unidentified|unknown|various|possibly|probably|artist|master|and|et|with|"
    r"culture|period|dynasty)\b",
    re.IGNORECASE,
)


def maker(raw: str) -> str:
    """The plain personal name a museum's maker string stands for, or `""`.

    Museums fill one field with a person ("Claude Monet"), a person with
    qualifiers ("Claude Monet (French, 1840-1926)", "Follower of Rembrandt"),
    several people, a workshop, or nobody ("Unidentified artist"). Only the
    first of those names a painter someone could choose, and a wrong guess puts
    a stranger's pictures under a chip, so the rule leans toward `""`: the first
    entry of a `|` or `;` list, parentheses dropped, and anything that still
    reads as a qualifier, a group or an absence is refused."""
    name = collapse(re.split(r"[|;]", raw or "")[0])
    name = collapse(re.sub(r"\([^)]*\)|\[[^\]]*\]", " ", name))
    name = name.rstrip(",.").strip()
    if not name or any(c.isdigit() or c in "?&/" for c in name):
        return ""
    if _NOT_A_SINGLE_PERSON.search(name) or len(name.split()) > 5:
        return ""
    # A name is capitalised; "van"/"de"/"da" particles come after the first word.
    if not name[0].isupper():
        return ""
    return name
