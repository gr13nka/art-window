//! The settings tab's model: what has been chosen but not yet applied, and what
//! the window should draw because of it.
//!
//! Every platform draws the tab its own way, but none of them decides anything.
//! Which chips to show, whether *Apply changes* can be pressed, what the preview
//! looks like and what a click on *Blur* restores are all answered here, once, so
//! the three windows cannot drift apart the way three copies of these rules would.
//! A platform keeps one of these, forwards clicks into it, and redraws from it.

use crate::art::museums::{self, Availability, FilterSection, Unavailable, MIN_POOL};
use crate::art::{artists, Artwork};
use crate::placement::Preview;
use crate::settings::{
    BlurVariant, Border, Region, Settings, Shape, Style, Subject, DEFAULT_BLUR_STRENGTH,
    DEFAULT_CUSTOM_BORDER,
};
use image::RgbaImage;
use std::path::{Path, PathBuf};

/// The four style cards, without their options — what a card click names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleKind {
    Borders,
    Zoom,
    Stretch,
    Blur,
}

impl StyleKind {
    pub const ALL: [StyleKind; 4] = [
        StyleKind::Borders,
        StyleKind::Zoom,
        StyleKind::Stretch,
        StyleKind::Blur,
    ];

    pub fn label(self) -> &'static str {
        match self {
            StyleKind::Borders => "Borders",
            StyleKind::Zoom => "Zoom",
            StyleKind::Stretch => "Stretch",
            StyleKind::Blur => "Blur",
        }
    }

    fn of(style: &Style) -> Self {
        match style {
            Style::Borders { .. } => StyleKind::Borders,
            Style::Zoom => StyleKind::Zoom,
            Style::Stretch => StyleKind::Stretch,
            Style::Blur { .. } => StyleKind::Blur,
        }
    }
}

/// One chip: what it says, and whether it is on.
pub struct Chip<T> {
    pub value: T,
    pub label: String,
    pub selected: bool,
    /// Present when clicking this choice should explain why it cannot be used
    /// instead of changing the staged settings.
    pub disabled_reason: Option<String>,
}

/// The settings tab's *Artist* row: the painters already chosen and one button
/// that opens the browser. Not a chip per painter — the catalogue names more of
/// them with every build, and a row of fourteen names tells nobody which to pick.
pub struct ArtistRow {
    /// Clicking one takes that painter out again.
    pub chosen: Vec<Chip<String>>,
    /// What the button that opens the browser says.
    pub browse: &'static str,
    /// Present when the button should explain itself instead of opening anything.
    pub disabled_reason: Option<String>,
}

/// One painter in the artist browser.
pub struct ArtistCard {
    /// The painter, dressed as a picture so that the shelf and preview each
    /// window already has for favourites can show it unchanged: `title` is the
    /// painter's name and what a click chooses, `byline` the painting they are
    /// shown by and how much of their work there is, `details_url` where to read
    /// about them.
    pub art: Artwork,
    pub selected: bool,
    /// Present when choosing this painter should explain why it cannot be done.
    pub disabled_reason: Option<String>,
}

pub struct Pending {
    applied: Settings,
    staged: Settings,
    /// Whether the configured source can honour filters at all. Only the museum
    /// catalogue can; with any other the filter sections are shown but inert.
    filters_apply: bool,
    /// The main display's width ÷ height, which the preview and the shape filter
    /// are both measured against.
    aspect: f64,
    /// Border and blur options survive a visit to another style, so trying *Zoom*
    /// and coming back does not lose a colour someone picked.
    border: Border,
    blur: (BlurVariant, u8),
    picture: Option<(PathBuf, Preview)>,
    availability: Availability,
    /// Where the painters' pictures are unpacked — see [`Pending::artist_cards`].
    pictures: PathBuf,
}

impl Pending {
    pub fn new(applied: Settings, filters_apply: bool, aspect: f64) -> Self {
        let mut pending = Self {
            staged: applied.clone(),
            applied,
            filters_apply,
            aspect,
            border: Border::Black,
            blur: (BlurVariant::Backdrop, DEFAULT_BLUR_STRENGTH),
            picture: None,
            availability: Availability::default(),
            pictures: std::env::temp_dir(),
        };
        pending.remember_options();
        pending.refresh();
        pending
    }

    /// Takes on settings the loop has just applied. Anything staged and not yet
    /// applied is discarded only if it was what was applied — a describe arriving
    /// for an unrelated reason (a new painting) must not throw away half-made
    /// choices.
    pub fn adopt(&mut self, applied: &Settings, filters_apply: bool, aspect: f64) {
        let untouched = self.staged == self.applied || self.staged == *applied;
        self.applied = applied.clone();
        self.filters_apply = filters_apply;
        self.aspect = aspect;
        if untouched {
            self.staged = applied.clone();
            self.remember_options();
        }
        self.refresh();
    }

    /// Points the preview at the picture on the desktop. Decoding is the slow part,
    /// so it is done only when the picture actually changes.
    pub fn set_picture(&mut self, path: Option<&Path>) {
        if self.picture.as_ref().map(|(p, _)| p.as_path()) == path {
            return;
        }
        self.picture = path.and_then(|p| Preview::new(p).ok().map(|pv| (p.to_path_buf(), pv)));
    }

    /// Says where the painters' pictures may be unpacked: the cache, which the
    /// window is told and this model is not built with. Until it is said they go
    /// to the system's temporary directory, which is as disposable.
    pub fn keep_pictures_in(&mut self, dir: &Path) {
        self.pictures = dir.to_path_buf();
    }

    /// The staged style drawn `width` pixels across at the screen's shape, or
    /// `None` while there is no picture to draw.
    pub fn preview(&self, width: u32) -> Option<RgbaImage> {
        self.picture
            .as_ref()
            .map(|(_, p)| p.render(&self.staged.style, self.aspect, width))
    }

    pub fn aspect(&self) -> f64 {
        self.aspect
    }

    pub fn staged(&self) -> &Settings {
        &self.staged
    }

    pub fn filters_apply(&self) -> bool {
        self.filters_apply
    }

    /// Whether *Apply changes* means anything: something differs, and — when the
    /// filters are in force — they leave at least [`MIN_POOL`] paintings to pick.
    ///
    /// Filters that were already applied are not held against a change of style:
    /// a selection saved before the floor existed would otherwise lock the whole
    /// tab until it was widened.
    pub fn can_apply(&self) -> bool {
        self.staged != self.applied
            && (!self.filters_apply
                || self.availability.is_enough()
                || self.staged.filters == self.applied.filters)
    }

    /// A line to show beside *Apply changes*, when there is something to say.
    pub fn note(&self) -> Option<String> {
        let changed = self.staged.filters != self.applied.filters;
        if !self.filters_apply {
            Some("Filters apply to the museum catalogue only — see config.toml.".to_owned())
        } else if !self.availability.is_enough() {
            let n = self.availability.matching;
            let few = match n {
                0 => "No painting matches".to_owned(),
                1 => "Only 1 painting matches".to_owned(),
                _ => format!("Only {n} paintings match"),
            };
            Some(if changed && self.staged.filters.artists.is_empty() {
                format!("{few} all of these — at least {MIN_POOL} are needed.")
            } else if changed {
                format!("{few} all of these.")
            } else {
                format!("{few} these filters, so pictures come from a wider selection.")
            })
        } else if changed {
            Some("Filters take effect from the next picture.".to_owned())
        } else {
            None
        }
    }

    pub fn style_kind(&self) -> StyleKind {
        StyleKind::of(&self.staged.style)
    }

    pub fn border(&self) -> Border {
        self.border
    }

    pub fn blur(&self) -> (BlurVariant, u8) {
        self.blur
    }

    pub fn set_style(&mut self, kind: StyleKind) {
        self.staged.style = match kind {
            StyleKind::Borders => Style::Borders {
                colour: self.border,
            },
            StyleKind::Zoom => Style::Zoom,
            StyleKind::Stretch => Style::Stretch,
            StyleKind::Blur => Style::Blur {
                variant: self.blur.0,
                strength: self.blur.1,
            },
        };
    }

    pub fn set_border(&mut self, border: Border) {
        self.border = border;
        self.set_style(StyleKind::Borders);
    }

    /// The custom colour last chosen, or the default one, for the colour picker to
    /// open on.
    pub fn custom_colour(&self) -> [u8; 3] {
        match self.border {
            Border::Custom { rgb } => rgb,
            _ => DEFAULT_CUSTOM_BORDER,
        }
    }

    pub fn set_blur_variant(&mut self, variant: BlurVariant) {
        self.blur.0 = variant;
        self.set_style(StyleKind::Blur);
    }

    pub fn set_blur_strength(&mut self, strength: u8) {
        self.blur.1 = strength.min(100);
        self.set_style(StyleKind::Blur);
    }

    pub fn shapes(&self) -> Vec<Chip<Shape>> {
        Shape::ALL
            .into_iter()
            .map(|s| Chip {
                value: s,
                label: s.label().to_owned(),
                selected: self.staged.filters.shape == s,
                disabled_reason: self.idle_reason().or_else(|| {
                    self.disabled_reason(
                        self.staged.filters.shape == s,
                        self.availability
                            .shapes
                            .iter()
                            .find_map(|(shape, why)| (*shape == s).then_some(why)),
                        s.label(),
                    )
                }),
            })
            .collect()
    }

    /// Every region stays put as other settings change. An unavailable one carries
    /// the reason a platform can show instead of letting the row jump around.
    pub fn regions(&self) -> Vec<Chip<Region>> {
        let chosen = &self.staged.filters.regions;
        Region::ALL
            .into_iter()
            .map(|r| Chip {
                value: r,
                label: r.label().to_owned(),
                selected: chosen.contains(&r),
                disabled_reason: self.idle_reason().or_else(|| {
                    self.disabled_reason(
                        chosen.contains(&r),
                        self.availability
                            .regions
                            .iter()
                            .find_map(|(region, why)| (*region == r).then_some(why)),
                        r.label(),
                    )
                }),
            })
            .collect()
    }

    pub fn subjects(&self) -> Vec<Chip<Subject>> {
        let chosen = &self.staged.filters.subjects;
        Subject::ALL
            .into_iter()
            .map(|s| Chip {
                value: s,
                label: s.label().to_owned(),
                selected: chosen.contains(&s),
                disabled_reason: self.disabled_reason(
                    chosen.contains(&s),
                    self.availability
                        .subjects
                        .iter()
                        .find_map(|(subject, why)| (*subject == s).then_some(why)),
                    s.label(),
                ),
            })
            .collect()
    }

    /// The *Artist* row. Every staged painter is listed, including one a newer
    /// catalogue no longer names, so that nothing chosen is ever out of reach.
    pub fn artist_row(&self) -> ArtistRow {
        let chosen = &self.staged.filters.artists;
        ArtistRow {
            chosen: chosen
                .iter()
                .map(|a| Chip {
                    value: a.clone(),
                    label: a.clone(),
                    selected: true,
                    disabled_reason: self.disabled_reason(true, None, a),
                })
                .collect(),
            browse: if chosen.is_empty() {
                "Any artist"
            } else {
                "Add"
            },
            disabled_reason: self.disabled_reason(false, None, ""),
        }
    }

    /// The artist browser's shelf: every painter who can be chosen, by region and
    /// then by name, each with the picture they are known by.
    ///
    /// Unpacks those pictures on the way, which is a file written per painter the
    /// first time and nothing after — so this is cheap enough to call on every
    /// redraw, but belongs behind the button and not in the row.
    pub fn artist_cards(&self) -> Vec<ArtistCard> {
        let chosen = &self.staged.filters.artists;
        let mut cards: Vec<_> = artists::all()
            .iter()
            .filter(|artist| museums::artists().contains(&artist.name))
            .collect();
        let place = |r: Region| Region::ALL.iter().position(|x| *x == r);
        cards.sort_by(|a, b| (place(a.region), &a.name).cmp(&(place(b.region), &b.name)));
        cards
            .into_iter()
            .map(|artist| {
                let selected = chosen.contains(&artist.name);
                // The catalogue's byline is "painter, year"; the painter is the
                // title here, so only the year is wanted.
                let year = artist
                    .byline
                    .strip_prefix(artist.name.as_str())
                    .map(|rest| rest.trim_start_matches([',', ' ']))
                    .filter(|year| !year.is_empty());
                let painting = match year {
                    Some(year) => format!("{}, {year}", artist.title),
                    None => artist.title.clone(),
                };
                ArtistCard {
                    art: Artwork {
                        title: artist.name.clone(),
                        byline: format!(
                            "{painting} · {}, {} paintings",
                            artist.region.label(),
                            museums::paintings_by(&artist.name)
                        ),
                        attribution: "Wikimedia Commons".to_owned(),
                        details_url: Some(artist.about.clone()),
                        // A picture that cannot be unpacked is an empty frame
                        // with the painter's name under it, not a missing row.
                        path: artists::picture(artist, &self.pictures).unwrap_or_default(),
                    },
                    selected,
                    disabled_reason: self.artist_reason(
                        selected,
                        self.availability
                            .artists
                            .iter()
                            .find_map(|(name, why)| (*name == artist.name).then_some(why)),
                        &artist.name,
                    ),
                }
            })
            .collect()
    }

    /// Shape and origin are not used while a painter is chosen — see
    /// `museums::admits` — and every chip in them says so, selected or not.
    fn idle_reason(&self) -> Option<String> {
        (self.filters_apply && !self.staged.filters.artists.is_empty())
            .then(|| "Not used while an artist is chosen.".to_owned())
    }

    /// A painter's card is blocked only by the sections that still apply to a
    /// painter: subject and *Hide religious*.
    fn artist_reason(
        &self,
        selected: bool,
        unavailable: Option<&Unavailable>,
        name: &str,
    ) -> Option<String> {
        if !self.filters_apply || selected {
            return self.disabled_reason(selected, None, name);
        }
        match unavailable? {
            Unavailable::TooFew => Some(format!("None of {name}'s paintings can be shown.")),
            Unavailable::Conflicts(sections) => Some(format!(
                "None of {name}'s paintings match {}.",
                self.section_labels(sections)
            )),
        }
    }

    fn section_labels(&self, sections: &[FilterSection]) -> String {
        sections
            .iter()
            .map(|section| self.section_label(*section))
            .collect::<Vec<_>>()
            .join(" and ")
    }

    fn disabled_reason(
        &self,
        selected: bool,
        unavailable: Option<&Unavailable>,
        choice: &str,
    ) -> Option<String> {
        if !self.filters_apply {
            return Some(
                "Filters are unavailable because Source is not set to ‘museums’.".to_owned(),
            );
        }
        // Multi-choice filters must always let a selected value be removed. A
        // selected single-choice shape is already the active value, so treating it
        // the same way also avoids presenting the current state as unclickable.
        if selected {
            return None;
        }
        match unavailable? {
            Unavailable::TooFew => Some(format!(
                "Fewer than {MIN_POOL} catalogue paintings are available for {choice}."
            )),
            Unavailable::Conflicts(sections) => Some(format!(
                "Too few paintings with {}.",
                self.section_labels(sections)
            )),
        }
    }

    fn section_label(&self, section: FilterSection) -> String {
        let filters = &self.staged.filters;
        match section {
            FilterSection::Shape => format!("Shape: {}", filters.shape.label()),
            FilterSection::Origin => format!(
                "Origin: {}",
                filters
                    .regions
                    .iter()
                    .map(|region| region.label())
                    .collect::<Vec<_>>()
                    .join(" or ")
            ),
            FilterSection::Subject => format!(
                "Subject: {}",
                filters
                    .subjects
                    .iter()
                    .map(|subject| subject.label())
                    .collect::<Vec<_>>()
                    .join(" or ")
            ),
            FilterSection::Artist => {
                format!("Artist: {}", filters.artists.join(" or "))
            }
            FilterSection::Content => "Content: Hide religious scenes".to_owned(),
        }
    }

    pub fn hide_religious(&self) -> bool {
        self.staged.filters.hide_religious
    }

    pub fn set_shape(&mut self, shape: Shape) {
        self.staged.filters.shape = shape;
        self.refresh();
    }

    pub fn toggle_region(&mut self, region: Region) {
        toggle(&mut self.staged.filters.regions, region);
        self.refresh();
    }

    pub fn toggle_subject(&mut self, subject: Subject) {
        toggle(&mut self.staged.filters.subjects, subject);
        self.refresh();
    }

    pub fn toggle_artist(&mut self, artist: &str) {
        toggle(&mut self.staged.filters.artists, artist.to_owned());
        self.refresh();
    }

    pub fn set_hide_religious(&mut self, hide: bool) {
        self.staged.filters.hide_religious = hide;
        self.refresh();
    }

    fn remember_options(&mut self) {
        match self.staged.style {
            Style::Borders { colour } => self.border = colour,
            Style::Blur { variant, strength } => self.blur = (variant, strength),
            _ => {}
        }
    }

    fn refresh(&mut self) {
        self.availability = museums::availability(&self.staged.filters, self.aspect);
    }
}

fn toggle<T: PartialEq>(set: &mut Vec<T>, value: T) {
    match set.iter().position(|v| *v == value) {
        Some(i) => {
            set.remove(i);
        }
        None => set.push(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pending() -> Pending {
        Pending::new(Settings::default(), true, 16.0 / 10.0)
    }

    #[test]
    fn nothing_to_apply_until_something_changes() {
        let mut p = pending();
        assert!(!p.can_apply());
        p.set_style(StyleKind::Zoom);
        assert!(p.can_apply());
        p.set_style(StyleKind::Borders);
        assert!(!p.can_apply());
    }

    #[test]
    fn a_style_remembers_its_options_across_a_visit_elsewhere() {
        let mut p = pending();
        p.set_border(Border::Custom { rgb: [9, 8, 7] });
        p.set_style(StyleKind::Stretch);
        p.set_style(StyleKind::Borders);
        assert_eq!(
            p.staged().style,
            Style::Borders {
                colour: Border::Custom { rgb: [9, 8, 7] }
            }
        );
    }

    #[test]
    fn a_describe_for_a_new_painting_keeps_half_made_choices() {
        let mut p = pending();
        p.set_style(StyleKind::Zoom);
        p.adopt(&Settings::default(), true, 16.0 / 10.0);
        assert_eq!(p.staged().style, Style::Zoom);
    }

    #[test]
    fn a_chosen_chip_stays_even_with_nothing_behind_it() {
        let mut p = pending();
        p.toggle_region(Region::Oceania);
        p.toggle_subject(Subject::StillLife);
        assert!(p
            .regions()
            .iter()
            .any(|c| c.value == Region::Oceania && c.selected));
    }

    #[test]
    fn the_artist_row_holds_only_who_was_chosen() {
        let mut p = pending();
        let row = p.artist_row();
        assert!(row.chosen.is_empty());
        assert_eq!(row.browse, "Any artist");

        // Somebody a later catalogue dropped is still there to be taken out.
        p.toggle_artist("Nobody At All");
        let row = p.artist_row();
        assert_eq!(row.browse, "Add");
        assert_eq!(row.chosen.len(), 1);
        assert!(row.chosen[0].selected && row.chosen[0].disabled_reason.is_none());
    }

    #[test]
    fn every_painter_on_the_shelf_can_be_chosen_by_the_name_it_shows() {
        let dir = std::env::temp_dir().join("art-window-test-artists");
        let mut p = pending();
        p.keep_pictures_in(&dir);
        let cards = p.artist_cards();
        assert_eq!(cards.len(), museums::artists().len());
        for card in &cards {
            assert!(museums::artists().contains(&card.art.title));
            assert!(card.art.path.is_file(), "{} has no picture", card.art.title);
            assert!(card.art.details_url.is_some());
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn every_fixed_filter_choice_stays_visible() {
        let p = pending();
        assert_eq!(p.shapes().len(), Shape::ALL.len());
        assert_eq!(p.regions().len(), Region::ALL.len());
        assert_eq!(p.subjects().len(), Subject::ALL.len());
    }

    #[test]
    fn a_thin_selection_cannot_be_applied_and_says_why() {
        let mut p = pending();
        // No catalogue names this painter, so the count is nought whatever is in it.
        p.toggle_artist("Nobody At All");
        assert!(!p.can_apply());
        assert_eq!(p.note().unwrap(), "No painting matches all of these.");
    }

    #[test]
    fn thin_filters_already_applied_do_not_hold_up_a_change_of_style() {
        let mut thin = Settings::default();
        thin.filters.artists = vec!["Nobody At All".to_owned()];
        let mut p = Pending::new(thin, true, 16.0 / 10.0);
        assert_eq!(
            p.note().unwrap(),
            "No painting matches these filters, so pictures come from a wider selection."
        );
        p.set_style(StyleKind::Zoom);
        assert!(p.can_apply());
    }

    #[test]
    fn shape_and_origin_are_idle_while_a_painter_is_staged() {
        let idle = Some("Not used while an artist is chosen.".to_owned());
        let mut p = pending();
        assert!(p.regions().iter().all(|c| c.disabled_reason != idle));
        p.toggle_artist("Nobody At All");
        assert!(p.shapes().iter().all(|c| c.disabled_reason == idle));
        assert!(p.regions().iter().all(|c| c.disabled_reason == idle));
        p.toggle_artist("Nobody At All");
        assert!(p.shapes().iter().all(|c| c.disabled_reason != idle));
        assert!(p.regions().iter().all(|c| c.disabled_reason != idle));
    }

    #[test]
    fn a_painters_card_names_only_subject_and_content() {
        let mut p = pending();
        p.keep_pictures_in(&std::env::temp_dir().join("art-window-test-artists-idle"));
        p.toggle_region(Region::Oceania);
        p.set_shape(Shape::NearSquare);
        for card in p.artist_cards() {
            if let Some(why) = card.disabled_reason {
                assert!(!why.contains("Shape") && !why.contains("Origin"), "{why}");
            }
        }
    }

    #[test]
    fn an_unsupported_source_explains_every_chip() {
        let p = Pending::new(Settings::default(), false, 16.0 / 10.0);
        let expected = "Filters are unavailable because Source is not set to ‘museums’.";
        assert!(p
            .shapes()
            .iter()
            .all(|chip| chip.disabled_reason.as_deref() == Some(expected)));
        assert!(p
            .regions()
            .iter()
            .all(|chip| chip.disabled_reason.as_deref() == Some(expected)));
        assert!(p
            .subjects()
            .iter()
            .all(|chip| chip.disabled_reason.as_deref() == Some(expected)));
    }
}
