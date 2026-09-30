//! The settings tab's model: what has been chosen but not yet applied, and what
//! the window should draw because of it.
//!
//! Every platform draws the tab its own way, but none of them decides anything.
//! Which chips to show, whether *Apply changes* can be pressed, what the preview
//! looks like and what a click on *Blur* restores are all answered here, once, so
//! the three windows cannot drift apart the way three copies of these rules would.
//! A platform keeps one of these, forwards clicks into it, and redraws from it.

use crate::art::museums::{self, Availability};
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
    /// filters are in force — they still leave something to pick.
    pub fn can_apply(&self) -> bool {
        self.staged != self.applied && (!self.filters_apply || self.availability.matching > 0)
    }

    /// A line to show beside *Apply changes*, when there is something to say.
    pub fn note(&self) -> Option<&'static str> {
        if !self.filters_apply {
            Some("Filters apply to the museum catalogue only — see config.toml.")
        } else if self.availability.matching == 0 {
            Some("No painting matches all of these.")
        } else if self.staged.filters != self.applied.filters {
            Some("Filters take effect from the next picture.")
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
            })
            .collect()
    }

    /// Regions with something to show, plus any already chosen — a chosen chip
    /// never vanishes out from under the person who chose it.
    pub fn regions(&self) -> Vec<Chip<Region>> {
        let chosen = &self.staged.filters.regions;
        Region::ALL
            .into_iter()
            .filter(|r| chosen.contains(r) || self.availability.regions.contains(r))
            .map(|r| Chip {
                value: r,
                label: r.label().to_owned(),
                selected: chosen.contains(&r),
            })
            .collect()
    }

    pub fn subjects(&self) -> Vec<Chip<Subject>> {
        let chosen = &self.staged.filters.subjects;
        Subject::ALL
            .into_iter()
            .filter(|s| chosen.contains(s) || self.availability.subjects.contains(s))
            .map(|s| Chip {
                value: s,
                label: s.label().to_owned(),
                selected: chosen.contains(&s),
            })
            .collect()
    }

    pub fn artists(&self) -> Vec<Chip<String>> {
        let chosen = &self.staged.filters.artists;
        museums::artists()
            .iter()
            .filter(|a| chosen.contains(a) || self.availability.artists.contains(a))
            .map(|a| Chip {
                value: a.clone(),
                label: a.clone(),
                selected: chosen.contains(a),
            })
            .collect()
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
}
