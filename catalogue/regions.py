"""Region classification shared by every catalogue source.

Maps a source's own geography text (department, country, culture,
nationality) to one of the six `ArtworkRegion` names the apps already
recognize — see `ArtworkRegion` in
`android/app/src/main/java/dev/artwindow/WallpaperPreferences.kt` and the
matching enum in `src/art/museums.rs`. This is a geography lookup only: no
subject, portrait, religious or shape judgment lives here, or anywhere in
`catalogue/` — those stay in the apps that actually render a picture.

`region_from_text` and `REGION_KEYWORDS` started as
`android/catalogue/build.py`'s table over the Met's CSV; this module is where
that logic now lives, shared by all four sources rather than duplicated.
"""

import re

REGIONS = ("EUROPE", "ASIA", "AFRICA", "NORTH_AMERICA", "SOUTH_AMERICA", "OCEANIA")

# A short, obvious keyword table matched case-insensitively as a whole word
# against whatever geography text a source has (department, country, culture,
# nationality). The earliest keyword match in the text wins, so "American,
# born Egypt" reads as American (the nationality), not Egyptian (the
# birthplace).
REGION_KEYWORDS: dict[str, list[str]] = {
    "EUROPE": [
        "italian", "italy", "french", "france", "british", "english", "britain",
        "dutch", "netherlands", "flemish", "flanders", "german", "germany",
        "spanish", "spain", "danish", "denmark", "swedish", "sweden",
        "norwegian", "norway", "swiss", "switzerland", "austrian", "austria",
        "belgian", "belgium", "portuguese", "portugal", "greek", "greece",
        "russian", "russia", "polish", "poland", "hungarian", "hungary",
        "irish", "ireland", "scottish", "scotland", "finnish", "finland",
        "czech", "europe", "european",
    ],
    "ASIA": [
        "chinese", "china", "japanese", "japan", "korean", "korea", "indian",
        "india", "persian", "iran", "turkish", "turkey", "thai", "thailand",
        "vietnamese", "vietnam", "indonesian", "indonesia", "mongolian",
        "mongolia", "tibetan", "tibet", "asia", "asian",
    ],
    "AFRICA": [
        "egyptian", "egypt", "moroccan", "morocco", "ethiopian", "ethiopia",
        "nigerian", "nigeria", "african", "africa", "south african",
        "south africa", "algerian", "algeria", "tunisian", "tunisia",
        "sudanese", "sudan", "ghanaian", "ghana", "congolese", "congo",
        "kenyan", "kenya",
    ],
    "NORTH_AMERICA": [
        "american", "america", "united states", "usa", "canadian", "canada",
        "mexican", "mexico", "north america",
    ],
    "SOUTH_AMERICA": [
        "brazilian", "brazil", "peruvian", "peru", "argentine", "argentina",
        "colombian", "colombia", "chilean", "chile", "south america",
        "south american", "latin american", "uruguayan", "uruguay",
        "venezuelan", "venezuela", "ecuadorian", "ecuador", "bolivian",
        "bolivia", "paraguayan", "paraguay", "cuzco", "cusco", "quito",
    ],
    "OCEANIA": [
        "australian", "australia", "new zealand", "oceania", "polynesian",
        "melanesian", "hawaiian", "aboriginal", "maori", "papua",
        "new zealander",
    ],
}

# Departments that map straight to a region regardless of country/culture —
# these hold nearly all the public-domain paintings in the Met's collection.
MET_DEPARTMENT_REGIONS = {
    "European Paintings": "EUROPE",
    "Robert Lehman Collection": "EUROPE",
    "Asian Art": "ASIA",
    "Islamic Art": "ASIA",
}


def region_from_text(text: str | None) -> str | None:
    """The region whose keyword appears earliest in `text`, or `None` if none
    do. `text` is normally one field (a nationality, a culture) — call it
    once per candidate field and take the first non-`None` result, in order
    of how much that source trusts the field."""
    if not text:
        return None
    lowered = text.lower()
    best_region = None
    best_index = None
    for region, keywords in REGION_KEYWORDS.items():
        for keyword in keywords:
            match = re.search(r"\b" + re.escape(keyword) + r"\b", lowered)
            if match and (best_index is None or match.start() < best_index):
                best_index = match.start()
                best_region = region
    return best_region


def met_department_region(department: str | None) -> str | None:
    return MET_DEPARTMENT_REGIONS.get(department or "")


# SMK's API never translates `production[].creator_nationality` to English —
# asking it to (`lang=en`) silently breaks the `object_names:maleri` filter
# that selects paintings at all, verified against the live API (see the note
# in `sources/smk.py`). So nationality arrives in Danish, and this is the
# small demonym table that reads it. It is not exhaustive; SMK is Denmark's
# national gallery, so a nationality this table doesn't recognise still
# defaults to EUROPE in `region_from_danish_nationality` below, rather than
# dropping an otherwise-qualifying painting for want of a translation.
DANISH_NATIONALITY_TO_ENGLISH = {
    "dansk": "danish",
    "engelsk": "english",
    "britisk": "british",
    "fransk": "french",
    "tysk": "german",
    "italiensk": "italian",
    "hollandsk": "dutch",
    "nederlandsk": "dutch",
    "flamsk": "flemish",
    "spansk": "spanish",
    "svensk": "swedish",
    "norsk": "norwegian",
    "finsk": "finnish",
    "russisk": "russian",
    "polsk": "polish",
    "østrigsk": "austrian",
    "belgisk": "belgian",
    "portugisisk": "portuguese",
    "græsk": "greek",
    "schweizisk": "swiss",
    "amerikansk": "american",
    "kinesisk": "chinese",
    "japansk": "japanese",
    "indisk": "indian",
    "koreansk": "korean",
    "tyrkisk": "turkish",
    "egyptisk": "egyptian",
    "brasiliansk": "brazilian",
    "mexicansk": "mexican",
    "australsk": "australian",
}


def region_from_danish_nationality(nationality: str | None) -> str | None:
    if not nationality:
        return None
    english = DANISH_NATIONALITY_TO_ENGLISH.get(nationality.strip().lower())
    region = region_from_text(english) if english else None
    return region or "EUROPE"
