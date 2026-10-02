//! How a picture meets the screen: the one place in the program that looks at
//! pixels.
//!
//! Most styles are something every desktop can already do. Fitting with coloured
//! margins, zooming to fill and stretching are placement options macOS, GNOME and
//! Windows each apply per display, which is also the only way to get them right on
//! a desk of mismatched monitors. So [`resolve`] mostly *translates*: it answers a
//! [`Hang`] naming the original file and an option. The pixels are read for two
//! reasons only — to measure the colour of a picture's edge for automatic borders,
//! and to compose a picture, which no desktop offers and so has to be drawn here
//! and handed over as a new file. A blurred backdrop is one such composition, and
//! [`cover_to`] and [`cover_to_fit`], for a video-call background that is not a
//! desktop at all, are the others.
//!
//! [`Preview`] draws every style, in miniature, with the same code, so what the
//! settings window shows is what [`resolve`] would hang.
//!
//! Rotation never comes here. A painting is downloaded, and its path handed on,
//! without anything decoding it; only hanging it does.

use crate::settings::{BlurVariant, Border, Style};
use anyhow::{bail, Context, Result};
use image::codecs::jpeg::JpegEncoder;
use image::imageops::{self, FilterType};
use image::{Rgba, RgbaImage};
use std::path::{Path, PathBuf};

/// How a desktop is to place [`Hang::path`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The whole picture, margins filled with [`Hang::colour`].
    Fit,
    /// Cover the screen, cropping the overflow.
    Fill,
    /// Cover the screen, distorting to fit.
    Stretch,
}

/// A file ready for the desktop, and how the desktop should place it.
#[derive(Debug, Clone, PartialEq)]
pub struct Hang {
    pub path: PathBuf,
    pub mode: Mode,
    /// Only seen in [`Mode::Fit`]'s margins, but always set, so that a backend
    /// never has to invent one.
    pub colour: [u8; 3],
}

/// The blurred canvas is this many pixels on its short side before it is scaled
/// up — small enough that the blur is cheap, and the scaling is itself part of the
/// softness. Android's `BLUR_SHORT_EDGE`.
const BLUR_SHORT_EDGE: u32 = 256;
/// The box-blur radius at full strength, on that canvas. Android's
/// `MAX_BLUR_RADIUS`.
const MAX_BLUR_RADIUS: u32 = 24;
/// How small a copy the edge colour is measured on. Averaging a band is no more
/// accurate for being done on forty million pixels.
const EDGE_SAMPLE: u32 = 160;
/// The long side of the copy [`Preview`] keeps.
const PREVIEW_SOURCE: u32 = 720;
/// The JPEG qualities [`cover_to_fit`] tries, best first. Below the last the
/// blocks start to show on a painting, and a smaller size looks better than that.
const FIT_QUALITIES: [u8; 6] = [88, 80, 72, 64, 56, 48];

/// Turns `style` into something the desktop can hang on a `screen` of that many
/// pixels, drawing a new picture into `scratch` only if the style needs one.
///
/// A composed picture is written to one of two alternating names, never the
/// original's: macOS remembers placement by path, and every desktop caches the
/// image behind one, so reusing a name would leave some displays showing the last
/// composition. The other name is left alone rather than deleted, because it may
/// still be what a Space waiting on a redraw is recorded as showing.
pub fn resolve(path: &Path, style: &Style, screen: (u32, u32), scratch: &Path) -> Result<Hang> {
    let hang = |mode, colour| Hang {
        path: path.to_path_buf(),
        mode,
        colour,
    };
    Ok(match *style {
        Style::Borders {
            colour: Border::Black,
        } => hang(Mode::Fit, [0, 0, 0]),
        Style::Borders {
            colour: Border::Custom { rgb },
        } => hang(Mode::Fit, rgb),
        Style::Borders {
            colour: Border::Auto,
        } => {
            let sample = decode(path)?.thumbnail(EDGE_SAMPLE, EDGE_SAMPLE).to_rgba8();
            hang(Mode::Fit, edge_colour(&sample))
        }
        Style::Zoom => hang(Mode::Fill, [0, 0, 0]),
        Style::Stretch => hang(Mode::Stretch, [0, 0, 0]),
        Style::Blur { variant, strength } => {
            let source = decode(path)?.to_rgba8();
            let (w, h) = (screen.0.max(1), screen.1.max(1));
            let composed = blur(&source, w, h, variant, strength);
            let out = next_scratch(scratch);
            std::fs::create_dir_all(scratch)?;
            image::DynamicImage::ImageRgba8(composed)
                .to_rgb8()
                .save_with_format(&out, image::ImageFormat::Jpeg)
                .with_context(|| format!("writing {}", out.display()))?;
            Hang {
                path: out,
                mode: Mode::Fill,
                colour: [0, 0, 0],
            }
        }
    })
}

/// Writes `path` cropped to cover exactly `size` pixels, as a PNG at `out`.
///
/// For a picture some other program will hang for itself, where there is no desktop
/// to ask for fill: a meeting app's virtual background takes whatever file it is
/// given and shows it at its own size, so the crop has to be made here. Centred,
/// like [`Style::Zoom`], and never distorted. The directory of `out` must exist.
pub fn cover_to(path: &Path, size: (u32, u32), out: &Path) -> Result<()> {
    let (w, h) = (size.0.max(1), size.1.max(1));
    let source = decode(path)?.to_rgba8();
    image::DynamicImage::ImageRgba8(cover(&source, w, h))
        .to_rgb8()
        .save_with_format(out, image::ImageFormat::Png)
        .with_context(|| format!("writing {}", out.display()))
}

/// `path` cropped to cover one of `sizes` and encoded as a JPEG of at most `budget`
/// bytes, in memory.
///
/// For a place that will take a picture only if it is no bigger than the file it
/// replaces. Sizes are tried in the order given, so callers pass the largest first,
/// and within each the qualities from best to worst: the first encoding that fits
/// wins, which makes it the sharpest picture the budget allows rather than merely
/// a small one. The picture is decoded once. Errors when not even the last size at
/// the lowest quality fits.
pub fn cover_to_fit(path: &Path, sizes: &[(u32, u32)], budget: usize) -> Result<Vec<u8>> {
    let source = decode(path)?.to_rgba8();
    for &(w, h) in sizes {
        let rgb = image::DynamicImage::ImageRgba8(cover(&source, w.max(1), h.max(1))).to_rgb8();
        for quality in FIT_QUALITIES {
            let mut jpeg = Vec::new();
            JpegEncoder::new_with_quality(&mut jpeg, quality)
                .encode_image(&rgb)
                .context("encoding a JPEG")?;
            if jpeg.len() <= budget {
                return Ok(jpeg);
            }
        }
    }
    bail!("the picture does not fit in {budget} bytes at any size tried")
}

/// A small copy of one picture, kept so the settings window can redraw it in any
/// style as fast as someone clicks.
pub struct Preview {
    source: RgbaImage,
    edge: [u8; 3],
}

impl Preview {
    pub fn new(path: &Path) -> Result<Self> {
        let source = decode(path)?
            .thumbnail(PREVIEW_SOURCE, PREVIEW_SOURCE)
            .to_rgba8();
        let edge = edge_colour(&imageops::thumbnail(&source, EDGE_SAMPLE, EDGE_SAMPLE));
        Ok(Self { source, edge })
    }

    /// The picture as `style` would hang it on a screen `aspect` wide, drawn
    /// `width` pixels across.
    pub fn render(&self, style: &Style, aspect: f64, width: u32) -> RgbaImage {
        let w = width.max(1);
        let h = ((f64::from(w) / aspect).round() as u32).max(1);
        match *style {
            Style::Borders { colour } => {
                let rgb = match colour {
                    Border::Black => [0, 0, 0],
                    Border::Auto => self.edge,
                    Border::Custom { rgb } => rgb,
                };
                let mut canvas = RgbaImage::from_pixel(w, h, opaque(rgb));
                overlay_fit(&mut canvas, &self.source);
                canvas
            }
            Style::Zoom => cover(&self.source, w, h),
            Style::Stretch => imageops::resize(&self.source, w, h, FilterType::Triangle),
            Style::Blur { variant, strength } => blur(&self.source, w, h, variant, strength),
        }
    }
}

fn decode(path: &Path) -> Result<image::DynamicImage> {
    image::ImageReader::open(path)
        .with_context(|| format!("opening {}", path.display()))?
        .with_guessed_format()?
        .decode()
        .with_context(|| format!("decoding {}", path.display()))
}

/// The two names a composed picture alternates between; the one written least
/// recently is the one to overwrite.
fn next_scratch(scratch: &Path) -> PathBuf {
    let [a, b] = ["rendered-a.jpg", "rendered-b.jpg"].map(|n| scratch.join(n));
    let modified = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    match (modified(&a), modified(&b)) {
        (None, _) => a,
        (_, None) => b,
        (Some(ta), Some(tb)) if ta <= tb => a,
        _ => b,
    }
}

/// The picture blurred to fill `w`×`h`, with the sharp picture fitted on top
/// unless `variant` asks for the blur alone. Android's `WallpaperRenderer` blur.
fn blur(source: &RgbaImage, w: u32, h: u32, variant: BlurVariant, strength: u8) -> RgbaImage {
    let scale = f64::from(BLUR_SHORT_EDGE) / f64::from(w.min(h));
    let (sw, sh) = if scale < 1.0 {
        (
            ((f64::from(w) * scale).round() as u32).max(1),
            ((f64::from(h) * scale).round() as u32).max(1),
        )
    } else {
        (w, h)
    };
    let radius = (f64::from(MAX_BLUR_RADIUS) * f64::from(strength.min(100)) / 100.0).round() as u32;
    let small = box_blur(cover(source, sw, sh), radius);
    let mut canvas = imageops::resize(&small, w, h, FilterType::Triangle);
    if variant == BlurVariant::Backdrop {
        overlay_fit(&mut canvas, source);
    }
    canvas
}

/// `source` scaled to cover `w`×`h` and cropped to it, centred.
fn cover(source: &RgbaImage, w: u32, h: u32) -> RgbaImage {
    let (sw, sh) = (f64::from(source.width()), f64::from(source.height()));
    let scale = (f64::from(w) / sw).max(f64::from(h) / sh);
    let (rw, rh) = (
        ((sw * scale).round() as u32).max(w),
        ((sh * scale).round() as u32).max(h),
    );
    let resized = imageops::resize(source, rw, rh, FilterType::Triangle);
    imageops::crop_imm(&resized, (rw - w) / 2, (rh - h) / 2, w, h).to_image()
}

/// Draws `source` onto `canvas`, scaled to fit entirely and centred.
fn overlay_fit(canvas: &mut RgbaImage, source: &RgbaImage) {
    let (cw, ch) = (f64::from(canvas.width()), f64::from(canvas.height()));
    let (sw, sh) = (f64::from(source.width()), f64::from(source.height()));
    let scale = (cw / sw).min(ch / sh);
    let (fw, fh) = (
        ((sw * scale).round() as u32).clamp(1, canvas.width()),
        ((sh * scale).round() as u32).clamp(1, canvas.height()),
    );
    let fitted = imageops::resize(source, fw, fh, FilterType::Triangle);
    let x = i64::from((canvas.width() - fw) / 2);
    let y = i64::from((canvas.height() - fh) / 2);
    imageops::replace(canvas, &fitted, x, y);
}

/// The average colour of the outer five percent of `image`. Android's
/// `edgeAverageColor`.
fn edge_colour(image: &RgbaImage) -> [u8; 3] {
    let (w, h) = image.dimensions();
    if w == 0 || h == 0 {
        return [0, 0, 0];
    }
    let band = ((f64::from(w.min(h)) * 0.05).ceil() as u32).max(1);
    let mut sum = [0u64; 3];
    let mut count = 0u64;
    for (x, y, px) in image.enumerate_pixels() {
        if x < band || x >= w - band || y < band || y >= h - band {
            for (s, c) in sum.iter_mut().zip(px.0) {
                *s += u64::from(c);
            }
            count += 1;
        }
    }
    sum.map(|s| (s / count.max(1)) as u8)
}

fn opaque([r, g, b]: [u8; 3]) -> Rgba<u8> {
    Rgba([r, g, b, 255])
}

/// Three passes of a horizontal then vertical box blur, edges clamped — close
/// enough to a Gaussian, and Android's `blurPixels` exactly.
fn box_blur(mut image: RgbaImage, radius: u32) -> RgbaImage {
    if radius == 0 {
        return image;
    }
    for _ in 0..3 {
        image = blur_pass(&image, radius, true);
        image = blur_pass(&image, radius, false);
    }
    image
}

fn blur_pass(source: &RgbaImage, radius: u32, horizontal: bool) -> RgbaImage {
    let (w, h) = source.dimensions();
    let (len, lines) = if horizontal { (w, h) } else { (h, w) };
    let at = |line: u32, i: i64| {
        let i = i.clamp(0, i64::from(len) - 1) as u32;
        if horizontal {
            source.get_pixel(i, line).0
        } else {
            source.get_pixel(line, i).0
        }
    };
    let r = i64::from(radius);
    let divisor = u64::from(radius * 2 + 1);
    let mut out = RgbaImage::new(w, h);
    for line in 0..lines {
        let mut sum = [0u64; 4];
        for i in -r..=r {
            for (s, c) in sum.iter_mut().zip(at(line, i)) {
                *s += u64::from(c);
            }
        }
        for i in 0..len {
            let px = Rgba(sum.map(|s| (s / divisor) as u8));
            if horizontal {
                out.put_pixel(i, line, px);
            } else {
                out.put_pixel(line, i, px);
            }
            let leaving = at(line, i64::from(i) - r);
            let entering = at(line, i64::from(i) + r + 1);
            for k in 0..4 {
                sum[k] = sum[k] + u64::from(entering[k]) - u64::from(leaving[k]);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::DEFAULT_BLUR_STRENGTH;

    fn framed() -> RgbaImage {
        // A red picture in a 10-pixel blue frame.
        RgbaImage::from_fn(200, 100, |x, y| {
            if x < 10 || y < 10 || x >= 190 || y >= 90 {
                Rgba([0, 0, 255, 255])
            } else {
                Rgba([255, 0, 0, 255])
            }
        })
    }

    #[test]
    fn edge_colour_reads_the_frame_not_the_middle() {
        assert_eq!(edge_colour(&framed()), [0, 0, 255]);
    }

    #[test]
    fn only_blur_draws_a_new_picture() {
        let dir = std::env::temp_dir().join("art-window-placement-test");
        let path = Path::new("/nonexistent/picture.jpg");
        let fit = resolve(path, &Style::default(), (100, 100), &dir).unwrap();
        assert_eq!(
            (fit.mode, fit.colour, fit.path.as_path()),
            (Mode::Fit, [0, 0, 0], path)
        );
        let custom = Style::Borders {
            colour: Border::Custom { rgb: [1, 2, 3] },
        };
        assert_eq!(
            resolve(path, &custom, (100, 100), &dir).unwrap().colour,
            [1, 2, 3]
        );
        assert_eq!(
            resolve(path, &Style::Zoom, (100, 100), &dir).unwrap().mode,
            Mode::Fill
        );
        assert_eq!(
            resolve(path, &Style::Stretch, (100, 100), &dir)
                .unwrap()
                .mode,
            Mode::Stretch
        );
    }

    #[test]
    fn a_composed_blur_is_screen_sized_and_alternates_names() {
        let dir = std::env::temp_dir().join(format!("art-window-blur-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("painting.png");
        framed().save(&src).unwrap();
        let style = Style::Blur {
            variant: BlurVariant::Backdrop,
            strength: DEFAULT_BLUR_STRENGTH,
        };
        let first = resolve(&src, &style, (320, 180), &dir).unwrap();
        let second = resolve(&src, &style, (320, 180), &dir).unwrap();
        assert_eq!(first.mode, Mode::Fill);
        assert_ne!(first.path, second.path);
        let size = image::image_dimensions(&second.path).unwrap();
        assert_eq!(size, (320, 180));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cover_to_writes_exactly_the_size_asked_for() {
        let dir = std::env::temp_dir().join(format!("art-window-cover-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("painting.png");
        framed().save(&src).unwrap();
        let out = dir.join("covered.png");
        cover_to(&src, (160, 90), &out).unwrap();
        assert_eq!(image::image_dimensions(&out).unwrap(), (160, 90));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A picture of pseudo-random noise, which no encoder can make small.
    fn noisy() -> RgbaImage {
        RgbaImage::from_fn(400, 300, |x, y| {
            let h = x.wrapping_mul(2_654_435_761) ^ y.wrapping_mul(40_503).rotate_left(13);
            let h = h.wrapping_mul(2_246_822_519);
            Rgba([(h >> 8) as u8, (h >> 16) as u8, (h >> 24) as u8, 255])
        })
    }

    fn fit_source(name: &str) -> (PathBuf, PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("art-window-fit-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("painting.png");
        noisy().save(&src).unwrap();
        (dir, src)
    }

    #[test]
    fn cover_to_fit_takes_the_first_size_that_fits() {
        let (dir, src) = fit_source("first");
        let jpeg = cover_to_fit(&src, &[(320, 180), (160, 90)], usize::MAX).unwrap();
        assert_eq!(image::load_from_memory(&jpeg).unwrap().width(), 320);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cover_to_fit_falls_to_a_smaller_size_within_the_budget() {
        let (dir, src) = fit_source("fall");
        let small = cover_to_fit(&src, &[(160, 90)], usize::MAX).unwrap();
        let jpeg = cover_to_fit(&src, &[(320, 180), (160, 90)], small.len()).unwrap();
        assert!(jpeg.len() <= small.len());
        assert_eq!(image::load_from_memory(&jpeg).unwrap().width(), 160);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cover_to_fit_errors_when_nothing_fits() {
        let (dir, src) = fit_source("none");
        assert!(cover_to_fit(&src, &[(320, 180), (160, 90)], 10).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn preview_matches_the_screen_shape() {
        let dir = std::env::temp_dir().join(format!("art-window-preview-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("painting.png");
        framed().save(&src).unwrap();
        let preview = Preview::new(&src).unwrap();
        for style in [
            Style::default(),
            Style::Zoom,
            Style::Stretch,
            Style::Blur {
                variant: BlurVariant::WholeImage,
                strength: 100,
            },
        ] {
            assert_eq!(
                preview.render(&style, 16.0 / 10.0, 160).dimensions(),
                (160, 100)
            );
        }
        // Black borders above and below a 2:1 picture on a 16:10 screen.
        let fitted = preview.render(&Style::default(), 16.0 / 10.0, 160);
        assert_eq!(fitted.get_pixel(80, 0).0, [0, 0, 0, 255]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn box_blur_keeps_a_flat_colour_flat() {
        let flat = RgbaImage::from_pixel(40, 30, Rgba([10, 20, 30, 255]));
        assert_eq!(box_blur(flat.clone(), 5), flat);
    }
}
