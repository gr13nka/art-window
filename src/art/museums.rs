//! A prebuilt list of paintings from four museums plus Wikimedia Commons, no live
//! search.
//!
//! `catalogue/build.py` — a separate, offline pipeline — walks the Met, the
//! National Gallery of Art (Washington), the Cleveland Museum of Art and SMK
//! (Denmark), keeps only what is public domain, catalogued as a painting and at
//! least `MIN_LONG_SIDE` pixels on its long side, and writes one row per painting
//! to `catalogue/dist/paintings.tsv`. That file is checked in and compiled
//! straight into this binary: a day's painting is a local pick out of an
//! already-verified list and exactly one download, never a live search. A
//! Wikimedia Commons row (`wmc`) carries a twelfth column naming the artist;
//! [`parse_line`] accepts and discards it — picking by artist is an
//! Android-only feature, not a desktop one.
//!
//! Subject, portrait and shape decisions are deliberately not the build's to make
//! — see "Deliberate omissions" in `CLAUDE.md` — so this module applies the same
//! landscape-subject and portrait rules [`met`](super::met) does, on the title and
//! tags the catalogue carries — widened with the Danish words SMK's records use,
//! since `met` never sees a Danish title to widen for.
//!
//! The Android app reads the same TSV from its own assets and keeps its own copy
//! of the subject rules, including the Danish words, in `Catalogue.kt` — see
//! `docs/android.md` — and the iOS app a third, in `ios/ArtWindowKit/Catalogue.swift`
//! (`docs/ios.md`). A word-list change belongs in all three.

use super::http;
use super::{pick_index, Artwork, Source};
use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The list `catalogue/build.py` writes. Never hand-edited — a stale or malformed
/// row here is a bug in that pipeline, not something this module should try to
/// repair, which is why a bad row is simply skipped rather than reported.
const CATALOGUE_TEXT: &str = include_str!("../../catalogue/dist/paintings.tsv");

/// How many entries to try before giving up. A download can fail for reasons that
/// have nothing to do with the catalogue being wrong — a dead link, a host that is
/// briefly unreachable — so a source with thousands of candidates simply moves on.
const CANDIDATES: usize = 3;

/// One of the museums — or Wikimedia Commons — a row in the catalogue can name.
///
/// The word actually written in the TSV, and the words this module shows a person,
/// are two different concerns — [`MuseumSource::parse`] and
/// [`MuseumSource::attribution`] each own one of them, the way [`super::SourceSpec`]
/// owns the spelling of `config.toml` and nothing else does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MuseumSource {
    Met,
    Nga,
    Cma,
    Smk,
    /// Wikimedia Commons: the artist-attributed rows, carrying the twelfth
    /// `artist` column the other four leave empty.
    Wmc,
}

impl MuseumSource {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "met" => Some(Self::Met),
            "nga" => Some(Self::Nga),
            "cma" => Some(Self::Cma),
            "smk" => Some(Self::Smk),
            "wmc" => Some(Self::Wmc),
            _ => None,
        }
    }

    /// The spelling used in the TSV and in a downloaded file's name — the same
    /// word in both places so [`key_of`] can read one back out of the other.
    fn key(self) -> &'static str {
        match self {
            Self::Met => "met",
            Self::Nga => "nga",
            Self::Cma => "cma",
            Self::Smk => "smk",
            Self::Wmc => "wmc",
        }
    }

    fn attribution(self) -> &'static str {
        match self {
            Self::Met => "The Metropolitan Museum of Art",
            Self::Nga => "National Gallery of Art, Washington",
            Self::Cma => "Cleveland Museum of Art",
            Self::Smk => "SMK – National Gallery of Denmark",
            Self::Wmc => "Wikimedia Commons",
        }
    }
}

/// One row of the catalogue, minus the columns nothing on the desktop reads
/// (region, pixel size, origin — no shape or region filtering here, per the
/// "Deliberate omissions" in `CLAUDE.md`). Still validated against the full
/// eleven- or twelve-column contract in [`parse_line`], so a row truncated or
/// reordered upstream is skipped rather than silently misread.
struct Entry {
    source: MuseumSource,
    id: String,
    image_url: String,
    details_url: String,
    title: String,
    byline: String,
    tags: Vec<String>,
}

/// The whole catalogue, parsed once and kept for the life of the process — it
/// never changes underneath a running instance, being compiled in rather than
/// read from disk.
fn catalogue() -> &'static [Entry] {
    static ENTRIES: OnceLock<Vec<Entry>> = OnceLock::new();
    ENTRIES.get_or_init(|| parse_catalogue(CATALOGUE_TEXT))
}

fn parse_catalogue(text: &str) -> Vec<Entry> {
    text.lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(parse_line)
        .collect()
}

/// Parses one `source id region width height image_url details_url title byline
/// origin tags` row, or `None` if it does not honour that contract.
///
/// A twelfth column is accepted and discarded: `wmc` rows carry an artist name
/// there for Android's benefit (see `docs/android.md`), but nothing on the
/// desktop picks by artist, so there is nothing here to keep it for.
fn parse_line(line: &str) -> Option<Entry> {
    let fields: Vec<&str> = line.split('\t').collect();
    let (source, id, _region, width, height, image_url, details_url, title, byline, _origin, tags) =
        match fields[..] {
            [source, id, region, width, height, image_url, details_url, title, byline, origin, tags]
            | [source, id, region, width, height, image_url, details_url, title, byline, origin, tags, _] => {
                (
                    source,
                    id,
                    region,
                    width,
                    height,
                    image_url,
                    details_url,
                    title,
                    byline,
                    origin,
                    tags,
                )
            }
            _ => return None,
        };

    let source = MuseumSource::parse(source)?;
    // The filename convention below reads the id back out of a single `-`-joined
    // segment; a source-supplied id containing a slash or whitespace would break
    // that, so such a row is malformed rather than merely unusual.
    if id.is_empty() || id.contains(['/', '\\']) || id.contains(char::is_whitespace) {
        return None;
    }
    // Unused past this check, but the contract promises real pixels here, so a row
    // that fails to parse one is as malformed as a missing column.
    width.parse::<u32>().ok()?;
    height.parse::<u32>().ok()?;
    if image_url.is_empty() {
        return None;
    }

    Some(Entry {
        source,
        id: id.to_owned(),
        image_url: image_url.to_owned(),
        details_url: details_url.to_owned(),
        title: title.to_owned(),
        byline: byline.to_owned(),
        tags: if tags.is_empty() {
            Vec::new()
        } else {
            tags.split('|').map(str::to_owned).collect()
        },
    })
}

/// Whether `text` contains `word`, or its plain `s`-plural, as a whole word —
/// case-insensitively, so "Landscapes" and "LANDSCAPE" both count but
/// "landscaping" does not.
fn mentions_word(text: &str, word: &str) -> bool {
    let plural = format!("{word}s");
    text.split(|c: char| !c.is_alphanumeric())
        .any(|w| w.eq_ignore_ascii_case(word) || w.eq_ignore_ascii_case(&plural))
}

/// Whether `text` contains `phrase` as a run of consecutive whole words —
/// case-insensitively, and with no plural form, unlike [`mentions_word`]: none of
/// the phrases this module checks take one.
fn mentions_phrase(text: &str, phrase: &str) -> bool {
    let words: Vec<&str> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let phrase_words: Vec<&str> = phrase.split_whitespace().collect();
    !phrase_words.is_empty()
        && words.windows(phrase_words.len()).any(|window| {
            window
                .iter()
                .zip(&phrase_words)
                .all(|(w, p)| w.eq_ignore_ascii_case(p))
        })
}

/// The single-word landscape terms checked against a catalogue entry's title and
/// tags. `"landscape"` mirrors the same subject [`met::SEARCH`](super::met)'s live
/// query narrows to, widened here to include the plural the same way its follow-up
/// metadata check does: a search for landscapes still turns up paintings of people
/// standing in one.
///
/// The rest are Danish: SMK's records come back in Danish (see `docs/android.md`),
/// so a plain SMK title never contains the English word at all. Each Danish
/// irregular plural is its own entry ("landskab", "landskaber") since
/// [`mentions_word`]'s "+s" rule does not form it. Danish "by" (town) is
/// deliberately absent — it collides with the English preposition "by" and would
/// flood the catalogue's English-language titles with false matches; see
/// `CLAUDE.md`'s External services section.
const LANDSCAPE_WORDS: &[&str] = &[
    "landscape",
    "landskab",
    "landskaber",
    "udsigt",
    "gade",
    "gader",
];

/// Landscape phrases that only make sense as a run of words, checked with
/// [`mentions_phrase`] instead. "Parti fra" ("view from") is Danish.
const LANDSCAPE_PHRASES: &[&str] = &["parti fra"];

fn is_landscape(entry: &Entry) -> bool {
    let texts: Vec<&str> = std::iter::once(entry.title.as_str())
        .chain(entry.tags.iter().map(String::as_str))
        .collect();
    texts
        .iter()
        .any(|t| LANDSCAPE_WORDS.iter().any(|w| mentions_word(t, w)))
        || texts
            .iter()
            .any(|t| LANDSCAPE_PHRASES.iter().any(|p| mentions_phrase(t, p)))
}

/// The same rule as `met::Object::is_portrait`, plus Danish "portræt" for SMK's
/// titles — checked as a substring like "portrait" is, rather than through
/// [`mentions_word`], since [`met::Object::is_portrait`](super::met) uses the same
/// substring form and the two are meant to stay in step.
fn is_portrait(entry: &Entry) -> bool {
    let title = entry.title.to_lowercase();
    title.contains("portrait")
        || title.contains("portræt")
        || entry
            .tags
            .iter()
            .any(|t| t.eq_ignore_ascii_case("portraits"))
}

/// Landscape, non-portrait entries, minus whichever one `avoid` names.
fn landscape_candidates<'a>(
    entries: &'a [Entry],
    avoid: Option<(MuseumSource, &str)>,
) -> Vec<&'a Entry> {
    entries
        .iter()
        .filter(|e| is_landscape(e) && !is_portrait(e))
        .filter(|e| avoid != Some((e.source, e.id.as_str())))
        .collect()
}

/// Recovers `(source, id)` from a file this source downloaded, or `None` if the
/// file came from somewhere else — `met-{id}.{ext}` included, which is exactly the
/// point: the distinct `museums-` prefix is what keeps this source and
/// [`met`](super::met) from ever claiming the same file as their own to delete.
///
/// Private, like `met::id_of`: recognising this source's own work is knowledge
/// that has no business leaving this file. Reading straight off the file stem
/// means a favourite copy — which keeps its original name wherever it is moved —
/// is still recognised, and still avoided by tomorrow's pick.
fn key_of(path: &Path) -> Option<(MuseumSource, &str)> {
    let stem = path.file_stem()?.to_str()?;
    let (source, id) = stem.strip_prefix("museums-")?.split_once('-')?;
    if id.is_empty() {
        return None;
    }
    Some((MuseumSource::parse(source)?, id))
}

pub struct Museums {
    agent: ureq::Agent,
    /// Where downloads go, and the only directory this source will delete from.
    cache: PathBuf,
}

impl Museums {
    pub fn new(cache: PathBuf) -> Self {
        Self {
            agent: http::agent(),
            cache,
        }
    }

    fn download(&self, entry: &Entry) -> Result<PathBuf> {
        let extension = http::extension_from_url(&entry.image_url);
        // Load-bearing: `key_of` reads the source and id back out of this name.
        let path = self.cache.join(format!(
            "museums-{}-{}.{extension}",
            entry.source.key(),
            entry.id
        ));
        http::download(&self.agent, &entry.image_url, &path)?;
        Ok(path)
    }
}

impl Source for Museums {
    fn fetch(&self, avoid: Option<&Artwork>) -> Result<Artwork> {
        let avoid = avoid.and_then(|a| key_of(&a.path));
        let pool = landscape_candidates(catalogue(), avoid);
        if pool.is_empty() {
            return Err(anyhow!("no landscape paintings in the catalogue"));
        }

        let mut last_error = None;
        for attempt in 0..CANDIDATES {
            let entry = pool[pick_index(pool.len(), attempt as u64)];
            match self.download(entry) {
                Ok(path) => {
                    return Ok(Artwork {
                        title: if entry.title.trim().is_empty() {
                            "Untitled".to_owned()
                        } else {
                            entry.title.clone()
                        },
                        byline: entry.byline.clone(),
                        attribution: entry.source.attribution().to_owned(),
                        details_url: if entry.details_url.is_empty() {
                            None
                        } else {
                            Some(entry.details_url.clone())
                        },
                        path,
                    })
                }
                Err(e) => last_error = Some(e),
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow!("no usable painting in {CANDIDATES} attempts")))
    }

    fn label(&self) -> &'static str {
        "the museum catalogue"
    }

    /// Deletes yesterday's downloads, and only those: a file is this source's to
    /// remove exactly when `key_of` recognises its name. Anything else in the
    /// directory — including a `met-{id}.{ext}` left by the other source — belongs
    /// to somebody else and is left alone.
    fn discard_all_but(&self, keep: &Path) {
        let Ok(entries) = std::fs::read_dir(&self.cache) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path != keep && key_of(&path).is_some() {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = "\
# generated 2026-01-01 met=2026-01-01 nga=2026-01-01 cma=2026-01-01 smk=2026-01-01
met\t1\tEUROPE\t3000\t2000\thttps://example.com/met1.jpg\thttps://example.com/details/1\tLandscape near the Rhine\tAnonymous, 1800\tGermany\tLandscapes|Rivers
nga\t2\tEUROPE\t4000\t3000\thttps://example.com/nga2.jpg\thttps://example.com/details/2\tLandscapes at Dusk\tJ. Smith, 1850\tFrance\t
smk\t3\tEUROPE\t2500\t3200\thttps://example.com/smk3.jpg\thttps://example.com/details/3\tPortrait of a Landscape Painter\tK. Hansen, 1770\tDenmark\tPortraits
met\t4\tEUROPE\t2500\t3200\thttps://example.com/met4.jpg\thttps://example.com/details/4\tStill Life with Fruit\tJ. Doe\tItaly\tStill Life
cma\t5\tsomewhere with too few columns
cma\t6\tEUROPE\tnot-a-number\t3200\thttps://example.com/cma6.jpg\thttps://example.com/details/6\tA Landscape\t\t\t
";

    #[test]
    fn skips_comments_and_malformed_rows() {
        let entries = parse_catalogue(FIXTURE);
        // Four well-formed data rows; the comment, the three-field row and the
        // non-numeric width are all dropped rather than crashing the parse.
        assert_eq!(entries.len(), 4);
    }

    #[test]
    fn landscape_and_portrait_filter() {
        let entries = parse_catalogue(FIXTURE);
        let pool = landscape_candidates(&entries, None);
        let ids: Vec<&str> = pool.iter().map(|e| e.id.as_str()).collect();

        // Landscape by title, and landscape by tag, both included.
        assert!(ids.contains(&"1"));
        assert!(ids.contains(&"2"));
        // Catalogued as a portrait despite the word "Landscape" in its title.
        assert!(!ids.contains(&"3"));
        // No mention of the subject at all.
        assert!(!ids.contains(&"4"));
    }

    #[test]
    fn danish_landscape_terms_from_smk_are_recognised() {
        let danish = "\
# generated 2026-01-01 met=2026-01-01 nga=2026-01-01 cma=2026-01-01 smk=2026-01-01
smk\t10\tEUROPE\t2500\t3200\thttps://example.com/smk10.jpg\thttps://example.com/details/10\tDansk landskab\t\tDenmark\t
smk\t11\tEUROPE\t2500\t3200\thttps://example.com/smk11.jpg\thttps://example.com/details/11\tUdsigt fra Kvæsthusgade over Københavns havn\t\tDenmark\t
smk\t12\tEUROPE\t2500\t3200\thttps://example.com/smk12.jpg\thttps://example.com/details/12\tParti fra Amerikavej i København\t\tDenmark\t
smk\t13\tEUROPE\t2500\t3200\thttps://example.com/smk13.jpg\thttps://example.com/details/13\tGade i Torello\t\tDenmark\t
smk\t14\tEUROPE\t2500\t3200\thttps://example.com/smk14.jpg\thttps://example.com/details/14\tPiskebåndsjøden på en af Københavns gader\t\tDenmark\t
smk\t15\tEUROPE\t2500\t3200\thttps://example.com/smk15.jpg\thttps://example.com/details/15\tNordsjællandsk motiv\t\tDenmark\tLandskaber
";
        let entries = parse_catalogue(danish);
        let pool = landscape_candidates(&entries, None);
        let ids: Vec<&str> = pool.iter().map(|e| e.id.as_str()).collect();

        assert!(ids.contains(&"10")); // "landskab"
        assert!(ids.contains(&"11")); // "udsigt"
        assert!(ids.contains(&"12")); // "parti fra"
        assert!(ids.contains(&"13")); // "gade"
        assert!(ids.contains(&"14")); // "gader"
        assert!(ids.contains(&"15")); // "Landskaber" tag
    }

    #[test]
    fn the_english_preposition_by_does_not_fool_the_landscape_match() {
        // Danish "by" (town) is deliberately absent from LANDSCAPE_WORDS — this is
        // the false positive that ruled it out: an ordinary English title using
        // "by" as a preposition must not read as a landscape.
        let entry = Entry {
            source: MuseumSource::Cma,
            id: "1".to_owned(),
            image_url: "https://example.com/cma1.jpg".to_owned(),
            details_url: String::new(),
            title: "Chrysanthemums by a Stream".to_owned(),
            byline: String::new(),
            tags: Vec::new(),
        };
        assert!(!is_landscape(&entry));
    }

    #[test]
    fn a_danish_portrait_title_is_excluded_the_same_as_an_english_one() {
        let entry = Entry {
            source: MuseumSource::Smk,
            id: "1".to_owned(),
            image_url: "https://example.com/smk1.jpg".to_owned(),
            details_url: String::new(),
            title: "Mandsportræt".to_owned(),
            byline: String::new(),
            tags: vec!["Landskaber".to_owned()],
        };
        assert!(is_landscape(&entry)); // matches the "Landskaber" tag
        assert!(is_portrait(&entry)); // but is still excluded as a portrait
    }

    #[test]
    fn avoid_skips_only_the_named_entry() {
        let entries = parse_catalogue(FIXTURE);
        let pool = landscape_candidates(&entries, Some((MuseumSource::Met, "1")));
        assert!(!pool
            .iter()
            .any(|e| e.source == MuseumSource::Met && e.id == "1"));
        assert!(pool
            .iter()
            .any(|e| e.source == MuseumSource::Nga && e.id == "2"));
    }

    #[test]
    fn key_of_round_trips() {
        let path = Path::new("/cache/museums-nga-2.jpg");
        assert_eq!(key_of(path), Some((MuseumSource::Nga, "2")));
    }

    #[test]
    fn key_of_rejects_foreign_names() {
        assert_eq!(key_of(Path::new("/cache/met-123.jpg")), None);
        assert_eq!(key_of(Path::new("/cache/vacation-photo.png")), None);
        assert_eq!(key_of(Path::new("/cache/museums-atlantis-1.jpg")), None);
    }

    #[test]
    fn key_of_recognises_wikimedia_commons_downloads() {
        let path = Path::new("/cache/museums-wmc-98765.jpg");
        assert_eq!(key_of(path), Some((MuseumSource::Wmc, "98765")));
    }

    const WMC_FIXTURE: &str = "\
# generated 2026-01-01 met=2026-01-01 nga=2026-01-01 cma=2026-01-01 smk=2026-01-01 wmc=2026-01-01
wmc\t20\tEUROPE\t3000\t4000\thttps://example.com/wmc20.jpg\thttps://example.com/details/20\tThe Swan Princess\tMikhail Vrubel, 1900\tRussia\tFairy Tale\tMikhail Vrubel
wmc\t21\tEUROPE\t3000\t4000\thttps://example.com/wmc21.jpg\thttps://example.com/details/21\tGirl in the Sunlight\tValentin Serov, 1888\tRussia\t\tValentin Serov
met\t1\tEUROPE\t3000\t2000\thttps://example.com/met1.jpg\thttps://example.com/details/1\tLandscape near the Rhine\tAnonymous, 1800\tGermany\tLandscapes|Rivers\t
";

    #[test]
    fn eleven_and_twelve_column_rows_both_parse() {
        // FIXTURE is all eleven-column rows; WMC_FIXTURE's wmc rows carry a
        // twelfth artist column that this module has no use for. Both shapes
        // must parse every well-formed row, the extra column simply discarded.
        assert_eq!(parse_catalogue(FIXTURE).len(), 4);
        let wmc_entries = parse_catalogue(WMC_FIXTURE);
        assert_eq!(wmc_entries.len(), 3);
        assert!(wmc_entries.iter().any(|e| e.id == "20"));
        assert!(wmc_entries.iter().any(|e| e.id == "21"));
        // The eleven-column met row in the same fixture still parses too.
        assert!(wmc_entries.iter().any(|e| e.id == "1"));
    }
}
