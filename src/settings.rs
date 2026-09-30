//! What the settings window decides: which paintings may arrive, and how one is
//! hung.
//!
//! A third file beside `config.toml` and `state.json`, because it has a third
//! author. `config.toml` is the person's, typed by hand and never written back, so
//! their comments survive; `state.json` is the program's own memory. This one is
//! written by the window when *Apply changes* is pressed, and read back by the
//! loop — a person may look at it, but is not expected to edit it.
//!
//! Every value here has a default that reproduces the program as it was before
//! the window existed — landscapes, any shape, fitted over black — so a file that
//! is missing, or that an older build cannot parse, changes nothing anyone sees.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub filters: Filters,
    pub style: Style,
}

/// Which of the museum catalogue's paintings may be picked.
///
/// Sections are ANDed; the choices inside one are ORed; an empty set means
/// *any*. Only the museum catalogue can answer these questions — the Met's live
/// search and a folder of the user's own pictures carry no region, subject or
/// artist to test — so the other sources ignore them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Filters {
    pub regions: Vec<Region>,
    pub subjects: Vec<Subject>,
    pub artists: Vec<String>,
    pub hide_religious: bool,
    pub shape: Shape,
}

impl Default for Filters {
    fn default() -> Self {
        Self {
            regions: Vec::new(),
            subjects: vec![Subject::Landscape],
            artists: Vec::new(),
            hide_religious: false,
            shape: Shape::Any,
        }
    }
}

/// Where a painting was made, as `catalogue/build.py` spells it in the TSV.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Region {
    Europe,
    Asia,
    Africa,
    NorthAmerica,
    SouthAmerica,
    Oceania,
}

impl Region {
    pub const ALL: [Region; 6] = [
        Region::Europe,
        Region::Asia,
        Region::Africa,
        Region::NorthAmerica,
        Region::SouthAmerica,
        Region::Oceania,
    ];

    /// The catalogue's spelling — the TSV's third column.
    pub fn parse_tsv(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.tsv() == s)
    }

    fn tsv(self) -> &'static str {
        match self {
            Region::Europe => "EUROPE",
            Region::Asia => "ASIA",
            Region::Africa => "AFRICA",
            Region::NorthAmerica => "NORTH_AMERICA",
            Region::SouthAmerica => "SOUTH_AMERICA",
            Region::Oceania => "OCEANIA",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Region::Europe => "Europe",
            Region::Asia => "Asia",
            Region::Africa => "Africa",
            Region::NorthAmerica => "North America",
            Region::SouthAmerica => "South America",
            Region::Oceania => "Oceania",
        }
    }
}

/// What a painting is of. The words that stand for each are the catalogue's
/// business — see `art::museums`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Subject {
    Landscape,
    Seascape,
    StillLife,
}

impl Subject {
    pub const ALL: [Subject; 3] = [Subject::Landscape, Subject::Seascape, Subject::StillLife];

    pub fn label(self) -> &'static str {
        match self {
            Subject::Landscape => "Landscape",
            Subject::Seascape => "Seascape",
            Subject::StillLife => "Still life",
        }
    }
}

/// Which way round a painting may be, relative to the main display.
///
/// A measurement of the pixel size the catalogue already verified, never of a
/// decoded image — nothing is downloaded to find out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    /// Shaped like the screen, give or take [`MAX_TRIM`].
    Screen,
    /// The screen's shape, or anything between it and a 4:5 upright.
    NearSquare,
    #[default]
    Any,
}

/// How far a painting's proportions may stray from the screen's and still count
/// as the same shape — the same number as Android's `Screen.MAX_TRIM`.
pub const MAX_TRIM: f64 = 0.15;
/// The near-square limits either side of 1:1, from Android's `ArtworkShape`.
const NEAR_SQUARE_MIN: f64 = 0.8;
const NEAR_SQUARE_MAX: f64 = 1.25;

impl Shape {
    pub const ALL: [Shape; 3] = [Shape::Screen, Shape::NearSquare, Shape::Any];

    pub fn label(self) -> &'static str {
        match self {
            Shape::Screen => "Screen-shaped",
            Shape::NearSquare => "Near square",
            Shape::Any => "Any shape",
        }
    }

    /// Whether a painting `aspect` (width ÷ height) wide belongs on a screen
    /// `screen` wide.
    pub fn accepts(self, aspect: f64, screen: f64) -> bool {
        if !aspect.is_finite() || aspect <= 0.0 {
            return false;
        }
        match self {
            Shape::Screen => 1.0 - aspect.min(screen) / aspect.max(screen) <= MAX_TRIM,
            Shape::NearSquare if screen >= 1.0 => {
                (NEAR_SQUARE_MIN..=screen * (1.0 + MAX_TRIM)).contains(&aspect)
            }
            Shape::NearSquare => (screen * (1.0 - MAX_TRIM)..=NEAR_SQUARE_MAX).contains(&aspect),
            Shape::Any => true,
        }
    }
}

/// How a picture is hung on the desktop.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Style {
    /// The whole picture, with the margins painted one colour.
    Borders { colour: Border },
    /// Fill the screen, cropping what does not fit.
    Zoom,
    /// Fill the screen, distorting to fit.
    Stretch,
    /// The picture over a blurred copy of itself, or the blurred copy alone.
    Blur { variant: BlurVariant, strength: u8 },
}

impl Default for Style {
    fn default() -> Self {
        Style::Borders {
            colour: Border::Black,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Border {
    Black,
    /// The average of the picture's outer edge, measured afresh for every picture.
    Auto,
    Custom {
        rgb: [u8; 3],
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlurVariant {
    Backdrop,
    WholeImage,
}

/// Android's default blur strength, and its default custom border colour.
pub const DEFAULT_BLUR_STRENGTH: u8 = 50;
pub const DEFAULT_CUSTOM_BORDER: [u8; 3] = [0x34, 0x34, 0xc8];

impl Settings {
    /// Reads the window's last choices, or the defaults if there are none.
    ///
    /// A file that will not parse is treated as absent rather than as an error:
    /// nobody typed it, so there is nobody to tell about a typo, and the defaults
    /// are exactly what the program did before the file existed.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self)?)
            .with_context(|| format!("writing {}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_program_before_settings_existed() {
        let s = Settings::default();
        assert_eq!(s.filters.subjects, vec![Subject::Landscape]);
        assert!(s.filters.regions.is_empty());
        assert_eq!(s.filters.shape, Shape::Any);
        assert_eq!(
            s.style,
            Style::Borders {
                colour: Border::Black
            }
        );
    }

    #[test]
    fn round_trips_through_json() {
        let s = Settings {
            filters: Filters {
                regions: vec![Region::Asia],
                subjects: vec![Subject::Seascape, Subject::StillLife],
                artists: vec!["Mikhail Vrubel".into()],
                hide_religious: true,
                shape: Shape::NearSquare,
            },
            style: Style::Blur {
                variant: BlurVariant::WholeImage,
                strength: 70,
            },
        };
        let text = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<Settings>(&text).unwrap(), s);
    }

    #[test]
    fn a_partial_file_keeps_the_rest_at_default() {
        let s: Settings = serde_json::from_str(r#"{"style":{"kind":"zoom"}}"#).unwrap();
        assert_eq!(s.style, Style::Zoom);
        assert_eq!(s.filters, Filters::default());
    }

    #[test]
    fn screen_shape_allows_the_trim_and_no_more() {
        let screen = 16.0 / 10.0;
        assert!(Shape::Screen.accepts(1.5, screen));
        assert!(!Shape::Screen.accepts(1.0, screen));
        assert!(Shape::Any.accepts(0.3, screen));
    }

    #[test]
    fn near_square_on_a_wide_screen_runs_from_four_by_five_to_the_screen() {
        let screen = 16.0 / 9.0;
        assert!(Shape::NearSquare.accepts(0.8, screen));
        assert!(Shape::NearSquare.accepts(1.0, screen));
        assert!(Shape::NearSquare.accepts(1.9, screen));
        assert!(!Shape::NearSquare.accepts(0.7, screen));
        assert!(!Shape::NearSquare.accepts(2.2, screen));
    }
}
