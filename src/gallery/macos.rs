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

use super::{Control, Pick, Snapshot};
use crate::art::Artwork;
use crate::favourites::Favourites;
use anyhow::{anyhow, Result};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{
    define_class, msg_send, sel, AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly,
};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSBezierPath, NSBitmapImageRep, NSBorderType, NSButton,
    NSCalibratedRGBColorSpace, NSColor, NSCompositingOperation, NSEvent, NSFont, NSGraphicsContext,
    NSImage, NSImageScaling, NSImageView, NSScrollView, NSTextField, NSView,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString, NSURL};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;
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

/// The window's contents, which is the shelf: everything else either hangs off it
/// or is a subview it never has to be asked about again.
pub struct Content {
    shelf: Retained<Shelf>,
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
        let whole = NSView::initWithFrame(NSView::alloc(mtm), root.bounds());
        whole.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        let size = whole.bounds().size;

        let pane = NSView::initWithFrame(
            NSView::alloc(mtm),
            NSRect::new(
                NSPoint::new(SHELF, 0.0),
                NSSize::new((size.width - SHELF).max(1.0), size.height),
            ),
        );
        pane.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );

        let easel = Easel::build(mtm, &pane);
        let shelf = Shelf::new(mtm, easel, on_pick);

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
        root.addSubview(&whole);

        Ok(Self { shelf })
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

    pub fn describe(&self, _snapshot: &Snapshot, favourites: &Favourites) {
        self.relist(favourites);
    }

    pub fn describe_status(&self, _snapshot: &Snapshot) {}

    pub fn set_login(&self, _enabled: bool) {}
}
