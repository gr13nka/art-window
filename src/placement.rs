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
//! and handed over as a new file. A blurred backdrop is one such composition; a
//! painting that has been zoomed or moved off-centre is another, since a desktop
//! can centre a crop but cannot be told which part to show; and [`cover_to`] and
//! [`cover_to_fit`], for a video-call background that is not a desktop at all,
//! are the rest.
//!
//! Where a framed painting sits is [`frame`]'s answer and nobody else's — the
//! same geometry as Android's `Screen.frame` and iOS's `Framing.rect`. [`Preview`]
//! draws every style, in miniature, through it and with the same code as
//! [`resolve`], so what the settings window shows is what would be hung.
//!
//! Rotation never comes here. A painting is downloaded, and its path handed on,
//! without anything decoding it; only hanging it does.

use crate::settings::{BlurVariant, Border, Frame, Framing, Style, CENTRED, MAX_ZOOM};
use anyhow::{bail, Context, Result};
use image::codecs::jpeg::JpegEncoder;
use image::imageops::{self, FilterType};
use image::{Rgba, RgbaImage};
use std::cell::RefCell;
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
/// The short side below which [`Preview`] stops halving its copy: nothing is
/// drawn smaller than this, so a smaller copy would never be picked.
const SMALLEST_LEVEL: u32 = 128;
/// The long side of the copy [`Preview`] keeps. Enough that a painting zoomed to
/// [`MAX_ZOOM`] in the window's preview is still a picture and not a smear.
const PREVIEW_SOURCE: u32 = 1600;
/// The JPEG qualities [`cover_to_fit`] tries, best first. Below the last the
/// blocks start to show on a painting, and a smaller size looks better than that.
const FIT_QUALITIES: [u8; 6] = [88, 80, 72, 64, 56, 48];

/// Turns `style` and `framing` into something the desktop can hang on a `screen`
/// of that many pixels, drawing a new picture into `scratch` only if one is
/// needed: for a blur, and for a painting zoomed or moved from where its style
/// would centre it. Anything else is left to the desktop, original file and all.
///
/// A composed picture is written to one of two alternating names, never the
/// original's: macOS remembers placement by path, and every desktop caches the
/// image behind one, so reusing a name would leave some displays showing the last
/// composition. The other name is left alone rather than deleted, because it may
/// still be what a Space waiting on a redraw is recorded as showing.
pub fn resolve(
    path: &Path,
    style: &Style,
    framing: &Framing,
    screen: (u32, u32),
    scratch: &Path,
) -> Result<Hang> {
    let hang = |mode, colour| Hang {
        path: path.to_path_buf(),
        mode,
        colour,
    };
    let (w, h) = (screen.0.max(1), screen.1.max(1));
    let frame = framing.for_painting(path);
    if let Some(base) = frame_base(style).filter(|_| !frame.is_plain()) {
        // A framing can be other than plain and still move nothing — a pan along
        // an axis the painting does not overflow — and then the desktop's own
        // placement is as good and costs no decoding. The size is in the header.
        let size = image::image_dimensions(path)
            .with_context(|| format!("measuring {}", path.display()))?;
        if self::frame(size, (w, h), base, frame)
            != self::frame(size, (w, h), base, Frame::default())
        {
            let source = decode(path)?.to_rgba8();
            let composed = match *style {
                Style::Blur { variant, strength } => blur(&source, w, h, variant, strength, frame),
                Style::Borders { colour } => {
                    let rgb = match colour {
                        Border::Black => [0, 0, 0],
                        Border::Custom { rgb } => rgb,
                        Border::Auto => {
                            edge_colour(&imageops::thumbnail(&source, EDGE_SAMPLE, EDGE_SAMPLE))
                        }
                    };
                    framed(&source, w, h, rgb, base, frame)
                }
                _ => framed(&source, w, h, [0, 0, 0], base, frame),
            };
            return compose(composed, scratch);
        }
    }
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
            return compose(
                blur(&source, w, h, variant, strength, Frame::default()),
                scratch,
            );
        }
    })
}

/// Writes a picture drawn here, already screen-sized, where the desktop can hang
/// it — under the next of the two alternating names.
fn compose(picture: RgbaImage, scratch: &Path) -> Result<Hang> {
    let out = next_scratch(scratch);
    std::fs::create_dir_all(scratch)?;
    image::DynamicImage::ImageRgba8(picture)
        .to_rgb8()
        .save_with_format(&out, image::ImageFormat::Jpeg)
        .with_context(|| format!("writing {}", out.display()))?;
    Ok(Hang {
        path: out,
        mode: Mode::Fill,
        colour: [0, 0, 0],
    })
}

/// The size a painting has before any zoom: filling the screen, or fitted inside
/// it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Base {
    Cover,
    Fit,
}

/// Which base `style` frames its painting from, or `None` for a style with no
/// sharp picture to move: a stretched one fills the screen whatever is done to
/// it, and a blur of the whole image has no edges to bring into view.
pub fn frame_base(style: &Style) -> Option<Base> {
    match style {
        Style::Zoom => Some(Base::Cover),
        Style::Borders { .. } => Some(Base::Fit),
        Style::Blur {
            variant: BlurVariant::Backdrop,
            ..
        } => Some(Base::Fit),
        Style::Blur {
            variant: BlurVariant::WholeImage,
            ..
        }
        | Style::Stretch => None,
    }
}

/// Where a painting's rectangle sits relative to the screen, in the screen's
/// pixels. Its corner may be off the screen, hence negative.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameRect {
    pub left: i64,
    pub top: i64,
    pub width: u32,
    pub height: u32,
}

fn base_scale(painting: (u32, u32), screen: (u32, u32), base: Base) -> f64 {
    let across = f64::from(screen.0) / f64::from(painting.0.max(1));
    let down = f64::from(screen.1) / f64::from(painting.1.max(1));
    match base {
        Base::Cover => across.max(down),
        Base::Fit => across.min(down),
    }
}

/// Where a `painting` of that many pixels sits on a `screen` when framed so: the
/// one geometry the desktop is composed with at its own size and the window's
/// preview at its, which is the only reason the two agree.
///
/// Along an axis the painting overflows, the pan chooses which part shows and the
/// painting always reaches both edges of the screen — no blank strip. Along one
/// it does not overflow it stays centred and the pan does nothing.
pub fn frame(painting: (u32, u32), screen: (u32, u32), base: Base, frame: Frame) -> FrameRect {
    let scale = base_scale(painting, screen, base) * f64::from(frame.zoom);
    // The epsilon keeps an exact fit from becoming one pixel too wide.
    let scaled = |side: u32| ((f64::from(side) * scale - 1e-6).ceil() as u32).max(1);
    let (width, height) = (scaled(painting.0), scaled(painting.1));
    FrameRect {
        left: offset(screen.0, width, frame.pan_x),
        top: offset(screen.1, height, frame.pan_y),
        width,
        height,
    }
}

/// `frame` after a gesture: the painting scaled by `scale_change` about the point
/// `at` on the screen — so the part under the pointer stays under it — and then
/// moved by `by` pixels, exactly as far as the pointer moved. Stops at the
/// painting's edges and at the zoom limits.
pub fn reframe(
    painting: (u32, u32),
    screen: (u32, u32),
    base: Base,
    framed: Frame,
    scale_change: f32,
    at: (f32, f32),
    by: (f32, f32),
) -> Frame {
    // A gesture that reports nonsense changes nothing, rather than leaving a NaN
    // where every later calculation would inherit it — `clamp` passes one straight
    // through, and on Android that once pinned a painting to its edge for good.
    if ![scale_change, at.0, at.1, by.0, by.1]
        .iter()
        .all(|v| v.is_finite())
    {
        return framed;
    }
    let zoom = (framed.zoom * scale_change).clamp(1.0, MAX_ZOOM);
    let ratio = zoom / framed.zoom;
    let before = frame(painting, screen, base, framed);
    let after = frame(painting, screen, base, Frame { zoom, ..framed });
    // Where the pan puts the painting exactly, not the whole pixel `frame` draws
    // it at. A pointer sends a fraction of a pixel per event, and starting each
    // event from the rounded position loses that fraction in one direction.
    let left = at.0 - (at.0 - exact_offset(screen.0, before.width, framed.pan_x)) * ratio + by.0;
    let top = at.1 - (at.1 - exact_offset(screen.1, before.height, framed.pan_y)) * ratio + by.1;
    Frame {
        zoom,
        pan_x: pan_for(screen.0, after.width, left),
        pan_y: pan_for(screen.1, after.height, top),
    }
}

fn offset(screen: u32, painting: u32, pan: f32) -> i64 {
    if painting > screen {
        -((f64::from(painting - screen) * f64::from(pan.clamp(0.0, 1.0))) as i64)
    } else {
        i64::from((screen - painting) / 2)
    }
}

/// [`offset`] before it is rounded to a pixel.
fn exact_offset(screen: u32, painting: u32, pan: f32) -> f32 {
    if painting > screen {
        -((painting - screen) as f32) * pan.clamp(0.0, 1.0)
    } else {
        (screen - painting) as f32 / 2.0
    }
}

fn pan_for(screen: u32, painting: u32, left: f32) -> f32 {
    if painting > screen {
        (-left / (painting - screen) as f32).clamp(0.0, 1.0)
    } else {
        CENTRED
    }
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
/// style as fast as someone clicks — and, since the picture can be dragged about,
/// as fast as a pointer moves, which is a hundred times a second and a good deal
/// less forgiving.
pub struct Preview {
    source: RgbaImage,
    /// `source` again at half the size, a quarter, and so on down. Drawing picks
    /// the smallest that is still at least as large as it will appear, so that
    /// four neighbouring pixels are always enough to make one — see
    /// [`Preview::draw`].
    smaller: Vec<RgbaImage>,
    edge: [u8; 3],
    /// The last blurred backdrop drawn and what it was drawn for. Dragging the
    /// painting about over a blur asks for the same backdrop at every step, and
    /// the blur is the only slow thing here.
    backdrop: RefCell<Option<(BackdropKey, RgbaImage)>>,
}

/// What a blurred backdrop depends on: the canvas's width and height, and the
/// blur's strength.
type BackdropKey = (u32, u32, u8);

impl Preview {
    pub fn new(path: &Path) -> Result<Self> {
        let source = decode(path)?
            .thumbnail(PREVIEW_SOURCE, PREVIEW_SOURCE)
            .to_rgba8();
        let edge = edge_colour(&imageops::thumbnail(&source, EDGE_SAMPLE, EDGE_SAMPLE));
        let mut smaller: Vec<RgbaImage> = Vec::new();
        loop {
            let last = smaller.last().unwrap_or(&source);
            let (w, h) = (last.width() / 2, last.height() / 2);
            if w.min(h) < SMALLEST_LEVEL {
                break;
            }
            smaller.push(imageops::thumbnail(last, w, h));
        }
        Ok(Self {
            source,
            smaller,
            edge,
            backdrop: RefCell::new(None),
        })
    }

    /// Draws the picture onto `canvas` where [`frame`] puts it — what
    /// [`draw_framed`] does, at a speed a drag can keep up with.
    ///
    /// The general resampling `draw_framed` uses takes a tenth of a second at
    /// this size, which is ten frames of a pointer's movement going unanswered.
    /// Blending the four pixels around each point is some thirty times quicker,
    /// and loses nothing so long as the copy it reads is not much larger than
    /// what it draws, which choosing among the halved copies sees to.
    fn draw(&self, canvas: &mut RgbaImage, base: Base, framed: Frame) {
        let (cw, ch) = canvas.dimensions();
        let rect = frame(self.size(), (cw, ch), base, framed);
        let level = self
            .smaller
            .iter()
            .rev()
            .find(|level| level.width() >= rect.width && level.height() >= rect.height)
            .unwrap_or(&self.source);
        let (lw, lh) = level.dimensions();
        if lw == 0 || lh == 0 {
            return;
        }

        // The rectangle's visible part, in canvas pixels.
        let (x0, y0) = (rect.left.max(0), rect.top.max(0));
        let x1 = (rect.left + i64::from(rect.width)).min(i64::from(cw));
        let y1 = (rect.top + i64::from(rect.height)).min(i64::from(ch));
        if x1 <= x0 || y1 <= y0 {
            return;
        }

        // Where canvas pixel `at` falls in the copy, as the pixel before it, the
        // pixel after, and how far between them out of 256.
        let place = |at: i64, start: i64, extent: u32, side: u32| {
            let centre = (at - start) as f64 + 0.5;
            let pos = (centre * f64::from(side) / f64::from(extent) - 0.5)
                .clamp(0.0, f64::from(side - 1));
            let before = pos.floor() as usize;
            let after = (before + 1).min(side as usize - 1);
            (before, after, ((pos - before as f64) * 256.0) as u32)
        };
        let columns: Vec<_> = (x0..x1)
            .map(|x| place(x, rect.left, rect.width, lw))
            .collect();

        let from = level.as_raw();
        let stride = lw as usize * 4;
        let row_len = cw as usize * 4;
        let into: &mut [u8] = canvas;
        for y in y0..y1 {
            let (above, below, down) = place(y, rect.top, rect.height, lh);
            let (above, below) = (&from[above * stride..], &from[below * stride..]);
            let row = &mut into[y as usize * row_len..][x0 as usize * 4..x1 as usize * 4];
            for (px, &(before, after, across)) in row.chunks_exact_mut(4).zip(&columns) {
                let (before, after) = (before * 4, after * 4);
                for k in 0..3 {
                    let top = u32::from(above[before + k]) * (256 - across)
                        + u32::from(above[after + k]) * across;
                    let bottom = u32::from(below[before + k]) * (256 - across)
                        + u32::from(below[after + k]) * across;
                    px[k] = ((top * (256 - down) + bottom * down) >> 16) as u8;
                }
                px[3] = 255;
            }
        }
    }

    /// The picture framed on a `w`×`h` canvas of one colour.
    fn framed(&self, w: u32, h: u32, rgb: [u8; 3], base: Base, frame: Frame) -> RgbaImage {
        let mut canvas = flat(w, h, rgb);
        self.draw(&mut canvas, base, frame);
        canvas
    }

    /// The pixel size of the copy kept — the painting's proportions, which is all
    /// [`frame`] and [`reframe`] need of it.
    pub fn size(&self) -> (u32, u32) {
        self.source.dimensions()
    }

    /// The canvas `width` pixels across at a screen `aspect` wide.
    pub fn canvas(aspect: f64, width: u32) -> (u32, u32) {
        let w = width.max(1);
        (w, ((f64::from(w) / aspect).round() as u32).max(1))
    }

    /// The picture as `style` would hang it, framed so, on a screen `aspect`
    /// wide, drawn `width` pixels across.
    pub fn render(&self, style: &Style, frame: Frame, aspect: f64, width: u32) -> RgbaImage {
        let (w, h) = Self::canvas(aspect, width);
        match *style {
            Style::Borders { colour } => {
                let rgb = match colour {
                    Border::Black => [0, 0, 0],
                    Border::Auto => self.edge,
                    Border::Custom { rgb } => rgb,
                };
                self.framed(w, h, rgb, Base::Fit, frame)
            }
            Style::Zoom => self.framed(w, h, [0, 0, 0], Base::Cover, frame),
            Style::Stretch => imageops::resize(&self.source, w, h, FilterType::Triangle),
            Style::Blur { variant, strength } => {
                let key = (w, h, strength);
                let mut kept = self.backdrop.borrow_mut();
                if kept.as_ref().map(|(k, _)| *k) != Some(key) {
                    *kept = Some((key, blurred(&self.source, w, h, strength)));
                }
                let mut canvas = kept.as_ref().expect("just filled").1.clone();
                if variant == BlurVariant::Backdrop {
                    self.draw(&mut canvas, Base::Fit, frame);
                }
                canvas
            }
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

/// The picture blurred to fill `w`×`h`, with the sharp picture framed on top
/// unless `variant` asks for the blur alone. Android's `WallpaperRenderer` blur.
fn blur(
    source: &RgbaImage,
    w: u32,
    h: u32,
    variant: BlurVariant,
    strength: u8,
    frame: Frame,
) -> RgbaImage {
    let mut canvas = blurred(source, w, h, strength);
    if variant == BlurVariant::Backdrop {
        draw_framed(&mut canvas, source, Base::Fit, frame);
    }
    canvas
}

/// The blurred backdrop alone, `w`×`h`.
fn blurred(source: &RgbaImage, w: u32, h: u32, strength: u8) -> RgbaImage {
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
    imageops::resize(&small, w, h, FilterType::Triangle)
}

/// `source` framed on a `w`×`h` canvas of one colour, which shows wherever the
/// painting does not reach.
fn framed(source: &RgbaImage, w: u32, h: u32, rgb: [u8; 3], base: Base, frame: Frame) -> RgbaImage {
    let mut canvas = flat(w, h, rgb);
    draw_framed(&mut canvas, source, base, frame);
    canvas
}

/// A `w`×`h` canvas of one colour.
fn flat(w: u32, h: u32, [r, g, b]: [u8; 3]) -> RgbaImage {
    let pixels = [r, g, b, 255].repeat(w as usize * h as usize);
    RgbaImage::from_raw(w, h, pixels).expect("as many bytes as the size asks for")
}

/// Draws `source` onto `canvas` where [`frame`] puts it.
///
/// Only the part that lands on the canvas is ever scaled: a painting zoomed
/// threefold on a large display is otherwise a picture several times the size of
/// that display, made in full and then mostly thrown away.
fn draw_framed(canvas: &mut RgbaImage, source: &RgbaImage, base: Base, framed: Frame) {
    let (cw, ch) = canvas.dimensions();
    let (sw, sh) = source.dimensions();
    if sw == 0 || sh == 0 {
        return;
    }
    let rect = frame((sw, sh), (cw, ch), base, framed);
    // The rectangle's visible part, in canvas pixels.
    let (x0, y0) = (rect.left.max(0), rect.top.max(0));
    let x1 = (rect.left + i64::from(rect.width)).min(i64::from(cw));
    let y1 = (rect.top + i64::from(rect.height)).min(i64::from(ch));
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    // The same part, in the source's pixels.
    let (fx, fy) = (
        f64::from(sw) / f64::from(rect.width),
        f64::from(sh) / f64::from(rect.height),
    );
    let crop_x = (((x0 - rect.left) as f64 * fx).floor() as u32).min(sw - 1);
    let crop_y = (((y0 - rect.top) as f64 * fy).floor() as u32).min(sh - 1);
    let crop_w = ((((x1 - x0) as f64) * fx).round() as u32).clamp(1, sw - crop_x);
    let crop_h = ((((y1 - y0) as f64) * fy).round() as u32).clamp(1, sh - crop_y);
    let visible = imageops::crop_imm(source, crop_x, crop_y, crop_w, crop_h).to_image();
    let scaled = imageops::resize(
        &visible,
        (x1 - x0) as u32,
        (y1 - y0) as u32,
        FilterType::Triangle,
    );
    imageops::replace(canvas, &scaled, x0, y0);
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

    fn framed_picture() -> RgbaImage {
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
        assert_eq!(edge_colour(&framed_picture()), [0, 0, 255]);
    }

    #[test]
    fn only_blur_draws_a_new_picture() {
        let dir = std::env::temp_dir().join("art-window-placement-test");
        let path = Path::new("/nonexistent/picture.jpg");
        let plain = Framing::default();
        let fit = resolve(path, &Style::default(), &plain, (100, 100), &dir).unwrap();
        assert_eq!(
            (fit.mode, fit.colour, fit.path.as_path()),
            (Mode::Fit, [0, 0, 0], path)
        );
        let custom = Style::Borders {
            colour: Border::Custom { rgb: [1, 2, 3] },
        };
        assert_eq!(
            resolve(path, &custom, &plain, (100, 100), &dir)
                .unwrap()
                .colour,
            [1, 2, 3]
        );
        assert_eq!(
            resolve(path, &Style::Zoom, &plain, (100, 100), &dir)
                .unwrap()
                .mode,
            Mode::Fill
        );
        assert_eq!(
            resolve(path, &Style::Stretch, &plain, (100, 100), &dir)
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
        framed_picture().save(&src).unwrap();
        let style = Style::Blur {
            variant: BlurVariant::Backdrop,
            strength: DEFAULT_BLUR_STRENGTH,
        };
        let first = resolve(&src, &style, &Framing::default(), (320, 180), &dir).unwrap();
        let second = resolve(&src, &style, &Framing::default(), (320, 180), &dir).unwrap();
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
        framed_picture().save(&src).unwrap();
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
        framed_picture().save(&src).unwrap();
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
                preview
                    .render(&style, Frame::default(), 16.0 / 10.0, 160)
                    .dimensions(),
                (160, 100)
            );
        }
        // Black borders above and below a 2:1 picture on a 16:10 screen.
        let fitted = preview.render(&Style::default(), Frame::default(), 16.0 / 10.0, 160);
        assert_eq!(fitted.get_pixel(80, 0).0, [0, 0, 0, 255]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // The framing cases below are Android's `ScreenTest`, on purpose: the three
    // apps keep their own copies of this geometry and these are what hold the
    // copies together.
    const PHONE: (u32, u32) = (1080, 2340);
    const WIDE: (u32, u32) = (4000, 2000);

    fn at(zoom: f32, pan_x: f32, pan_y: f32) -> Frame {
        Frame { zoom, pan_x, pan_y }
    }

    #[test]
    fn at_zoom_one_and_centred_the_frame_is_a_centred_cover_or_fit() {
        // Cover: 4000x2000 fills 2340 down, so it is 4680 across and centred.
        let cover = frame(WIDE, PHONE, Base::Cover, Frame::default());
        assert_eq!((cover.width, cover.height), (4680, 2340));
        assert_eq!((cover.left, cover.top), (-(4680 - 1080) / 2, 0));

        // Fit: 1080 across, 540 down, centred.
        let fit = frame(WIDE, PHONE, Base::Fit, Frame::default());
        assert_eq!((fit.width, fit.height), (1080, 540));
        assert_eq!((fit.left, fit.top), (0, (2340 - 540) / 2));
    }

    #[test]
    fn a_zoomed_fitted_painting_overflows_one_axis_and_pan_puts_it_at_each_edge() {
        let left = frame(WIDE, PHONE, Base::Fit, at(3.0, 0.0, 0.0));
        let middle = frame(WIDE, PHONE, Base::Fit, at(3.0, 0.5, 0.5));
        let right = frame(WIDE, PHONE, Base::Fit, at(3.0, 1.0, 1.0));

        assert_eq!(left.width, 3240);
        assert_eq!(left.left, 0);
        assert_eq!(middle.left, -(3240 - 1080) / 2);
        assert_eq!(right.left, 1080 - 3240);
        // 1620 is shorter than the screen, so it stays centred whatever the pan.
        assert_eq!(left.top, (2340 - 1620) / 2);
        assert_eq!(right.top, (2340 - 1620) / 2);
    }

    #[test]
    fn pan_on_an_axis_that_does_not_overflow_changes_nothing() {
        assert_eq!(
            frame(WIDE, PHONE, Base::Fit, at(1.0, 0.0, 0.0)),
            frame(WIDE, PHONE, Base::Fit, at(1.0, 1.0, 1.0)),
        );
        assert_eq!(
            frame(WIDE, PHONE, Base::Fit, at(2.0, 0.0, 0.0)),
            frame(WIDE, PHONE, Base::Fit, at(2.0, 0.0, 1.0)),
        );
    }

    #[test]
    fn an_overflowing_axis_always_reaches_both_screen_edges() {
        for zoom in [1.0, 1.5, 2.0, 3.0] {
            for pan in [0.0, 0.3, 0.5, 1.0] {
                let rect = frame(WIDE, PHONE, Base::Cover, at(zoom, pan, pan));
                assert!(rect.left <= 0 && rect.left + i64::from(rect.width) >= 1080);
                assert!(rect.top <= 0 && rect.top + i64::from(rect.height) >= 2340);
            }
        }
    }

    #[test]
    fn a_preview_scale_frame_is_the_screen_scale_frame_scaled_down() {
        let framing = at(2.0, 0.25, 0.75);
        let full = frame(WIDE, PHONE, Base::Cover, framing);
        let small = frame(WIDE, (270, 585), Base::Cover, framing);
        let near = |a: f64, b: f64| (a - b).abs() <= 1.0;
        assert!(near(f64::from(full.width) / 4.0, f64::from(small.width)));
        assert!(near(full.left as f64 / 4.0, small.left as f64));
        assert!(near(full.top as f64 / 4.0, small.top as f64));
    }

    #[test]
    fn dragging_moves_the_painting_by_exactly_the_pointer_and_stops_at_its_edge() {
        let start = at(2.0, 0.5, 0.5);
        let before = frame(WIDE, PHONE, Base::Cover, start);
        let moved = reframe(
            WIDE,
            PHONE,
            Base::Cover,
            start,
            1.0,
            (500.0, 500.0),
            (100.0, 0.0),
        );
        let after = frame(WIDE, PHONE, Base::Cover, moved);
        assert!((after.left - (before.left + 100)).abs() <= 1);

        let past = reframe(
            WIDE,
            PHONE,
            Base::Cover,
            start,
            1.0,
            (500.0, 500.0),
            (1e6, 1e6),
        );
        assert_eq!((past.pan_x, past.pan_y), (0.0, 0.0));
    }

    #[test]
    fn a_slow_drag_adds_up_whichever_way_it_goes() {
        let preview = (400, 880);
        for step in [-0.4f32, 0.4] {
            let mut framing = at(2.0, 0.5, 0.5);
            let before = frame(WIDE, preview, Base::Cover, framing);
            for _ in 0..200 {
                framing = reframe(
                    WIDE,
                    preview,
                    Base::Cover,
                    framing,
                    1.0,
                    (200.0, 440.0),
                    (step, step),
                );
            }
            let after = frame(WIDE, preview, Base::Cover, framing);
            let moved = f64::from(step) * 200.0;
            assert!(((after.left - before.left) as f64 - moved).abs() <= 1.5);
            assert!(((after.top - before.top) as f64 - moved).abs() <= 1.5);
        }
    }

    #[test]
    fn a_gesture_that_reports_nonsense_changes_nothing() {
        let start = at(2.0, 0.3, 0.7);
        let same = reframe(
            WIDE,
            PHONE,
            Base::Cover,
            start,
            1.0,
            (f32::NAN, f32::NAN),
            (0.0, 0.0),
        );
        assert_eq!(same, start);
        let same = reframe(
            WIDE,
            PHONE,
            Base::Cover,
            start,
            f32::INFINITY,
            (0.0, 0.0),
            (0.0, 0.0),
        );
        assert_eq!(same, start);
    }

    #[test]
    fn zooming_keeps_the_point_under_the_pointer_and_stays_within_the_limits() {
        let start = Frame::default();
        let before = frame(WIDE, PHONE, Base::Cover, start);
        let zoomed = reframe(
            WIDE,
            PHONE,
            Base::Cover,
            start,
            2.0,
            (800.0, 1200.0),
            (0.0, 0.0),
        );
        let after = frame(WIDE, PHONE, Base::Cover, zoomed);
        let under = |r: FrameRect| (800.0 - r.left as f64) / f64::from(r.width);
        assert!((under(before) - under(after)).abs() < 0.002);

        let most = reframe(
            WIDE,
            PHONE,
            Base::Cover,
            start,
            50.0,
            (0.0, 0.0),
            (0.0, 0.0),
        );
        assert_eq!(most.zoom, MAX_ZOOM);
        let least = reframe(
            WIDE,
            PHONE,
            Base::Cover,
            at(2.0, 0.5, 0.5),
            0.01,
            (0.0, 0.0),
            (0.0, 0.0),
        );
        assert_eq!(least.zoom, 1.0);
    }

    #[test]
    fn stretch_and_a_blur_of_the_whole_picture_cannot_be_framed() {
        let blur = |variant| Style::Blur {
            variant,
            strength: 50,
        };
        assert_eq!(frame_base(&Style::Zoom), Some(Base::Cover));
        assert_eq!(frame_base(&Style::default()), Some(Base::Fit));
        assert_eq!(frame_base(&blur(BlurVariant::Backdrop)), Some(Base::Fit));
        assert_eq!(frame_base(&blur(BlurVariant::WholeImage)), None);
        assert_eq!(frame_base(&Style::Stretch), None);
    }

    /// Left half red, right half blue, twice as wide as tall.
    fn halves() -> RgbaImage {
        RgbaImage::from_fn(400, 200, |x, _| {
            if x < 200 {
                Rgba([255, 0, 0, 255])
            } else {
                Rgba([0, 0, 255, 255])
            }
        })
    }

    #[test]
    fn a_plain_frame_draws_what_the_centred_cover_always_drew() {
        let source = framed_picture();
        let was = cover(&source, 160, 100);
        let now = framed(&source, 160, 100, [0, 0, 0], Base::Cover, Frame::default());
        assert_eq!(now.dimensions(), was.dimensions());
        for (x, y) in [(0, 0), (80, 50), (159, 99), (10, 90), (150, 5)] {
            let (a, b) = (was.get_pixel(x, y).0, now.get_pixel(x, y).0);
            for k in 0..3 {
                assert!(a[k].abs_diff(b[k]) <= 24, "{a:?} against {b:?} at {x},{y}");
            }
        }
    }

    #[test]
    fn the_previews_quick_drawing_agrees_with_the_careful_one() {
        let dir = std::env::temp_dir().join(format!("art-window-quick-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("painting.png");
        // Large enough that the preview keeps halved copies and has to choose.
        imageops::resize(&halves(), 1200, 600, FilterType::Nearest)
            .save(&src)
            .unwrap();
        let preview = Preview::new(&src).unwrap();
        assert!(!preview.smaller.is_empty());

        for (base, style) in [(Base::Cover, Style::Zoom), (Base::Fit, Style::default())] {
            for framing in [Frame::default(), at(2.0, 0.2, 0.5), at(3.0, 1.0, 0.0)] {
                let quick = preview.render(&style, framing, 16.0 / 10.0, 320);
                let careful = framed(&preview.source, 320, 200, [0, 0, 0], base, framing);
                assert_eq!(quick.dimensions(), careful.dimensions());
                // Away from the seam between the two halves, where one pixel's
                // difference in where the edge falls is a whole colour's.
                for (x, y) in [
                    (8, 100),
                    (100, 100),
                    (220, 100),
                    (311, 100),
                    (160, 4),
                    (160, 196),
                ] {
                    let (a, b) = (quick.get_pixel(x, y).0, careful.get_pixel(x, y).0);
                    let far = (0..3).any(|k| a[k].abs_diff(b[k]) > 40);
                    let seam = a[0].min(a[2]) > 20 || b[0].min(b[2]) > 20;
                    assert!(
                        !far || seam,
                        "{a:?} against {b:?} at {x},{y} for {framing:?}"
                    );
                }
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pan_chooses_which_end_of_the_painting_a_zoomed_cover_shows() {
        // A 2:1 picture on a square screen overflows across at zoom 1.
        let left = framed(
            &halves(),
            100,
            100,
            [0, 0, 0],
            Base::Cover,
            at(1.0, 0.0, 0.5),
        );
        let right = framed(
            &halves(),
            100,
            100,
            [0, 0, 0],
            Base::Cover,
            at(1.0, 1.0, 0.5),
        );
        assert_eq!(left.get_pixel(50, 50).0, [255, 0, 0, 255]);
        assert_eq!(right.get_pixel(50, 50).0, [0, 0, 255, 255]);
    }

    #[test]
    fn only_a_framing_that_moves_the_painting_is_composed() {
        let dir = std::env::temp_dir().join(format!("art-window-framed-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("painting.png");
        halves().save(&src).unwrap();
        let mut framing = Framing::default();

        // Zoomed: drawn here, at the screen's size, and hung to fill it.
        framing.set(at(2.0, 0.0, 0.5), &src);
        let zoomed = resolve(&src, &Style::Zoom, &framing, (320, 200), &dir).unwrap();
        assert_ne!(zoomed.path, src);
        assert_eq!(zoomed.mode, Mode::Fill);
        assert_eq!(image::image_dimensions(&zoomed.path).unwrap(), (320, 200));

        // Borders at zoom 1 overflow nothing, so a pan moves nothing and the
        // desktop still places the original itself.
        framing.set(at(1.0, 0.0, 0.0), &src);
        let fitted = resolve(&src, &Style::default(), &framing, (320, 200), &dir).unwrap();
        assert_eq!(
            (fitted.path.as_path(), fitted.mode),
            (src.as_path(), Mode::Fit)
        );

        // Another painting is not this framing's business, beyond the zoom.
        framing.set(at(1.0, 0.0, 0.5), Path::new("somebody-else.png"));
        let other = resolve(&src, &Style::Zoom, &framing, (320, 200), &dir).unwrap();
        assert_eq!(other.path, src);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn box_blur_keeps_a_flat_colour_flat() {
        let flat = RgbaImage::from_pixel(40, 30, Rgba([10, 20, 30, 255]));
        assert_eq!(box_blur(flat.clone(), 5), flat);
    }
}
