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
//! Wikimedia Commons row (`wmc`) carries a twelfth column naming the artist.
//!
//! Subject, portrait, religious-scene and shape decisions are deliberately not the
//! build's to make, so this module makes them, from the [`Filters`] the settings
//! window chose: on the title and tags the catalogue carries — widened with the
//! Danish words SMK's records use — and on the pixel size it verified. Portraits
//! are excluded whatever was chosen, as [`met`](super::met) excludes them.
//!
//! The Android app reads the same TSV from its own assets and keeps its own copy
//! of the subject rules, including the Danish words, in `Catalogue.kt` — see
//! `docs/android.md` — and the iOS app a third, in `ios/ArtWindowKit/Catalogue.swift`
//! (`docs/ios.md`). A word-list change belongs in all three.

use super::http;
use super::{pick_index, Artwork, Selection, Source};
use crate::settings::{Filters, Region, Subject};
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

/// One row of the catalogue, minus the origin column nothing on the desktop
/// reads. Validated against the full eleven- or twelve-column contract in
/// [`parse_line`], so a row truncated or reordered upstream is skipped rather
/// than silently misread.
struct Entry {
    source: MuseumSource,
    id: String,
    region: Region,
    width: u32,
    height: u32,
    /// Empty on every row but Wikimedia Commons'.
    artist: String,
    image_url: String,
    details_url: String,
    title: String,
    byline: String,
    tags: Vec<String>,
    /// What the words in the title and tags say, read once when the catalogue is
    /// parsed rather than on every question the settings window asks of it.
    traits: Traits,
}

#[derive(Debug, Clone, Copy, Default)]
struct Traits {
    /// Indexed like [`Subject::ALL`].
    subjects: [bool; 3],
    religious: bool,
    portrait: bool,
}

impl Traits {
    fn of(entry: &Entry) -> Self {
        let texts = texts(entry);
        Self {
            subjects: Subject::ALL.map(|s| names_subject(&texts, s)),
            religious: names_religion(&texts),
            portrait: is_portrait(entry),
        }
    }

    fn is(&self, subject: Subject) -> bool {
        let i = Subject::ALL.iter().position(|&s| s == subject);
        i.is_some_and(|i| self.subjects[i])
    }
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
/// A twelfth column, present only on `wmc` rows, names the artist.
fn parse_line(line: &str) -> Option<Entry> {
    let fields: Vec<&str> = line.split('\t').collect();
    let (source, id, region, width, height, image_url, details_url, title, byline, tags, artist) =
        match fields[..] {
            [source, id, region, width, height, image_url, details_url, title, byline, _origin, tags] => {
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
                    tags,
                    "",
                )
            }
            [source, id, region, width, height, image_url, details_url, title, byline, _origin, tags, artist] => {
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
                    tags,
                    artist,
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
    let region = Region::parse_tsv(region)?;
    let width = width.parse::<u32>().ok().filter(|&w| w > 0)?;
    let height = height.parse::<u32>().ok().filter(|&h| h > 0)?;
    if image_url.is_empty() {
        return None;
    }

    let mut entry = Entry {
        source,
        id: id.to_owned(),
        region,
        width,
        height,
        artist: artist.trim().to_owned(),
        image_url: image_url.to_owned(),
        details_url: details_url.to_owned(),
        title: title.to_owned(),
        byline: byline.to_owned(),
        tags: if tags.is_empty() {
            Vec::new()
        } else {
            tags.split('|').map(str::to_owned).collect()
        },
        traits: Traits::default(),
    };
    entry.traits = Traits::of(&entry);
    Some(entry)
}

/// `text` as lower-case whole words — split once per text, since every word
/// list below is checked against the same few titles and tags.
fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Whether `words` contains `query` as a run of consecutive whole words, the
/// last optionally with a plain `s`-plural when `plural` — so "Landscapes" and
/// "LANDSCAPE" both count but "landscaping" does not. The same rule as Android's
/// `matchesQuery`.
fn mentions(words: &[String], query: &str, plural: bool) -> bool {
    let wanted: Vec<&str> = query.split_whitespace().collect();
    let Some(last) = wanted.len().checked_sub(1) else {
        return false;
    };
    words.windows(wanted.len()).any(|window| {
        window
            .iter()
            .zip(&wanted)
            .enumerate()
            .all(|(i, (w, q))| w == q || (plural && i == last && w.strip_suffix('s') == Some(*q)))
    })
}

/// The words that stand for each subject, checked against a title and every tag.
///
/// The Danish ones are there because SMK's records come back in Danish (see
/// `docs/android.md`), so a plain SMK title never contains the English word at
/// all. Each Danish irregular plural is its own entry ("landskab", "landskaber")
/// since [`mentions`]' "+s" rule does not form it. Danish "by" (town) is
/// deliberately absent — it collides with the English preposition "by" and would
/// flood the catalogue's English-language titles with false matches.
///
/// Kept word for word with Android's `ArtworkSubject` and iOS's `Catalogue.swift`.
fn subject_words(subject: Subject) -> &'static [&'static str] {
    match subject {
        Subject::Landscape => &[
            "landscape",
            "cityscape",
            "city",
            "street",
            "landskab",
            "landskaber",
            "parti fra",
            "udsigt",
            "gade",
            "gader",
        ],
        Subject::Seascape => &[
            "seascape", "marine", "boats", "havn", "skibe", "kyst", "strand", "hav", "både",
        ],
        Subject::StillLife => &[
            "still life",
            "flowers",
            "opstilling",
            "blomster",
            "stilleben",
        ],
    }
}

/// Religious scenes and figures, for *Hide religious scenes*. Every form is
/// spelled out and matched as whole words, so "Christmas" is never "Christ".
/// "St." is left out on purpose — it would also hide St. Petersburg. The same list
/// as Android's `RELIGIOUS_TERMS`.
const RELIGIOUS_WORDS: &[&str] = &[
    "christ",
    "jesus",
    "madonna",
    "virgin",
    "saint",
    "saints",
    "holy",
    "annunciation",
    "crucifixion",
    "crucified",
    "nativity",
    "adoration",
    "magi",
    "pieta",
    "pietà",
    "lamentation",
    "resurrection",
    "ascension",
    "assumption",
    "transfiguration",
    "apostle",
    "apostles",
    "evangelist",
    "evangelists",
    "baptism",
    "angel",
    "angels",
    "deposition",
    "entombment",
    "magdalene",
    "pope",
    "bible",
    "biblical",
    "gospel",
    "prophet",
    "prophets",
    "martyr",
    "martyrs",
    "martyrdom",
    "last supper",
    "pentecost",
    "flight into egypt",
    "moses",
    "abraham",
    "noah",
    "jonah",
    "tobias",
    "judith",
    "susanna",
    "samson",
    "buddha",
    "bodhisattva",
    "arhat",
    "deity",
    "deities",
    "kristus",
    "jomfru maria",
    "helgen",
    "apostel",
    "engel",
    "korsfæstelse",
];

/// The title and every tag, each as [`words`].
fn texts(entry: &Entry) -> Vec<Vec<String>> {
    std::iter::once(entry.title.as_str())
        .chain(entry.tags.iter().map(String::as_str))
        .map(words)
        .collect()
}

fn names_subject(texts: &[Vec<String>], subject: Subject) -> bool {
    texts
        .iter()
        .any(|t| subject_words(subject).iter().any(|q| mentions(t, q, true)))
}

fn names_religion(texts: &[Vec<String>]) -> bool {
    texts
        .iter()
        .any(|t| RELIGIOUS_WORDS.iter().any(|q| mentions(t, q, false)))
}

#[cfg(test)]
fn is_subject(entry: &Entry, subject: Subject) -> bool {
    names_subject(&texts(entry), subject)
}

#[cfg(test)]
fn is_religious(entry: &Entry) -> bool {
    names_religion(&texts(entry))
}

/// The same rule as `met::Object::is_portrait`, plus Danish "portræt" for SMK's
/// titles — checked as a substring like "portrait" is, since
/// [`met::Object::is_portrait`](super::met) uses the same substring form and the
/// two are meant to stay in step.
fn is_portrait(entry: &Entry) -> bool {
    let title = entry.title.to_lowercase();
    title.contains("portrait")
        || title.contains("portræt")
        || entry
            .tags
            .iter()
            .any(|t| t.eq_ignore_ascii_case("portraits"))
}

/// Whether `entry` passes every section of `filters` on a screen `aspect` wide.
fn admits(entry: &Entry, filters: &Filters, aspect: f64) -> bool {
    let traits = &entry.traits;
    (filters.regions.is_empty() || filters.regions.contains(&entry.region))
        && (filters.subjects.is_empty() || filters.subjects.iter().any(|&s| traits.is(s)))
        && (filters.artists.is_empty() || filters.artists.contains(&entry.artist))
        && !traits.portrait
        && !(filters.hide_religious && traits.religious)
        && filters
            .shape
            .accepts(f64::from(entry.width) / f64::from(entry.height), aspect)
}

/// Entries `selection` admits, minus whichever one `avoid` names.
fn candidates<'a>(
    entries: &'a [Entry],
    selection: &Selection,
    avoid: Option<(MuseumSource, &str)>,
) -> Vec<&'a Entry> {
    entries
        .iter()
        .filter(|e| admits(e, &selection.filters, selection.screen_aspect))
        .filter(|e| avoid != Some((e.source, e.id.as_str())))
        .collect()
}

/// Which choices in each section would still find a painting, the other sections
/// held where they are — what the settings window asks before it shows a chip.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Availability {
    pub regions: Vec<Region>,
    pub subjects: Vec<Subject>,
    pub artists: Vec<String>,
    /// How many paintings the filters as a whole admit. Zero means *Apply* would
    /// leave the rotation with nothing to pick.
    pub matching: usize,
}

/// Asks the whole catalogue [`Availability`]'s question: for each option, the
/// pool with only that option chosen in its own section. Android's
/// `Catalogue.availableRegions` and its siblings, in one pass per option.
pub fn availability(filters: &Filters, aspect: f64) -> Availability {
    let entries = catalogue();
    let any = |f: &Filters| entries.iter().any(|e| admits(e, f, aspect));
    Availability {
        regions: Region::ALL
            .into_iter()
            .filter(|&r| {
                any(&Filters {
                    regions: vec![r],
                    ..filters.clone()
                })
            })
            .collect(),
        subjects: Subject::ALL
            .into_iter()
            .filter(|&s| {
                any(&Filters {
                    subjects: vec![s],
                    ..filters.clone()
                })
            })
            .collect(),
        artists: artists()
            .iter()
            .filter(|a| {
                any(&Filters {
                    artists: vec![(*a).clone()],
                    ..filters.clone()
                })
            })
            .cloned()
            .collect(),
        matching: entries
            .iter()
            .filter(|e| admits(e, filters, aspect))
            .count(),
    }
}

/// Every artist the catalogue names, sorted.
pub fn artists() -> &'static [String] {
    static ARTISTS: OnceLock<Vec<String>> = OnceLock::new();
    ARTISTS.get_or_init(|| {
        let mut names: Vec<String> = catalogue()
            .iter()
            .filter(|e| !e.artist.is_empty())
            .map(|e| e.artist.clone())
            .collect();
        names.sort();
        names.dedup();
        names
    })
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
    selection: Selection,
}

impl Museums {
    pub fn new(cache: PathBuf, selection: Selection) -> Self {
        Self {
            agent: http::agent(),
            cache,
            selection,
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
        let pool = candidates(catalogue(), &self.selection, avoid);
        if pool.is_empty() {
            return Err(anyhow!(
                "no paintings in the catalogue match the chosen filters"
            ));
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
    use crate::settings::Shape;

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

    fn landscapes() -> Selection {
        Selection::default()
    }

    fn entry(title: &str, tags: &[&str]) -> Entry {
        Entry {
            source: MuseumSource::Cma,
            id: "1".to_owned(),
            region: Region::Europe,
            width: 3000,
            height: 2000,
            artist: String::new(),
            image_url: "https://example.com/cma1.jpg".to_owned(),
            details_url: String::new(),
            title: title.to_owned(),
            byline: String::new(),
            tags: tags.iter().map(|t| (*t).to_owned()).collect(),
            traits: Traits::default(),
        }
    }

    #[test]
    fn landscape_and_portrait_filter() {
        let entries = parse_catalogue(FIXTURE);
        let pool = candidates(&entries, &landscapes(), None);
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
        let pool = candidates(&entries, &landscapes(), None);
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
        let entry = entry("Chrysanthemums by a Stream", &[]);
        assert!(!is_subject(&entry, Subject::Landscape));
    }

    #[test]
    fn a_danish_portrait_title_is_excluded_the_same_as_an_english_one() {
        let entry = entry("Mandsportræt", &["Landskaber"]);
        assert!(is_subject(&entry, Subject::Landscape)); // matches the "Landskaber" tag
        assert!(is_portrait(&entry)); // but is still excluded as a portrait
    }

    #[test]
    fn avoid_skips_only_the_named_entry() {
        let entries = parse_catalogue(FIXTURE);
        let pool = candidates(&entries, &landscapes(), Some((MuseumSource::Met, "1")));
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
        // twelfth artist column. Both shapes must parse every well-formed row.
        assert_eq!(parse_catalogue(FIXTURE).len(), 4);
        let wmc_entries = parse_catalogue(WMC_FIXTURE);
        assert_eq!(wmc_entries.len(), 3);
        assert!(wmc_entries.iter().any(|e| e.id == "20"));
        assert!(wmc_entries.iter().any(|e| e.id == "21"));
        // The eleven-column met row in the same fixture still parses too.
        assert!(wmc_entries.iter().any(|e| e.id == "1"));
    }

    #[test]
    fn the_artist_column_is_kept() {
        let entries = parse_catalogue(WMC_FIXTURE);
        let vrubel = entries.iter().find(|e| e.id == "20").unwrap();
        assert_eq!(vrubel.artist, "Mikhail Vrubel");
        assert!(entries
            .iter()
            .find(|e| e.id == "1")
            .unwrap()
            .artist
            .is_empty());
    }

    #[test]
    fn sections_are_anded_and_choices_ored() {
        let entries = parse_catalogue(FIXTURE);
        let still_lifes_or_landscapes = Selection {
            filters: Filters {
                subjects: vec![Subject::Landscape, Subject::StillLife],
                ..Filters::default()
            },
            ..Selection::default()
        };
        let ids: Vec<&str> = candidates(&entries, &still_lifes_or_landscapes, None)
            .iter()
            .map(|e| e.id.as_str())
            .collect();
        assert!(ids.contains(&"1") && ids.contains(&"4"));

        let asian_landscapes = Selection {
            filters: Filters {
                regions: vec![Region::Asia],
                ..Filters::default()
            },
            ..Selection::default()
        };
        assert!(candidates(&entries, &asian_landscapes, None).is_empty());
    }

    #[test]
    fn an_empty_subject_set_means_any_subject() {
        let entries = parse_catalogue(FIXTURE);
        let anything = Selection {
            filters: Filters {
                subjects: Vec::new(),
                ..Filters::default()
            },
            ..Selection::default()
        };
        // Everything but the portrait.
        assert_eq!(candidates(&entries, &anything, None).len(), 3);
    }

    #[test]
    fn shape_is_read_from_the_verified_pixel_size() {
        let entries = parse_catalogue(FIXTURE);
        let wide = Selection {
            filters: Filters {
                subjects: Vec::new(),
                shape: Shape::Screen,
                ..Filters::default()
            },
            screen_aspect: 16.0 / 10.0,
        };
        let ids: Vec<&str> = candidates(&entries, &wide, None)
            .iter()
            .map(|e| e.id.as_str())
            .collect();
        // 3000×2000 is 1.5, within the trim of 1.6; 4000×3000 and the upright
        // still life are not.
        assert_eq!(ids, vec!["1"]);
    }

    #[test]
    fn religious_words_match_whole_words_only() {
        assert!(is_religious(&entry("The Adoration of the Magi", &[])));
        assert!(is_religious(&entry("A river", &["Virgin Mary"])));
        assert!(is_religious(&entry("Christ's Entry", &[])));
        assert!(is_religious(&entry("Korsfæstelse", &[])));
        assert!(!is_religious(&entry("Christmas Eve in the Snow", &[])));
        assert!(!is_religious(&entry("View of St. Petersburg", &[])));
    }

    #[test]
    fn multi_word_subjects_match_as_phrases() {
        assert!(is_subject(
            &entry("Still Life with Fruit", &[]),
            Subject::StillLife
        ));
        assert!(!is_subject(&entry("Life, Still", &[]), Subject::StillLife));
        assert!(is_subject(
            &entry("Boats at Honfleur", &[]),
            Subject::Seascape
        ));
    }
}
