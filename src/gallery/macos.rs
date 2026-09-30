//! What is inside the favourites window.
//!
//! A column of thumbnails on the left, and on the right whichever of them is being
//! looked at, large, with what it is called and the two things that can be done
//! about it.
//!
//! The column is one custom view rather than a control per picture. It is the
//! scroll view's document view, it works out for itself which row a click landed
//! in, and it is what the two buttons are aimed at — three jobs, but one object,
//! because all three are the same question of which picture is meant.

use super::{Control, Pending, Pick, Snapshot, StyleKind, Tab};
use crate::art::Artwork;
use crate::favourites::Favourites;
use crate::settings::{BlurVariant, Border};
use anyhow::{anyhow, Result};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{
    define_class, msg_send, sel, AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, Message,
};
use objc2_app_kit::{
    NSAppearance, NSAppearanceCustomization, NSAppearanceNameAqua, NSAutoresizingMaskOptions,
    NSBezierPath, NSBitmapImageRep, NSBorderType, NSButton, NSCalibratedRGBColorSpace, NSColor,
    NSColorSpace, NSColorWell, NSCompositingOperation, NSEvent, NSFont, NSGraphicsContext, NSImage,
    NSImageScaling, NSImageView, NSScrollView, NSSegmentSwitchTracking, NSSegmentedControl,
    NSShadow, NSSlider, NSTextAlignment, NSTextField, NSView,
};
use objc2_foundation::{NSArray, NSPoint, NSRect, NSSize, NSString, NSURL};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::Path;
use std::rc::{Rc, Weak};
use tao::platform::macos::WindowExtMacOS;
use tao::window::Window;

pub(super) fn present(window: &Window) {
    window.set_visible(true);
    window.set_focus();
}

pub(super) fn close(_window: &Window) -> bool {
    false
}

/// How wide the column of thumbnails is. Fixed, so that the picture beside it gets
/// every point the window gains — and so that the column never has to be laid out
/// again once it is built.
const SHELF: f64 = 160.0;
/// The side of the square each thumbnail is fitted inside.
const THUMB: f64 = 120.0;
/// One row of the column: a thumbnail and the air around it.
const CELL: f64 = THUMB + 20.0;
/// How many pixels a thumbnail is drawn per point. Two is as dense as any Mac
/// display goes, so a thumbnail made at two is never short of pixels; on a display
/// that wants one it is merely scaled down, which costs a quarter of a megabyte and
/// looks right.
const RETINA: f64 = 2.0;
const PAD: f64 = 16.0;
const BUTTON_H: f64 = 28.0;
const LINE: f64 = 18.0;
const TITLE_H: f64 = 22.0;
/// Everything under the picture — two lines and two buttons, and the air between
/// them — which is the height the picture does not get.
const FOOT: f64 = PAD + BUTTON_H + 14.0 + LINE + 2.0 + TITLE_H + PAD;

/// One kept picture, as the window has it.
struct Card {
    key: String,
    art: Artwork,
}

/// The list as it stands, and the thumbnails made for it.
///
/// One cell and not two, because every reader of either wants both at once.
#[derive(Default)]
struct Shown {
    cards: Vec<Card>,
    /// Kept by key rather than by position, so that a list rebuilt around a
    /// deletion does not silently pair a picture with somebody else's thumbnail.
    /// Keys are unique by construction — see `Favourites::free_name`.
    thumbs: HashMap<String, Retained<NSImage>>,
}

struct Ivars {
    shown: RefCell<Shown>,
    selected: Cell<Option<usize>>,
    easel: Easel,
    on_pick: Rc<dyn Fn(Pick)>,
}

define_class!(
    // SAFETY:
    // - NSView imposes nothing on a subclass beyond living on the main thread,
    //   which the thread kind below states and the compiler then holds us to.
    // - `Shelf` does not implement `Drop`.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[ivars = Ivars]
    struct Shelf;

    impl Shelf {
        /// Rows are counted from the top, which is the only direction a list reads.
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            self.paint();
        }

        /// A click picks a picture even when the window was not the active one.
        ///
        /// The ordinary rule is that the first click into an inactive window only
        /// wakes it, and the person clicks again for what they actually wanted.
        /// That rule is wrong here: this program is an accessory and its window is
        /// hardly ever the active one, so obeying it would put an extra click in
        /// front of every single visit.
        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            let at = self.convertPoint_fromView(event.locationInWindow(), None);
            if at.y < 0.0 {
                return;
            }
            let row = (at.y / CELL) as usize;
            if row >= self.ivars().shown.borrow().cards.len() {
                return;
            }
            self.select(Some(row));
            // A second click on a picture already chosen is impatience, and means
            // the button beside it.
            if event.clickCount() >= 2 {
                self.ask(Pick::Show);
            }
        }

        #[unsafe(method(hangPicture:))]
        fn hang_picture(&self, _sender: Option<&AnyObject>) {
            self.ask(Pick::Show);
        }

        #[unsafe(method(forgetPicture:))]
        fn forget_picture(&self, _sender: Option<&AnyObject>) {
            self.ask(Pick::Forget);
        }
    }
);

impl Shelf {
    fn new(mtm: MainThreadMarker, easel: Easel, on_pick: Rc<dyn Fn(Pick)>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars {
            shown: RefCell::new(Shown::default()),
            selected: Cell::new(None),
            easel,
            on_pick,
        });
        let empty = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(SHELF, 0.0));
        unsafe { msg_send![super(this), initWithFrame: empty] }
    }

    /// Points the pane's buttons at this shelf.
    ///
    /// Separate from building them because the ring cannot be closed in one pass:
    /// the buttons live in the pane, the pane had to exist before the shelf that
    /// answers them, and so the shelf is the last of the three to be made. Nothing
    /// leaks by it — AppKit does not retain a target, so this is a ring and not a
    /// knot.
    fn take_the_buttons(&self) {
        let target: &AnyObject = self;
        let easel = &self.ivars().easel;
        unsafe {
            easel.show.setTarget(Some(target));
            easel.show.setAction(Some(sel!(hangPicture:)));
            easel.forget.setTarget(Some(target));
            easel.forget.setAction(Some(sel!(forgetPicture:)));
        }
    }

    /// Takes a new list, keeping what can be kept: the selection stays on the same
    /// painting where that painting is still there, and a thumbnail already made is
    /// never made twice.
    fn adopt(&self, cards: Vec<Card>) {
        let was = self.selected_key();

        {
            let mut shown = self.ivars().shown.borrow_mut();
            shown
                .thumbs
                .retain(|key, _| cards.iter().any(|card| &card.key == key));
            for card in &cards {
                if !shown.thumbs.contains_key(&card.key) {
                    if let Some(thumb) = thumbnail(&card.art.path) {
                        shown.thumbs.insert(card.key.clone(), thumb);
                    }
                }
            }
            shown.cards = cards;
        }

        let (row, rows) = {
            let shown = self.ivars().shown.borrow();
            let row = was
                .and_then(|key| shown.cards.iter().position(|card| card.key == key))
                .or_else(|| (!shown.cards.is_empty()).then_some(0));
            (row, shown.cards.len())
        };

        // The column is as tall as it needs to be and no taller; the scroll view
        // reads that height to decide whether there is anything to scroll.
        let width = self.frame().size.width;
        self.setFrameSize(NSSize::new(width, rows as f64 * CELL));
        self.select(row);
    }

    /// Picks out a row: lights it, and fills the pane beside it.
    fn select(&self, row: Option<usize>) {
        self.ivars().selected.set(row);
        {
            let shown = self.ivars().shown.borrow();
            let art = row
                .and_then(|row| shown.cards.get(row))
                .map(|card| &card.art);
            self.ivars().easel.point_at(art);
        }
        self.setNeedsDisplay(true);
    }

    /// The key of the picture in the pane, if there is one.
    fn selected_key(&self) -> Option<String> {
        let row = self.ivars().selected.get()?;
        let shown = self.ivars().shown.borrow();
        shown.cards.get(row).map(|card| card.key.clone())
    }

    /// Says what was asked of the picture in the pane.
    ///
    /// By key, and read out before anybody is told, because answering this will take
    /// the list apart underneath us.
    fn ask(&self, what: fn(String) -> Pick) {
        let Some(key) = self.selected_key() else {
            return;
        };
        (self.ivars().on_pick)(what(key));
    }

    /// Draws the column: one picture to a row, the chosen one on a lit ground.
    fn paint(&self) {
        let shown = self.ivars().shown.borrow();
        let chosen = self.ivars().selected.get();
        let width = self.frame().size.width;
        for (row, card) in shown.cards.iter().enumerate() {
            let cell = NSRect::new(
                NSPoint::new(0.0, row as f64 * CELL),
                NSSize::new(width, CELL),
            );
            if chosen == Some(row) {
                NSColor::selectedContentBackgroundColor().setFill();
                NSBezierPath::fillRect(cell);
            }
            let Some(thumb) = shown.thumbs.get(&card.key) else {
                continue;
            };
            let size = thumb.size();
            let into = NSRect::new(
                NSPoint::new(
                    cell.origin.x + (cell.size.width - size.width) / 2.0,
                    cell.origin.y + (cell.size.height - size.height) / 2.0,
                ),
                size,
            );
            // `respectFlipped` because this view counts from the top and the image
            // does not; without it every painting hangs upside down.
            unsafe {
                thumb.drawInRect_fromRect_operation_fraction_respectFlipped_hints(
                    into,
                    NSRect::ZERO,
                    NSCompositingOperation::SourceOver,
                    1.0,
                    true,
                    None,
                );
            }
        }
    }
}

/// The right-hand side: the picture being looked at, what it is called, and the two
/// things that can be done about it.
struct Easel {
    canvas: Retained<NSImageView>,
    title: Retained<NSTextField>,
    byline: Retained<NSTextField>,
    show: Retained<NSButton>,
    forget: Retained<NSButton>,
}

impl Easel {
    /// Builds the pane's contents into `pane`, leaving the buttons unaimed — see
    /// [`Shelf::take_the_buttons`].
    fn build(mtm: MainThreadMarker, pane: &NSView) -> Self {
        let size = pane.bounds().size;
        let wide = (size.width - PAD * 2.0).max(1.0);

        // The pane is not flipped, so all of this is measured up from its bottom
        // edge: the buttons sit on the floor and the picture takes what is left.
        let show = button(mtm, "Set as wallpaper", PAD, 168.0);
        let forget = button(mtm, "Forget", PAD + 168.0 + 8.0, 96.0);

        let byline = label(
            mtm,
            NSRect::new(
                NSPoint::new(PAD, PAD + BUTTON_H + 14.0),
                NSSize::new(wide, LINE),
            ),
            NSFont::systemFontOfSize(12.0),
            true,
        );
        let title = label(
            mtm,
            NSRect::new(
                NSPoint::new(PAD, PAD + BUTTON_H + 14.0 + LINE + 2.0),
                NSSize::new(wide, TITLE_H),
            ),
            NSFont::boldSystemFontOfSize(15.0),
            false,
        );

        let canvas = NSImageView::initWithFrame(
            NSImageView::alloc(mtm),
            NSRect::new(
                NSPoint::new(PAD, FOOT),
                NSSize::new(wide, (size.height - FOOT - PAD).max(1.0)),
            ),
        );
        // Fit and letterbox, which is the same thing the desktop does with it.
        canvas.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
        canvas.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );

        pane.addSubview(&canvas);
        pane.addSubview(&title);
        pane.addSubview(&byline);
        pane.addSubview(&show);
        pane.addSubview(&forget);

        Self {
            canvas,
            title,
            byline,
            show,
            forget,
        }
    }

    /// Points the pane at a picture, or empties it when there is none.
    ///
    /// This is the one place a painting is held at its full size, and only ever one
    /// at a time: handing the view a new image is what lets go of the last.
    fn point_at(&self, art: Option<&Artwork>) {
        match art {
            Some(art) => {
                let url = NSURL::fileURLWithPath(&NSString::from_str(&art.path.to_string_lossy()));
                let full = NSImage::initWithContentsOfURL(NSImage::alloc(), &url);
                self.canvas.setImage(full.as_deref());
                self.title.setStringValue(&NSString::from_str(&art.title));
                self.byline
                    .setStringValue(&NSString::from_str(if art.byline.is_empty() {
                        &art.attribution
                    } else {
                        &art.byline
                    }));
                self.show.setEnabled(true);
                self.forget.setEnabled(true);
            }
            None => {
                self.canvas.setImage(None);
                self.title
                    .setStringValue(&NSString::from_str("Nothing kept yet"));
                self.byline.setStringValue(&NSString::from_str(
                    "Add to favourites keeps the picture on the desktop",
                ));
                self.show.setEnabled(false);
                self.forget.setEnabled(false);
            }
        }
    }
}

/// A push button on the pane's floor, staying there as the window grows.
fn button(mtm: MainThreadMarker, title: &str, x: f64, width: f64) -> Retained<NSButton> {
    let button = unsafe {
        NSButton::buttonWithTitle_target_action(&NSString::from_str(title), None, None, mtm)
    };
    button.setFrame(NSRect::new(
        NSPoint::new(x, PAD),
        NSSize::new(width, BUTTON_H),
    ));
    button.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewMaxXMargin | NSAutoresizingMaskOptions::ViewMaxYMargin,
    );
    button
}

/// A line of text on the pane, widening with the window but staying at its foot.
fn label(
    mtm: MainThreadMarker,
    frame: NSRect,
    font: Retained<NSFont>,
    quiet: bool,
) -> Retained<NSTextField> {
    let label = NSTextField::labelWithString(&NSString::from_str(""), mtm);
    label.setFrame(frame);
    label.setFont(Some(&font));
    if quiet {
        label.setTextColor(Some(&NSColor::secondaryLabelColor()));
    }
    label.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewMaxYMargin,
    );
    label
}

/// A small copy of the picture at `path`, or nothing if it cannot be read.
///
/// The full painting is decoded to make it and let go of again before the next one
/// is read, so a shelf of any length costs one painting's worth of memory to build
/// rather than the whole folder's. AppKit does the decoding and the scaling: this
/// program has no image decoder and wants none, and drawing a picture into a
/// smaller picture is how you ask the one it already has.
///
/// The bitmap is made at [`RETINA`] times the size the thumbnail is drawn at, and
/// the rep is then *told* it measures the smaller amount. That pair — many pixels,
/// few points — is what a sharp image on a Retina display is, and it is also why
/// this does not use `NSImage::lockFocus`: locking focus draws at whatever the
/// screen happens to be, which is a thumbnail that is crisp or blurred depending on
/// which display the window was opened on.
fn thumbnail(path: &Path) -> Option<Retained<NSImage>> {
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    let full = NSImage::initWithContentsOfURL(NSImage::alloc(), &url)?;
    let size = full.size();
    if size.width < 1.0 || size.height < 1.0 {
        return None;
    }
    let scale = (THUMB / size.width).min(THUMB / size.height);
    let fitted = NSSize::new(
        (size.width * scale).round().max(1.0),
        (size.height * scale).round().max(1.0),
    );

    // SAFETY: a null `planes` asks AppKit to allocate the pixels itself, and a zero
    // `bytesPerRow`/`bitsPerPixel` asks it to work them out from the rest. Those are
    // the documented ways of spelling "you decide", not omissions.
    let rep = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            std::ptr::null_mut(),
            (fitted.width * RETINA) as isize,
            (fitted.height * RETINA) as isize,
            8,
            4,
            true,
            false,
            NSCalibratedRGBColorSpace,
            0,
            0,
        )
    }?;
    rep.setSize(fitted);

    let context = NSGraphicsContext::graphicsContextWithBitmapImageRep(&rep)?;
    NSGraphicsContext::saveGraphicsState_class();
    NSGraphicsContext::setCurrentContext(Some(&context));
    full.drawInRect_fromRect_operation_fraction(
        NSRect::new(NSPoint::new(0.0, 0.0), fitted),
        NSRect::ZERO,
        NSCompositingOperation::Copy,
        1.0,
    );
    NSGraphicsContext::restoreGraphicsState_class();

    let thumb = NSImage::initWithSize(NSImage::alloc(), fitted);
    thumb.addRepresentation(&rep);
    Some(thumb)
}

/// The window's contents: the shelf, which everything on the favourites tab hangs
/// off, and the settings tab beside it.
pub struct Content {
    shelf: Retained<Shelf>,
    ui: Rc<Ui>,
}

impl Content {
    /// Fills `window` with the shelf, the pane and the buttons, and aims the clicks
    /// at `on_pick`.
    ///
    /// Must run on the main thread, and says so with an error rather than a comment,
    /// for the same reason [`crate::desktop::pin`] does.
    pub fn install(
        window: &Window,
        on_pick: Rc<dyn Fn(Pick)>,
        _on_control: Rc<dyn Fn(Control)>,
    ) -> Result<Self> {
        let mtm = MainThreadMarker::new()
            .ok_or_else(|| anyhow!("the favourites window must be built on the main thread"))?;

        // tao owns the window and this view; everything below is a subview of it and
        // goes when it goes.
        let root: &NSView = unsafe { &*(window.ns_view() as *const NSView) };

        // A container of our own, so that the arithmetic below is written in
        // coordinates this file decides the orientation of rather than tao's.
        let outer = NSView::initWithFrame(NSView::alloc(mtm), root.bounds());
        outer.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        let total = outer.bounds().size;
        let below = NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(total.width, (total.height - STRIP).max(1.0)),
        );
        let fill = NSAutoresizingMaskOptions::ViewWidthSizable
            | NSAutoresizingMaskOptions::ViewHeightSizable;

        // The favourites tab: unchanged, only shorter by the strip above it.
        let whole = NSView::initWithFrame(NSView::alloc(mtm), below);
        whole.setAutoresizingMask(fill);
        let size = whole.bounds().size;

        let pane = NSView::initWithFrame(
            NSView::alloc(mtm),
            NSRect::new(
                NSPoint::new(SHELF, 0.0),
                NSSize::new((size.width - SHELF).max(1.0), size.height),
            ),
        );
        pane.setAutoresizingMask(fill);

        let easel = Easel::build(mtm, &pane);
        let shelf = Shelf::new(mtm, easel, on_pick.clone());

        let scroll = NSScrollView::initWithFrame(
            NSScrollView::alloc(mtm),
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(SHELF, size.height)),
        );
        scroll.setHasVerticalScroller(true);
        scroll.setAutohidesScrollers(true);
        scroll.setDrawsBackground(false);
        scroll.setBorderType(NSBorderType::NoBorder);
        // Fixed width, full height: the column keeps its size as the window grows,
        // which is why it never needs laying out again.
        scroll.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewHeightSizable
                | NSAutoresizingMaskOptions::ViewMaxXMargin,
        );
        shelf.setFrameSize(NSSize::new(scroll.contentSize().width, 0.0));
        scroll.setDocumentView(Some(&shelf));

        shelf.take_the_buttons();

        whole.addSubview(&scroll);
        whole.addSubview(&pane);

        // The settings tab, laid over the same area and hidden until asked for.
        let ui = Ui::build(mtm, below, on_pick, whole.clone());

        outer.addSubview(&whole);
        outer.addSubview(&ui.settings);
        outer.addSubview(&ui.segments);
        ui.segments.setFrame(NSRect::new(
            NSPoint::new((total.width - TABS_W) / 2.0, total.height - 8.0 - TABS_H),
            NSSize::new(TABS_W, TABS_H),
        ));
        root.addSubview(&outer);
        ui.show_tab(Tab::Favourites);

        Ok(Self { shelf, ui })
    }

    pub fn relist(&self, favourites: &Favourites) {
        self.shelf.adopt(
            favourites
                .iter()
                .map(|(key, art)| Card {
                    key: key.to_string(),
                    art: art.clone(),
                })
                .collect(),
        );
    }

    pub fn describe(&self, snapshot: &Snapshot, favourites: &Favourites) {
        self.relist(favourites);
        self.ui.adopt(snapshot);
    }

    /// Brings one of the two tabs forward.
    pub fn show_tab(&self, tab: Tab) {
        self.ui.show_tab(tab);
    }

    pub fn describe_status(&self, _snapshot: &Snapshot) {}

    pub fn set_login(&self, _enabled: bool) {}
}

// ---------------------------------------------------------------------------
// The settings tab.
//
// Everything decided here is `Pending`'s; this half only draws it and forwards
// clicks into it. Chips and cards are custom-drawn views (`Pill`) rather than
// buttons, because a filled capsule of arbitrary colour is not something an
// `NSButton` will be, and the section rows are rebuilt from the model after every
// click — the number of chips changes with what is chosen.
// ---------------------------------------------------------------------------

/// The strip above both tabs that holds the switch between them.
const TABS_H: f64 = 28.0;
const TABS_W: f64 = 220.0;
const STRIP: f64 = TABS_H + 16.0;
/// The bar under the sections, which stays put while they scroll.
const BAR: f64 = 64.0;
const OUTER: f64 = 24.0;
/// The preview takes about half the tab, within these widths. It is rendered once
/// at the larger, and drawn smaller when the window is.
const PREVIEW_MIN: f64 = 300.0;
const PREVIEW_MAX: f64 = 560.0;
/// Air left under the preview card for its shadow to fall into.
const SHADOW_BELOW: f64 = 30.0;
/// Space above a section's title, so that sections read as separate groups.
const SECTION_GAP: f64 = 28.0;
/// Between a tile and the ring round it, plus the ring's own thickness. A tile's
/// view reserves this much on every side so the ring is drawn inside it.
const RING_GAP: f64 = 3.0;
const RING_W: f64 = 2.5;
const RING: f64 = RING_GAP + RING_W;
const GAP: f64 = 8.0;
const CHIP_H: f64 = 30.0;

const WHITE: u32 = 0xffffff;
const INK: u32 = 0x1d1d1f;
const MUTED: u32 = 0x6e6e73;
const FILL: u32 = 0xeef0f3;
const BLUE: u32 = 0x1a5ce0;
const MEDIUM: f64 = 0.23;
const SEMIBOLD: f64 = 0.3;

fn rgb(hex: u32, alpha: f64) -> Retained<NSColor> {
    NSColor::colorWithSRGBRed_green_blue_alpha(
        ((hex >> 16) & 0xff) as f64 / 255.0,
        ((hex >> 8) & 0xff) as f64 / 255.0,
        (hex & 0xff) as f64 / 255.0,
        alpha,
    )
}

/// Every caller passes an `NSView` or one of its subclasses, which is what makes
/// widening the type sound.
fn as_view<T: Message>(view: Retained<T>) -> Retained<NSView> {
    unsafe { Retained::cast_unchecked(view) }
}

/// A single line of text at its natural size.
fn text_label(
    mtm: MainThreadMarker,
    text: &str,
    size: f64,
    weight: f64,
    hex: u32,
) -> Retained<NSTextField> {
    let label = NSTextField::labelWithString(&NSString::from_str(text), mtm);
    label.setFont(Some(&NSFont::systemFontOfSize_weight(size, weight)));
    label.setTextColor(Some(&rgb(hex, 1.0)));
    label.sizeToFit();
    label
}

/// How a pill is dressed. The palette is decided here and nowhere else: callers
/// say what a pill *is* (chosen, not chosen) and never what colour it is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Look {
    /// Solid blue with white text: the button that matters, and a chosen chip.
    Primary,
    /// White with a hairline border: a chip or tile that is not chosen.
    Plain,
    /// `Plain` with a blue ring standing off from it: the chosen tile. Needs a
    /// view that reserves [`RING`] on every side.
    Ringed,
}

struct PillIvars {
    look: Look,
    radius: f64,
    /// Room reserved round the shape, inside the view, for a ring to be drawn in.
    inset: f64,
    enabled: Cell<bool>,
    on_click: RefCell<Option<Rc<dyn Fn()>>>,
}

define_class!(
    // SAFETY: NSView imposes nothing on a subclass beyond the main thread, and
    // `Pill` does not implement `Drop`.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[ivars = PillIvars]
    struct Pill;

    impl Pill {
        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            let PillIvars {
                look,
                radius,
                inset,
                ..
            } = *self.ivars();
            let bounds = self.bounds();
            let shape = shrink(bounds, inset);
            match look {
                Look::Primary => rgb(BLUE, 1.0).setFill(),
                Look::Plain | Look::Ringed => rgb(WHITE, 1.0).setFill(),
            }
            NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(shape, radius, radius).fill();
            if look == Look::Primary {
                return;
            }
            // A stroke is centred on its path, so the path sits half a line inside
            // the edge to keep the whole hairline inside the shape.
            let hairline = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(
                shrink(shape, 0.5),
                radius - 0.5,
                radius - 0.5,
            );
            hairline.setLineWidth(1.0);
            rgb(0x000000, 0.12).setStroke();
            hairline.stroke();
            if look == Look::Ringed {
                let ring = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(
                    shrink(bounds, RING_W / 2.0),
                    radius + inset - RING_W / 2.0,
                    radius + inset - RING_W / 2.0,
                );
                ring.setLineWidth(RING_W);
                rgb(BLUE, 1.0).setStroke();
                ring.stroke();
            }
        }

        /// The text inside is only a picture of a word: the click belongs to the
        /// pill, so it must not be swallowed by the label sitting on top of it. Only
        /// the shape answers, not the room reserved round it, so that neighbours
        /// whose rings overlap in the gap between them do not fight over it.
        #[unsafe(method(hitTest:))]
        fn hit_test(&self, point: NSPoint) -> Option<&NSView> {
            let frame = shrink(self.frame(), self.ivars().inset);
            let inside = point.x >= frame.origin.x
                && point.x < frame.origin.x + frame.size.width
                && point.y >= frame.origin.y
                && point.y < frame.origin.y + frame.size.height;
            let this: &NSView = self;
            inside.then_some(this)
        }

        /// Accepted at once for the same reason the shelf does: an accessory's
        /// window is hardly ever the active one.
        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, _event: &NSEvent) {
            if !self.ivars().enabled.get() {
                return;
            }
            let click = self.ivars().on_click.borrow().clone();
            if let Some(click) = click {
                // The click usually rebuilds the row this pill is in, dropping it
                // from its superview; hold on to it until the answer has returned.
                let _keep = self.retain();
                click();
            }
        }
    }
);

/// `rect` with `by` taken off every side.
fn shrink(rect: NSRect, by: f64) -> NSRect {
    NSRect::new(
        NSPoint::new(rect.origin.x + by, rect.origin.y + by),
        NSSize::new(rect.size.width - by * 2.0, rect.size.height - by * 2.0),
    )
}

impl Pill {
    /// A pill with `text` on it. `shape` is the size of the visible shape, and a
    /// `width` of `None` fits the text plus `pad` either side; the view is larger
    /// by `inset` on every side.
    #[allow(clippy::too_many_arguments)]
    fn new(
        mtm: MainThreadMarker,
        text: &str,
        size: f64,
        look: Look,
        radius: f64,
        (width, height): (Option<f64>, f64),
        pad: f64,
        inset: f64,
        on_click: Option<Rc<dyn Fn()>>,
    ) -> Retained<Self> {
        let (ink, weight) = match look {
            Look::Primary => (WHITE, SEMIBOLD),
            Look::Plain | Look::Ringed => (INK, MEDIUM),
        };
        let label = text_label(mtm, text, size, weight, ink);
        let line = label.frame().size;
        let width = width.unwrap_or(line.width.ceil() + pad * 2.0);
        let this = Self::alloc(mtm).set_ivars(PillIvars {
            look,
            radius,
            inset,
            enabled: Cell::new(true),
            on_click: RefCell::new(on_click),
        });
        let frame = NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(width + inset * 2.0, height + inset * 2.0),
        );
        let pill: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
        label.setAlignment(NSTextAlignment::Center);
        label.setFrame(NSRect::new(
            NSPoint::new(inset, inset + ((height - line.height) / 2.0).floor()),
            NSSize::new(width, line.height),
        ));
        pill.addSubview(&label);
        pill
    }

    /// A pill that ignores clicks, dimmed to `dim` while it does.
    fn set_enabled(&self, on: bool, dim: f64) {
        self.ivars().enabled.set(on);
        self.setAlphaValue(if on { 1.0 } else { dim });
    }
}

struct PlateIvars {
    fill: u32,
    alpha: f64,
}

define_class!(
    // SAFETY: as for `Pill`.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[ivars = PlateIvars]
    struct Plate;

    impl Plate {
        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            rgb(self.ivars().fill, self.ivars().alpha).setFill();
            NSBezierPath::fillRect(self.bounds());
        }
    }
);

impl Plate {
    fn new(mtm: MainThreadMarker, frame: NSRect, fill: u32, alpha: f64) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(PlateIvars { fill, alpha });
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }
}

/// The settings tab's own surface: white, and it says when its size changes so
/// that the preview and the column can be laid out again for the new one.
struct PaneIvars {
    size: Cell<NSSize>,
    resized: RefCell<Option<Rc<dyn Fn()>>>,
}

define_class!(
    // SAFETY: as for `Pill`.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[ivars = PaneIvars]
    struct Pane;

    impl Pane {
        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            rgb(WHITE, 1.0).setFill();
            NSBezierPath::fillRect(self.bounds());
        }

        #[unsafe(method(setFrameSize:))]
        fn set_frame_size(&self, size: NSSize) {
            let _: () = unsafe { msg_send![super(self), setFrameSize: size] };
            let before = self.ivars().size.get();
            if size.width != before.width || size.height != before.height {
                self.ivars().size.set(size);
                let resized = self.ivars().resized.borrow().clone();
                if let Some(resized) = resized {
                    resized();
                }
            }
        }
    }
);

impl Pane {
    fn new(mtm: MainThreadMarker, frame: NSRect) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(PaneIvars {
            size: Cell::new(frame.size),
            resized: RefCell::new(None),
        });
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }
}

struct CanvasIvars {
    image: RefCell<Option<Retained<NSImage>>>,
    /// How wide the card is drawn; the picture is scaled down to it.
    card_w: Cell<f64>,
    none_yet: Retained<NSTextField>,
}

define_class!(
    // SAFETY: as for `Pill`.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[ivars = CanvasIvars]
    struct Canvas;

    impl Canvas {
        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            let card = self.card();
            if card.size.height < 1.0 {
                return;
            }
            let path = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(card, 10.0, 10.0);

            // The shadow is cast by a white fill, so it is there whether or not the
            // picture on top of it is.
            NSGraphicsContext::saveGraphicsState_class();
            let shadow = NSShadow::new();
            shadow.setShadowOffset(NSSize::new(0.0, -6.0));
            shadow.setShadowBlurRadius(24.0);
            shadow.setShadowColor(Some(&rgb(0x000000, 0.10)));
            shadow.set();
            rgb(WHITE, 1.0).setFill();
            path.fill();
            NSGraphicsContext::restoreGraphicsState_class();

            match &*self.ivars().image.borrow() {
                Some(image) => {
                    NSGraphicsContext::saveGraphicsState_class();
                    path.addClip();
                    image.drawInRect_fromRect_operation_fraction(
                        card,
                        NSRect::ZERO,
                        NSCompositingOperation::SourceOver,
                        1.0,
                    );
                    NSGraphicsContext::restoreGraphicsState_class();
                }
                None => {
                    rgb(FILL, 1.0).setFill();
                    path.fill();
                }
            }
        }
    }
);

impl Canvas {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let none_yet = text_label(mtm, "No picture yet", 13.0, 0.0, MUTED);
        none_yet.setAlignment(NSTextAlignment::Center);
        let this = Self::alloc(mtm).set_ivars(CanvasIvars {
            image: RefCell::new(None),
            card_w: Cell::new(PREVIEW_MIN),
            none_yet,
        });
        let canvas: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: NSRect::ZERO] };
        canvas.addSubview(&canvas.ivars().none_yet);
        canvas
    }

    /// Where the card itself sits in the view: the margins are its shadow's room.
    fn card(&self) -> NSRect {
        let height = self.bounds().size.height;
        NSRect::new(
            NSPoint::new(OUTER, SHADOW_BELOW),
            NSSize::new(
                self.ivars().card_w.get(),
                (height - OUTER - SHADOW_BELOW).max(0.0),
            ),
        )
    }

    /// Sizes the view to a card of `card` size, hung from the top of `above`.
    fn place(&self, above: f64, card: NSSize) {
        let total = OUTER + card.height + SHADOW_BELOW;
        self.ivars().card_w.set(card.width);
        self.setFrame(NSRect::new(
            NSPoint::new(0.0, above - total),
            NSSize::new(OUTER * 2.0 + card.width, total),
        ));
        let card = self.card();
        let note = &self.ivars().none_yet;
        let line = note.frame().size.height;
        note.setFrame(NSRect::new(
            NSPoint::new(
                card.origin.x,
                card.origin.y + (card.size.height - line) / 2.0,
            ),
            NSSize::new(card.size.width, line),
        ));
        self.setNeedsDisplay(true);
    }

    fn set_image(&self, image: Option<Retained<NSImage>>) {
        self.ivars().none_yet.setHidden(image.is_some());
        *self.ivars().image.borrow_mut() = image;
        self.setNeedsDisplay(true);
    }
}

/// A picture made from the model's RGBA bytes, at the same many-pixels-few-points
/// ratio the thumbnails use, so the preview is as sharp as they are.
fn preview_image(pixels: &image::RgbaImage) -> Option<Retained<NSImage>> {
    let (w, h) = pixels.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    // SAFETY: as in `thumbnail`; the explicit row length is the one the copy below
    // assumes.
    let rep = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            std::ptr::null_mut(),
            w as isize,
            h as isize,
            8,
            4,
            true,
            false,
            NSCalibratedRGBColorSpace,
            (w * 4) as isize,
            32,
        )
    }?;
    let data = rep.bitmapData();
    let bytes = pixels.as_raw();
    if data.is_null() || bytes.len() != (w * h * 4) as usize {
        return None;
    }
    // SAFETY: the rep owns `w * 4 * h` bytes at `data`, and `bytes` is exactly that
    // long, checked above.
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), data, bytes.len()) };
    let points = NSSize::new(PREVIEW_MAX, PREVIEW_MAX * h as f64 / w as f64);
    rep.setSize(points);
    let image = NSImage::initWithSize(NSImage::alloc(), points);
    image.addRepresentation(&rep);
    Some(image)
}

/// A document view that counts rows from the top, and says when its width changes
/// so the chips can be wrapped afresh.
struct DocIvars {
    width: Cell<f64>,
    resized: RefCell<Option<Rc<dyn Fn()>>>,
}

define_class!(
    // SAFETY: as for `Pill`.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[ivars = DocIvars]
    struct Doc;

    impl Doc {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        #[unsafe(method(setFrameSize:))]
        fn set_frame_size(&self, size: NSSize) {
            let _: () = unsafe { msg_send![super(self), setFrameSize: size] };
            if size.width != self.ivars().width.get() {
                self.ivars().width.set(size.width);
                let resized = self.ivars().resized.borrow().clone();
                if let Some(resized) = resized {
                    resized();
                }
            }
        }
    }
);

impl Doc {
    fn new(mtm: MainThreadMarker, frame: NSRect) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(DocIvars {
            width: Cell::new(frame.size.width),
            resized: RefCell::new(None),
        });
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }
}

struct ActionIvars {
    ui: RefCell<Weak<Ui>>,
}

define_class!(
    // SAFETY: NSObject imposes nothing on a subclass, and `Actions` does not
    // implement `Drop`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = ActionIvars]
    struct Actions;

    /// What the native controls — the ones that are not a pill — aim at.
    impl Actions {
        #[unsafe(method(tabChanged:))]
        fn tab_changed(&self, sender: Option<&AnyObject>) {
            let Some(control) = sender.and_then(|s| s.downcast_ref::<NSSegmentedControl>()) else {
                return;
            };
            let tab = if control.selectedSegment() == 1 {
                Tab::Settings
            } else {
                Tab::Favourites
            };
            if let Some(ui) = self.ui() {
                ui.show_tab(tab);
            }
        }

        /// Fires when the slider is let go rather than all the way along, so the
        /// preview is redrawn once and the slider is never rebuilt under the hand
        /// that holds it.
        #[unsafe(method(blurStrength:))]
        fn blur_strength(&self, sender: Option<&AnyObject>) {
            let Some(slider) = sender.and_then(|s| s.downcast_ref::<NSSlider>()) else {
                return;
            };
            let strength = slider.doubleValue().round().clamp(0.0, 100.0) as u8;
            if let Some(ui) = self.ui() {
                ui.change_light(|p| p.set_blur_strength(strength));
            }
        }

        #[unsafe(method(colourPicked:))]
        fn colour_picked(&self, sender: Option<&AnyObject>) {
            let Some(well) = sender.and_then(|s| s.downcast_ref::<NSColorWell>()) else {
                return;
            };
            let Some(colour) = well
                .color()
                .colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())
            else {
                return;
            };
            let byte = |c: f64| (c * 255.0).round().clamp(0.0, 255.0) as u8;
            let rgb = [
                byte(colour.redComponent()),
                byte(colour.greenComponent()),
                byte(colour.blueComponent()),
            ];
            if let Some(ui) = self.ui() {
                ui.change_light(|p| p.set_border(Border::Custom { rgb }));
            }
        }

        #[unsafe(method(religiousToggled:))]
        fn religious_toggled(&self, sender: Option<&AnyObject>) {
            let Some(button) = sender.and_then(|s| s.downcast_ref::<NSButton>()) else {
                return;
            };
            let on = button.state() == 1;
            if let Some(ui) = self.ui() {
                ui.change(|p| p.set_hide_religious(on));
            }
        }
    }
);

impl Actions {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(ActionIvars {
            ui: RefCell::new(Weak::new()),
        });
        unsafe { msg_send![super(this), init] }
    }

    fn ui(&self) -> Option<Rc<Ui>> {
        self.ivars().ui.borrow().upgrade()
    }
}

/// The room a view reserves round its visible shape, if it is a pill.
fn bleed(view: &NSView) -> f64 {
    let object: &AnyObject = view;
    object
        .downcast_ref::<Pill>()
        .map_or(0.0, |pill| pill.ivars().inset)
}

/// One stretch of the scrolling column, in the order it is laid out.
enum Block {
    /// A section's title.
    Title(Retained<NSView>),
    /// Views that wrap onto the next line when the column is too narrow.
    Flow(Vec<Retained<NSView>>),
    /// Views that share one line, centred on it.
    Line(Vec<Retained<NSView>>),
}

/// What a row's clickable shape is: a text chip, or a larger tile with a ring.
#[derive(Clone, Copy)]
enum Face {
    Chip,
    Tile,
}

/// The settings tab: the model, and the views drawn from it.
struct Ui {
    pending: RefCell<Pending>,
    on_pick: Rc<dyn Fn(Pick)>,
    mtm: MainThreadMarker,
    actions: Retained<Actions>,
    fav: Retained<NSView>,
    settings: Retained<Pane>,
    segments: Retained<NSSegmentedControl>,
    canvas: Retained<Canvas>,
    scroll: Retained<NSScrollView>,
    doc: Retained<Doc>,
    /// Built once and kept through every rebuild, because its own click is what
    /// causes one and a control cannot be taken apart while it is answering.
    religious: Retained<NSButton>,
    apply: Retained<Pill>,
    note: Retained<NSTextField>,
    blocks: RefCell<Vec<Block>>,
}

impl Ui {
    fn build(
        mtm: MainThreadMarker,
        frame: NSRect,
        on_pick: Rc<dyn Fn(Pick)>,
        fav: Retained<NSView>,
    ) -> Rc<Self> {
        let actions = Actions::new(mtm);
        let target: &AnyObject = &actions;
        let fill = NSAutoresizingMaskOptions::ViewWidthSizable
            | NSAutoresizingMaskOptions::ViewHeightSizable;

        let labels = NSArray::from_retained_slice(&[
            NSString::from_str("Favourites"),
            NSString::from_str("Settings"),
        ]);
        let segments = unsafe {
            NSSegmentedControl::segmentedControlWithLabels_trackingMode_target_action(
                &labels,
                NSSegmentSwitchTracking::SelectOne,
                Some(target),
                Some(sel!(tabChanged:)),
                mtm,
            )
        };
        segments.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewMinXMargin
                | NSAutoresizingMaskOptions::ViewMaxXMargin
                | NSAutoresizingMaskOptions::ViewMinYMargin,
        );

        let size = frame.size;
        let settings = Pane::new(mtm, frame);
        settings.setAutoresizingMask(fill);
        // The tab is designed light only.
        if let Some(aqua) = unsafe { NSAppearance::appearanceNamed(NSAppearanceNameAqua) } {
            settings.setAppearance(Some(&aqua));
        }

        // The canvas and the scroll view are placed by `arrange`, which owns their
        // frames, so neither autoresizes.
        let canvas = Canvas::new(mtm);

        let scroll = NSScrollView::initWithFrame(
            NSScrollView::alloc(mtm),
            NSRect::new(
                NSPoint::new(OUTER * 2.0 + PREVIEW_MIN, BAR + 1.0),
                NSSize::new(
                    (size.width - OUTER * 2.0 - PREVIEW_MIN).max(1.0),
                    (size.height - BAR - 1.0).max(1.0),
                ),
            ),
        );
        scroll.setHasVerticalScroller(true);
        scroll.setAutohidesScrollers(true);
        scroll.setDrawsBackground(false);
        scroll.setBorderType(NSBorderType::NoBorder);
        let doc = Doc::new(
            mtm,
            NSRect::new(NSPoint::new(0.0, 0.0), scroll.contentSize()),
        );
        doc.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable);
        scroll.setDocumentView(Some(&doc));

        let religious = unsafe {
            NSButton::checkboxWithTitle_target_action(
                &NSString::from_str("Hide religious scenes"),
                Some(target),
                Some(sel!(religiousToggled:)),
                mtm,
            )
        };
        religious.sizeToFit();
        doc.addSubview(&religious);

        let hairline = Plate::new(
            mtm,
            NSRect::new(NSPoint::new(0.0, BAR), NSSize::new(size.width, 1.0)),
            0x000000,
            0.09,
        );
        hairline.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewMaxYMargin,
        );

        let apply = Pill::new(
            mtm,
            "Apply changes",
            14.0,
            Look::Primary,
            8.0,
            (None, 32.0),
            20.0,
            0.0,
            None,
        );
        let apply_w = apply.frame().size.width;
        apply.setFrameOrigin(NSPoint::new(
            size.width - OUTER - apply_w,
            (BAR - 32.0) / 2.0,
        ));
        apply.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewMinXMargin | NSAutoresizingMaskOptions::ViewMaxYMargin,
        );

        let note = text_label(mtm, "", 13.0, 0.0, MUTED);
        note.setAlignment(NSTextAlignment::Right);
        note.setFrame(NSRect::new(
            NSPoint::new(OUTER, (BAR - 16.0) / 2.0),
            NSSize::new((size.width - OUTER * 2.0 - apply_w - 16.0).max(1.0), 16.0),
        ));
        note.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewMaxYMargin,
        );

        settings.addSubview(&canvas);
        settings.addSubview(&scroll);
        settings.addSubview(&hairline);
        settings.addSubview(&apply);
        settings.addSubview(&note);
        settings.setHidden(true);

        let ui = Rc::new(Self {
            pending: RefCell::new(Pending::new(Default::default(), true, 16.0 / 10.0)),
            on_pick,
            mtm,
            actions,
            fav,
            settings,
            segments,
            canvas,
            scroll,
            doc,
            religious,
            apply,
            note,
            blocks: RefCell::new(Vec::new()),
        });
        *ui.actions.ivars().ui.borrow_mut() = Rc::downgrade(&ui);
        let weak = Rc::downgrade(&ui);
        *ui.doc.ivars().resized.borrow_mut() = Some(Rc::new(move || {
            if let Some(ui) = weak.upgrade() {
                ui.layout();
            }
        }));
        let weak = Rc::downgrade(&ui);
        *ui.settings.ivars().resized.borrow_mut() = Some(Rc::new(move || {
            if let Some(ui) = weak.upgrade() {
                ui.arrange();
            }
        }));
        ui.apply.ivars().on_click.replace(Some(ui.hook(|ui| {
            let staged = {
                let pending = ui.pending.borrow();
                if !pending.can_apply() {
                    return;
                }
                pending.staged().clone()
            };
            (ui.on_pick)(Pick::Apply(staged));
        })));
        ui.rebuild();
        ui
    }

    fn show_tab(&self, tab: Tab) {
        self.fav.setHidden(tab != Tab::Favourites);
        self.settings.setHidden(tab != Tab::Settings);
        self.segments
            .setSelectedSegment(if tab == Tab::Settings { 1 } else { 0 });
    }

    /// Takes what the loop has applied and the picture now on the desktop, and
    /// redraws.
    fn adopt(self: &Rc<Self>, snapshot: &Snapshot) {
        {
            let mut pending = self.pending.borrow_mut();
            pending.adopt(&snapshot.settings, snapshot.filters_apply, snapshot.aspect);
            pending.set_picture(snapshot.shown.as_ref().map(|art| art.path.as_path()));
        }
        self.rebuild();
    }

    /// A click handler that runs `act` on this tab if it is still there.
    fn hook(self: &Rc<Self>, act: impl Fn(&Rc<Ui>) + 'static) -> Rc<dyn Fn()> {
        let weak = Rc::downgrade(self);
        Rc::new(move || {
            if let Some(ui) = weak.upgrade() {
                act(&ui);
            }
        })
    }

    /// Changes the model, then rebuilds every row, since the chips may differ.
    fn change(self: &Rc<Self>, f: impl FnOnce(&mut Pending)) {
        f(&mut self.pending.borrow_mut());
        self.rebuild();
    }

    /// Changes the model and redraws only what shows the result, for controls
    /// that must survive their own change.
    fn change_light(&self, f: impl FnOnce(&mut Pending)) {
        f(&mut self.pending.borrow_mut());
        self.refresh();
    }

    /// The one constructor for everything clickable in a row. `selected` is all a
    /// caller says about looks; the pill decides what that means.
    fn chip(
        self: &Rc<Self>,
        face: Face,
        text: &str,
        selected: bool,
        enabled: bool,
        act: impl Fn(&Rc<Ui>) + 'static,
    ) -> Retained<NSView> {
        let pill = match face {
            Face::Chip => Pill::new(
                self.mtm,
                text,
                13.0,
                if selected { Look::Primary } else { Look::Plain },
                8.0,
                (None, CHIP_H),
                14.0,
                0.0,
                Some(self.hook(act)),
            ),
            Face::Tile => Pill::new(
                self.mtm,
                text,
                13.0,
                if selected { Look::Ringed } else { Look::Plain },
                10.0,
                (Some(96.0), 64.0),
                0.0,
                RING,
                Some(self.hook(act)),
            ),
        };
        pill.set_enabled(enabled, 0.5);
        as_view(pill)
    }

    fn title(&self, text: &str) -> Block {
        Block::Title(as_view(text_label(self.mtm, text, 13.0, SEMIBOLD, INK)))
    }

    /// Takes every row apart and builds it again from the model.
    fn rebuild(self: &Rc<Self>) {
        let blocks = self.blocks_from_model();

        let keep: &NSView = &self.religious;
        // Collected first: `subviews` may hand back the live array, and removing
        // from it mid-enumeration panics.
        let old: Vec<_> = self.doc.subviews().iter().collect();
        for view in old {
            if !std::ptr::eq(&*view, keep) {
                view.removeFromSuperview();
            }
        }
        for block in &blocks {
            match block {
                Block::Title(view) => self.doc.addSubview(view),
                Block::Flow(views) | Block::Line(views) => {
                    for view in views {
                        if !std::ptr::eq(&**view, keep) {
                            self.doc.addSubview(view);
                        }
                    }
                }
            }
        }
        *self.blocks.borrow_mut() = blocks;
        self.layout();
        self.refresh();
    }

    fn blocks_from_model(self: &Rc<Self>) -> Vec<Block> {
        let p = self.pending.borrow();
        let on = p.filters_apply();
        let mut blocks = vec![self.title("Style")];

        let cards = StyleKind::ALL
            .into_iter()
            .map(|kind| {
                self.chip(
                    Face::Tile,
                    kind.label(),
                    p.style_kind() == kind,
                    true,
                    move |ui| ui.change(|p| p.set_style(kind)),
                )
            })
            .collect();
        blocks.push(Block::Flow(cards));

        match p.style_kind() {
            StyleKind::Borders => {
                let border = p.border();
                blocks.push(Block::Flow(vec![
                    self.chip(Face::Chip, "Black", border == Border::Black, true, |ui| {
                        ui.change(|p| p.set_border(Border::Black))
                    }),
                    self.chip(
                        Face::Chip,
                        "Automatic",
                        border == Border::Auto,
                        true,
                        |ui| ui.change(|p| p.set_border(Border::Auto)),
                    ),
                    self.chip(
                        Face::Chip,
                        "Custom",
                        matches!(border, Border::Custom { .. }),
                        true,
                        |ui| {
                            ui.change(|p| {
                                let rgb = p.custom_colour();
                                p.set_border(Border::Custom { rgb })
                            })
                        },
                    ),
                ]));
                if matches!(border, Border::Custom { .. }) {
                    let [r, g, b] = p.custom_colour();
                    let well = NSColorWell::initWithFrame(
                        NSColorWell::alloc(self.mtm),
                        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(44.0, 28.0)),
                    );
                    well.setColor(&NSColor::colorWithSRGBRed_green_blue_alpha(
                        r as f64 / 255.0,
                        g as f64 / 255.0,
                        b as f64 / 255.0,
                        1.0,
                    ));
                    let target: &AnyObject = &self.actions;
                    unsafe {
                        well.setTarget(Some(target));
                        well.setAction(Some(sel!(colourPicked:)));
                    }
                    blocks.push(Block::Line(vec![as_view(well)]));
                }
            }
            StyleKind::Blur => {
                let (variant, strength) = p.blur();
                blocks.push(Block::Flow(vec![
                    self.chip(
                        Face::Chip,
                        "Behind the picture",
                        variant == BlurVariant::Backdrop,
                        true,
                        |ui| ui.change(|p| p.set_blur_variant(BlurVariant::Backdrop)),
                    ),
                    self.chip(
                        Face::Chip,
                        "Whole picture",
                        variant == BlurVariant::WholeImage,
                        true,
                        |ui| ui.change(|p| p.set_blur_variant(BlurVariant::WholeImage)),
                    ),
                ]));
                let target: &AnyObject = &self.actions;
                let slider = unsafe {
                    NSSlider::sliderWithValue_minValue_maxValue_target_action(
                        strength as f64,
                        0.0,
                        100.0,
                        Some(target),
                        Some(sel!(blurStrength:)),
                        self.mtm,
                    )
                };
                slider.setContinuous(false);
                slider.setFrame(NSRect::new(
                    NSPoint::new(0.0, 0.0),
                    NSSize::new(200.0, 20.0),
                ));
                blocks.push(Block::Line(vec![
                    as_view(text_label(self.mtm, "Strength", 13.0, 0.0, MUTED)),
                    as_view(slider),
                ]));
            }
            StyleKind::Zoom | StyleKind::Stretch => {}
        }

        blocks.push(self.title("Shape"));
        blocks.push(Block::Flow(
            p.shapes()
                .into_iter()
                .map(|c| {
                    let shape = c.value;
                    self.chip(Face::Chip, &c.label, c.selected, on, move |ui| {
                        ui.change(|p| p.set_shape(shape))
                    })
                })
                .collect(),
        ));

        blocks.push(self.title("Origin"));
        blocks.push(Block::Flow(
            p.regions()
                .into_iter()
                .map(|c| {
                    let region = c.value;
                    self.chip(Face::Chip, &c.label, c.selected, on, move |ui| {
                        ui.change(|p| p.toggle_region(region))
                    })
                })
                .collect(),
        ));

        blocks.push(self.title("Subject"));
        blocks.push(Block::Flow(
            p.subjects()
                .into_iter()
                .map(|c| {
                    let subject = c.value;
                    self.chip(Face::Chip, &c.label, c.selected, on, move |ui| {
                        ui.change(|p| p.toggle_subject(subject))
                    })
                })
                .collect(),
        ));

        let artists = p.artists();
        if !artists.is_empty() {
            blocks.push(self.title("Artist"));
            blocks.push(Block::Flow(
                artists
                    .into_iter()
                    .map(|c| {
                        let artist = c.value;
                        self.chip(Face::Chip, &c.label, c.selected, on, move |ui| {
                            ui.change(|p| p.toggle_artist(&artist))
                        })
                    })
                    .collect(),
            ));
        }

        self.religious.setState(p.hide_religious() as isize);
        self.religious.setEnabled(on);
        self.religious.setAlphaValue(if on { 1.0 } else { 0.5 });
        blocks.push(Block::Line(vec![as_view(self.religious.clone())]));
        blocks
    }

    /// Puts every view of the column where the current width says it goes.
    ///
    /// Only moves things, so it is safe to call while the column is being resized.
    fn layout(&self) {
        let width = self.doc.frame().size.width;
        let inner = (width - OUTER * 2.0).max(60.0);
        let mut y = 0.0;
        let mut after_title = false;
        for block in self.blocks.borrow().iter() {
            match block {
                Block::Title(view) => {
                    y += SECTION_GAP;
                    view.setFrameOrigin(NSPoint::new(OUTER, y));
                    y += view.frame().size.height + 10.0;
                    after_title = true;
                    continue;
                }
                Block::Flow(views) => {
                    if !after_title {
                        y += 12.0;
                    }
                    let (mut x, mut row) = (0.0, 0.0_f64);
                    for view in views {
                        // A tile's view is larger than the shape it shows, by the
                        // room for its ring; it is the shape that lines up.
                        let bleed = bleed(view);
                        let size = view.frame().size;
                        let (w, h) = (size.width - bleed * 2.0, size.height - bleed * 2.0);
                        if x > 0.0 && x + w > inner {
                            x = 0.0;
                            y += row + GAP;
                            row = 0.0;
                        }
                        view.setFrameOrigin(NSPoint::new(OUTER + x - bleed, y - bleed));
                        x += w + GAP;
                        row = row.max(h);
                    }
                    y += row;
                }
                Block::Line(views) => {
                    if !after_title {
                        y += 12.0;
                    }
                    let line = views
                        .iter()
                        .map(|v| v.frame().size.height)
                        .fold(0.0, f64::max);
                    let mut x = 0.0;
                    for view in views {
                        let size = view.frame().size;
                        view.setFrameOrigin(NSPoint::new(
                            OUTER + x,
                            y + ((line - size.height) / 2.0).floor(),
                        ));
                        x += size.width + 12.0;
                    }
                    y += line;
                }
            }
            after_title = false;
        }
        y += OUTER;
        // Never shorter than the window, or the clip view hangs a short column from
        // its bottom edge instead of its top.
        let height = y.max(self.scroll.contentSize().height);
        self.doc.setFrameSize(NSSize::new(width, height));
    }

    /// Places the preview at the top left and the column beside it, for the tab's
    /// current size. Only moves things, so it is safe to call while the window is
    /// being dragged: the picture is not made again, only drawn smaller.
    fn arrange(&self) {
        let size = self.settings.frame().size;
        let aspect = match self.pending.borrow().aspect() {
            a if a > 0.1 => a,
            _ => 16.0 / 10.0,
        };
        let wide = ((size.width - OUTER * 3.0) / 2.0).clamp(PREVIEW_MIN, PREVIEW_MAX);
        // A tall picture in a short window gives up width rather than reaching
        // down into the bar.
        let room = (size.height - BAR - 1.0 - OUTER - SHADOW_BELOW).max(1.0);
        let card_w = wide.min(room * aspect);
        self.canvas
            .place(size.height, NSSize::new(card_w, card_w / aspect));

        let left = OUTER * 2.0 + wide;
        self.scroll.setFrame(NSRect::new(
            NSPoint::new(left, BAR + 1.0),
            NSSize::new(
                (size.width - left).max(1.0),
                (size.height - BAR - 1.0).max(1.0),
            ),
        ));
    }

    /// Redraws what depends on the staged choices without touching the rows.
    fn refresh(&self) {
        let p = self.pending.borrow();
        // Made once at the largest the card can be, at twice the points for a
        // Retina display — see `RETINA` — and scaled down by the canvas.
        let image = p
            .preview((PREVIEW_MAX * RETINA) as u32)
            .and_then(|pixels| preview_image(&pixels));
        self.canvas.set_image(image);
        self.arrange();

        self.apply.set_enabled(p.can_apply(), 0.4);
        self.note
            .setStringValue(&NSString::from_str(p.note().unwrap_or("")));
    }
}
