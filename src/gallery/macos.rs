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
use crate::settings::Border;
use anyhow::{anyhow, Result};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{
    define_class, msg_send, sel, AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, Message,
};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSBezelStyle, NSBezierPath, NSBitmapImageRep, NSBorderType,
    NSButton, NSCalibratedRGBColorSpace, NSColor, NSColorSpace, NSColorWell,
    NSCompositingOperation, NSEvent, NSFont, NSGraphicsContext, NSImage, NSImageScaling,
    NSImageView, NSPopover, NSPopoverBehavior, NSScrollView, NSSegmentDistribution, NSSegmentStyle,
    NSSegmentSwitchTracking, NSSegmentedControl, NSSlider, NSTextAlignment, NSTextField, NSView,
    NSViewController, NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState,
    NSVisualEffectView, NSWindow, NSWindowStyleMask, NSWindowTitleVisibility,
};
use objc2_foundation::{NSArray, NSPoint, NSRect, NSRectEdge, NSSize, NSString, NSURL};
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
const SHELF: f64 = 184.0;
/// The side of the square each thumbnail is fitted inside.
const THUMB: f64 = 132.0;
/// One row of the column: a thumbnail and the air around it.
const CELL: f64 = THUMB + 20.0;
/// How many pixels a thumbnail is drawn per point. Two is as dense as any Mac
/// display goes, so a thumbnail made at two is never short of pixels; on a display
/// that wants one it is merely scaled down, which costs a quarter of a megabyte and
/// looks right.
const RETINA: f64 = 2.0;
const PAD: f64 = 16.0;
const BUTTON_H: f64 = 32.0;
const LINE: f64 = 18.0;
const TITLE_H: f64 = 22.0;
/// Everything under the picture — two lines and two buttons, and the air between
/// them — which is the height the picture does not get.
const FOOT: f64 = PAD + BUTTON_H + 14.0 + LINE + 2.0 + TITLE_H + PAD;

/// One picture on a shelf, as the window has it.
#[derive(Clone)]
struct Card {
    key: String,
    art: Artwork,
    /// Ticked on the shelf. Only the artist browser has anything to tick.
    chosen: bool,
    /// Why the primary button cannot do its work, if it cannot.
    blocked: Option<String>,
}

type Act = Rc<dyn Fn(&Card)>;
type Say = Rc<dyn Fn(&NSView, &str)>;

/// What makes a shelf and its easel a particular page: the words on them, and what
/// the two buttons and a double-click do. The pair itself knows nothing about
/// favourites or painters, which is how the same two views serve both.
struct Kit {
    /// The two lines shown instead of a picture when the shelf is empty.
    empty: (&'static str, &'static str),
    /// Says what the primary button does to a card that is (not) chosen.
    primary_title: fn(chosen: bool) -> &'static str,
    secondary_title: &'static str,
    secondary_role: ButtonRole,
    /// The widths of the primary and secondary buttons.
    widths: (f64, f64),
    on_primary: Act,
    on_secondary: Act,
    on_double: Option<Act>,
    /// Shows a blocked card's reason beside the primary button, which stays
    /// pressable for exactly that.
    explain: Option<Say>,
}

impl Kit {
    fn favourites(on_pick: Rc<dyn Fn(Pick)>) -> Self {
        let show = on_pick.clone();
        let double = on_pick.clone();
        Self {
            empty: (
                "Nothing kept yet",
                "Add to favourites keeps a painting here.",
            ),
            primary_title: |_| "Set as wallpaper",
            secondary_title: "Forget",
            secondary_role: ButtonRole::Destructive,
            widths: (152.0, 80.0),
            on_primary: Rc::new(move |card| show(Pick::Show(card.key.clone()))),
            on_secondary: Rc::new(move |card| on_pick(Pick::Forget(card.key.clone()))),
            on_double: Some(Rc::new(move |card| double(Pick::Show(card.key.clone())))),
            explain: None,
        }
    }
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
    kit: Kit,
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
                if let (Some(card), Some(act)) =
                    (self.selected_card(), self.ivars().kit.on_double.clone())
                {
                    act(&card);
                }
            }
        }

        #[unsafe(method(primaryPressed:))]
        fn primary_pressed(&self, _sender: Option<&AnyObject>) {
            let Some(card) = self.selected_card() else {
                return;
            };
            let kit = &self.ivars().kit;
            match (&card.blocked, &kit.explain) {
                (Some(reason), Some(explain)) => explain(&self.ivars().easel.show, reason),
                _ => (kit.on_primary)(&card),
            }
        }

        #[unsafe(method(secondaryPressed:))]
        fn secondary_pressed(&self, _sender: Option<&AnyObject>) {
            if let Some(card) = self.selected_card() {
                (self.ivars().kit.on_secondary)(&card);
            }
        }
    }
);

impl Shelf {
    fn new(mtm: MainThreadMarker, easel: Easel, kit: Kit) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars {
            shown: RefCell::new(Shown::default()),
            selected: Cell::new(None),
            easel,
            kit,
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
            easel.show.setAction(Some(sel!(primaryPressed:)));
            easel.forget.setTarget(Some(target));
            easel.forget.setAction(Some(sel!(secondaryPressed:)));
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
            let card = row.and_then(|row| shown.cards.get(row));
            let kit = &self.ivars().kit;
            self.ivars().easel.point_at(
                card,
                card.map(|card| (kit.primary_title)(card.chosen)),
                kit.secondary_title,
            );
        }
        self.setNeedsDisplay(true);
    }

    /// Picks the row with this key, if the list has it.
    fn select_key(&self, key: &str) {
        let row = self
            .ivars()
            .shown
            .borrow()
            .cards
            .iter()
            .position(|card| card.key == key);
        if let Some(row) = row {
            self.select(Some(row));
            // The column may have been built while hidden and left scrolled
            // anywhere; the chosen painter is always brought into view.
            let at = row as f64 * CELL;
            self.scrollRectToVisible(NSRect::new(NSPoint::new(0.0, at), NSSize::new(SHELF, CELL)));
        }
    }

    /// The key of the picture in the pane, if there is one.
    fn selected_key(&self) -> Option<String> {
        self.selected_card().map(|card| card.key)
    }

    /// The picture in the pane, copied out before anybody is told what was asked of
    /// it, because answering will take the list apart underneath us.
    fn selected_card(&self) -> Option<Card> {
        let row = self.ivars().selected.get()?;
        self.ivars().shown.borrow().cards.get(row).cloned()
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
                NSColor::controlAccentColor()
                    .colorWithAlphaComponent(0.12)
                    .setFill();
                NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(
                    shrink(cell, 8.0),
                    10.0,
                    10.0,
                )
                .fill();
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
            if chosen == Some(row) {
                let outline = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(
                    shrink(into, -3.0),
                    5.0,
                    5.0,
                );
                outline.setLineWidth(2.0);
                NSColor::controlAccentColor().setStroke();
                outline.stroke();
            }
            if card.chosen {
                tick(into);
            }
        }
    }
}

/// The mark on a chosen painter's thumbnail: a tick in an accent disc, hung on the
/// picture's top corner where it never covers the face of the painting.
fn tick(over: NSRect) {
    let d = 18.0;
    let disc = NSRect::new(
        NSPoint::new(
            over.origin.x + over.size.width - d + 6.0,
            over.origin.y - 6.0,
        ),
        NSSize::new(d, d),
    );
    // A ring in the window's own colour, so the disc reads against any painting.
    Tone::Solid.color().setFill();
    NSBezierPath::bezierPathWithOvalInRect(shrink(disc, -1.5)).fill();
    Tone::Accent.color().setFill();
    NSBezierPath::bezierPathWithOvalInRect(disc).fill();
    let mark = NSBezierPath::bezierPath();
    mark.moveToPoint(NSPoint::new(disc.origin.x + 5.0, disc.origin.y + 9.5));
    mark.lineToPoint(NSPoint::new(disc.origin.x + 8.0, disc.origin.y + 12.5));
    mark.lineToPoint(NSPoint::new(disc.origin.x + 13.5, disc.origin.y + 5.5));
    mark.setLineWidth(2.0);
    Tone::OnAccent.color().setStroke();
    mark.stroke();
}

/// The right-hand side: the picture being looked at, what it is called, and the two
/// things that can be done about it.
struct Easel {
    canvas: Retained<NSImageView>,
    title: Retained<NSTextField>,
    byline: Retained<NSTextField>,
    empty_title: Retained<NSTextField>,
    empty_note: Retained<NSTextField>,
    show: Retained<NSButton>,
    forget: Retained<NSButton>,
}

impl Easel {
    /// Builds the pane's contents into `pane`, leaving the buttons unaimed — see
    /// [`Shelf::take_the_buttons`].
    fn build(mtm: MainThreadMarker, pane: &NSView, kit: &Kit) -> Self {
        let size = pane.bounds().size;
        let wide = (size.width - PAD * 2.0).max(1.0);

        // The pane is not flipped, so all of this is measured up from its bottom
        // edge. Metadata and actions share one footer instead of forming two
        // unrelated rows beneath the painting.
        let ((show_w, forget_w), gap) = (kit.widths, 8.0);
        let show_x = size.width - PAD - show_w;
        let forget_x = show_x - gap - forget_w;
        let mid = PAD + BUTTON_H / 2.0;
        let show = action_button(
            mtm,
            (kit.primary_title)(false),
            (show_x, show_w),
            mid,
            ButtonRole::Primary,
        );
        let forget = action_button(
            mtm,
            kit.secondary_title,
            (forget_x, forget_w),
            mid,
            kit.secondary_role,
        );
        show.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewMinXMargin | NSAutoresizingMaskOptions::ViewMaxYMargin,
        );
        forget.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewMinXMargin | NSAutoresizingMaskOptions::ViewMaxYMargin,
        );
        let copy_w = (forget_x - PAD - 16.0).max(1.0);

        let byline = label(
            mtm,
            NSRect::new(
                NSPoint::new(PAD, PAD + BUTTON_H + 14.0),
                NSSize::new(copy_w, LINE),
            ),
            NSFont::systemFontOfSize(12.0),
            true,
        );
        let title = label(
            mtm,
            NSRect::new(
                NSPoint::new(PAD, PAD + BUTTON_H + 14.0 + LINE + 2.0),
                NSSize::new(copy_w, TITLE_H),
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

        let empty_title = label(
            mtm,
            NSRect::new(
                NSPoint::new(PAD, FOOT + (size.height - FOOT) / 2.0 + 4.0),
                NSSize::new(wide, TITLE_H),
            ),
            NSFont::systemFontOfSize_weight(15.0, SEMIBOLD),
            false,
        );
        empty_title.setAlignment(NSTextAlignment::Center);
        empty_title.setStringValue(&NSString::from_str(kit.empty.0));
        empty_title.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewMinYMargin
                | NSAutoresizingMaskOptions::ViewMaxYMargin,
        );
        let empty_note = label(
            mtm,
            NSRect::new(
                NSPoint::new(PAD, FOOT + (size.height - FOOT) / 2.0 - LINE),
                NSSize::new(wide, LINE),
            ),
            NSFont::systemFontOfSize(12.0),
            true,
        );
        empty_note.setAlignment(NSTextAlignment::Center);
        empty_note.setStringValue(&NSString::from_str(kit.empty.1));
        empty_note.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewMinYMargin
                | NSAutoresizingMaskOptions::ViewMaxYMargin,
        );

        pane.addSubview(&canvas);
        pane.addSubview(&empty_title);
        pane.addSubview(&empty_note);
        pane.addSubview(&title);
        pane.addSubview(&byline);
        pane.addSubview(&show);
        pane.addSubview(&forget);

        Self {
            canvas,
            title,
            byline,
            empty_title,
            empty_note,
            show,
            forget,
        }
    }

    /// Points the pane at a picture, or empties it when there is none.
    ///
    /// This is the one place a painting is held at its full size, and only ever one
    /// at a time: handing the view a new image is what lets go of the last.
    fn point_at(&self, card: Option<&Card>, primary: Option<&str>, secondary: &str) {
        match card.map(|card| (&card.art, card)) {
            Some((art, card)) => {
                self.show
                    .setTitle(&NSString::from_str(primary.unwrap_or_default()));
                self.forget.setTitle(&NSString::from_str(secondary));
                // Dimmed and not disabled: a disabled button cannot be pressed, and
                // pressing it is how the reason is asked for.
                self.show
                    .setAlphaValue(if card.blocked.is_some() { 0.45 } else { 1.0 });
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
                self.title.setHidden(false);
                self.byline.setHidden(false);
                self.empty_title.setHidden(true);
                self.empty_note.setHidden(true);
                self.show.setHidden(false);
                self.forget.setHidden(false);
                self.show.setEnabled(true);
                self.forget.setEnabled(true);
            }
            None => {
                self.canvas.setImage(None);
                self.title.setStringValue(&NSString::from_str(""));
                self.byline.setStringValue(&NSString::from_str(""));
                self.title.setHidden(true);
                self.byline.setHidden(true);
                self.empty_title.setHidden(false);
                self.empty_note.setHidden(false);
                self.show.setHidden(true);
                self.forget.setHidden(true);
                self.show.setEnabled(false);
                self.forget.setEnabled(false);
            }
        }
    }
}

#[derive(Clone, Copy)]
enum ButtonRole {
    Primary,
    Secondary,
    Destructive,
}

/// One action-button language for both tabs. Roles choose emphasis; AppKit keeps
/// native target/action and accessibility behavior.
///
/// `x`, `width` and `mid_y` describe the bezel that is seen, not the frame: AppKit
/// draws a push button some points inside its frame, and by a different amount for
/// each bezel style, so margins measured to the frame come out wider than the ones
/// beside them and two buttons sit further apart than was asked for.
fn action_button(
    mtm: MainThreadMarker,
    title: &str,
    (x, width): (f64, f64),
    mid_y: f64,
    role: ButtonRole,
) -> Retained<NSButton> {
    let button = unsafe {
        NSButton::buttonWithTitle_target_action(&NSString::from_str(title), None, None, mtm)
    };
    button.setBezelStyle(NSBezelStyle::Push);
    match role {
        ButtonRole::Primary => {
            button.setBezelColor(Some(&NSColor::controlAccentColor()));
            button.setContentTintColor(Some(&NSColor::whiteColor()));
        }
        ButtonRole::Secondary => {}
        ButtonRole::Destructive => {
            button.setBezelStyle(NSBezelStyle::AccessoryBarAction);
            button.setContentTintColor(Some(&NSColor::systemRedColor()));
        }
    }
    let height = button.intrinsicContentSize().height;
    button.setFrame(button.frameForAlignmentRect(NSRect::new(
        NSPoint::new(x, (mid_y - height / 2.0).round()),
        NSSize::new(width, height),
    )));
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

/// Fills `parent` with a column of thumbnails and the pane beside it. Returns the
/// shelf, which is the handle to the pair.
fn hang_shelf(mtm: MainThreadMarker, parent: &NSView, kit: Kit) -> Retained<Shelf> {
    let size = parent.bounds().size;
    let fill =
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable;

    let pane = NSView::initWithFrame(
        NSView::alloc(mtm),
        NSRect::new(
            NSPoint::new(SHELF, 0.0),
            NSSize::new((size.width - SHELF).max(1.0), size.height),
        ),
    );
    pane.setAutoresizingMask(fill);

    let easel = Easel::build(mtm, &pane, &kit);
    let shelf = Shelf::new(mtm, easel, kit);

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
        NSAutoresizingMaskOptions::ViewHeightSizable | NSAutoresizingMaskOptions::ViewMaxXMargin,
    );
    shelf.setFrameSize(NSSize::new(scroll.contentSize().width, 0.0));
    scroll.setDocumentView(Some(&shelf));

    shelf.take_the_buttons();

    let edge = Plate::new(
        mtm,
        NSRect::new(NSPoint::new(SHELF, 0.0), NSSize::new(1.0, size.height)),
        Tone::Line,
    );
    edge.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewHeightSizable | NSAutoresizingMaskOptions::ViewMaxXMargin,
    );
    parent.addSubview(&scroll);
    parent.addSubview(&pane);
    parent.addSubview(&edge);
    shelf
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
        on_control: Rc<dyn Fn(Control)>,
    ) -> Result<Self> {
        let mtm = MainThreadMarker::new()
            .ok_or_else(|| anyhow!("the favourites window must be built on the main thread"))?;

        // tao owns the window and this view; everything below is a subview of it and
        // goes when it goes.
        let root: &NSView = unsafe { &*(window.ns_view() as *const NSView) };

        // The content runs up under a see-through title bar, so the material below
        // is one surface from the top edge down. The tabs say what the window is,
        // which is why the title itself is hidden.
        let ns_window: &NSWindow = unsafe { &*(window.ns_window() as *const NSWindow) };
        ns_window.setStyleMask(ns_window.styleMask() | NSWindowStyleMask::FullSizeContentView);
        ns_window.setTitlebarAppearsTransparent(true);
        ns_window.setTitleVisibility(NSWindowTitleVisibility::Hidden);
        let title_bar = root.bounds().size.height - ns_window.contentLayoutRect().size.height;

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
            NSSize::new(total.width, (total.height - title_bar - STRIP).max(1.0)),
        );

        // Held active: this is an accessory's window and hardly ever the key one,
        // and a material that follows the window's state would sit there greyed.
        let material =
            NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), root.bounds());
        material.setMaterial(NSVisualEffectMaterial::UnderWindowBackground);
        material.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
        material.setState(NSVisualEffectState::Active);
        material.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        outer.addSubview(&material);
        let fill = NSAutoresizingMaskOptions::ViewWidthSizable
            | NSAutoresizingMaskOptions::ViewHeightSizable;

        // The favourites tab: unchanged, only shorter by the strip above it.
        let whole = NSView::initWithFrame(NSView::alloc(mtm), below);
        whole.setAutoresizingMask(fill);
        let shelf = hang_shelf(mtm, &whole, Kit::favourites(on_pick.clone()));

        // The settings tab, laid over the same area and hidden until asked for.
        let ui = Ui::build(mtm, below, on_pick, on_control, whole.clone());

        outer.addSubview(&whole);
        outer.addSubview(&ui.settings);
        outer.addSubview(&ui.segments);
        ui.segments.setFrame(NSRect::new(
            NSPoint::new(
                (total.width - TABS_W) / 2.0,
                total.height - title_bar - 8.0 - TABS_H,
            ),
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
                    chosen: false,
                    blocked: None,
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

    pub fn describe_status(&self, snapshot: &Snapshot) {
        self.ui.set_fetching(snapshot.fetching);
    }

    pub fn set_login(&self, _enabled: bool) {}
}

// ---------------------------------------------------------------------------
// The settings tab.
//
// Everything decided here is `Pending`'s; this half only draws it and forwards
// clicks into it. Choices are custom-drawn views (`Pill`) so selected and
// unavailable states share one quiet visual language. Section rows are rebuilt
// from the model after every click because their availability changes together.
// ---------------------------------------------------------------------------

/// The strip above both tabs that holds the switch between them.
const TABS_H: f64 = 28.0;
const TABS_W: f64 = 220.0;
const STRIP: f64 = TABS_H + 16.0;
/// The bar under the sections, which stays put while they scroll.
const BAR: f64 = 64.0;
const OUTER: f64 = 24.0;
/// The width of *Back*, which sits at the left of the bar while browsing.
const BACK_W: f64 = 72.0;
/// The preview takes about half the tab, within these widths. It is rendered once
/// at the larger, and drawn smaller when the window is.
const PREVIEW_MIN: f64 = 300.0;
const PREVIEW_MAX: f64 = 560.0;
/// Breathing room under the preview, matching its top and side margins.
const PREVIEW_BOTTOM: f64 = OUTER;
/// The line under the preview that says it can be framed. Always reserved, shown or
/// not, so the card does not jump when a style that cannot be framed is chosen.
const HINT_H: f64 = 20.0;
/// How far a scroll moves the zoom, per point of a trackpad's fine deltas and per
/// notch of a wheel's coarse ones. A wheel reports whole lines, so it needs the
/// larger step to feel like the same gesture.
const ZOOM_PER_POINT: f64 = 0.01;
const ZOOM_PER_NOTCH: f64 = 0.08;
/// The air on either side of the line between two sections.
const SECTION_GAP: f64 = OUTER;
/// Between one row and the next, and between a section's title and its first row.
const ROW_GAP: f64 = 12.0;
/// Between chips, across and down.
const GAP: f64 = 8.0;
/// The height and corner of a regular AppKit control, so a row of chips sits level
/// with the segmented control and the colour well in the rows around it.
const CHIP_H: f64 = 24.0;
const CHIP_RADIUS: f64 = 6.0;
const LABEL_W: f64 = 72.0;
const LABEL_GAP: f64 = 12.0;
/// How far below the top of its frame a section title's capitals begin. Taken off
/// wherever a title is measured against an edge, so it is the letters that line up
/// and not the box around them.
const CAP_INSET: f64 = 4.0;

const SEMIBOLD: f64 = 0.3;

/// The tab's colours, by role. Every one is a system colour, resolved when it is
/// drawn, so the tab follows light and dark and the user's accent colour, and the
/// translucent material behind it shows through instead of being painted over.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tone {
    Ink,
    Muted,
    Accent,
    OnAccent,
    /// A chip that is not chosen: a faint wash over the material, not a fill.
    Wash,
    Line,
    Placeholder,
    Solid,
}

impl Tone {
    fn color(self) -> Retained<NSColor> {
        match self {
            Tone::Ink => NSColor::labelColor(),
            Tone::Muted => NSColor::secondaryLabelColor(),
            Tone::Accent => NSColor::controlAccentColor(),
            Tone::OnAccent => NSColor::whiteColor(),
            Tone::Wash => NSColor::labelColor().colorWithAlphaComponent(0.06),
            Tone::Line => NSColor::separatorColor(),
            Tone::Placeholder => NSColor::quaternaryLabelColor(),
            Tone::Solid => NSColor::windowBackgroundColor(),
        }
    }
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
    tone: Tone,
) -> Retained<NSTextField> {
    let label = NSTextField::labelWithString(&NSString::from_str(text), mtm);
    label.setFont(Some(&NSFont::systemFontOfSize_weight(size, weight)));
    label.setTextColor(Some(&tone.color()));
    label.sizeToFit();
    label
}

/// How a pill is dressed. The palette is decided here and nowhere else: callers
/// say what a pill *is* (chosen, not chosen) and never what colour it is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Look {
    /// Filled with the accent colour. That is the whole of the difference: the
    /// words and their weight stay as they were, so choosing a chip never changes
    /// its width and nothing beside it moves.
    Selected,
    /// A faint wash with a hairline border: a chip that is not chosen.
    Plain,
}

type Explain = Rc<dyn Fn(&NSView)>;

struct PillIvars {
    look: Look,
    radius: f64,
    enabled: Cell<bool>,
    on_click: RefCell<Option<Rc<dyn Fn()>>>,
    on_unavailable: RefCell<Option<Explain>>,
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
                look, radius, ..
            } = *self.ivars();
            let bounds = self.bounds();
            let shape = bounds;
            match look {
                Look::Selected => Tone::Accent.color().setFill(),
                Look::Plain => Tone::Wash.color().setFill(),
            }
            NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(shape, radius, radius).fill();
            if look == Look::Selected {
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
            Tone::Line.color().setStroke();
            hairline.stroke();
        }

        /// The text inside is only a picture of a word: the click belongs to the
        /// pill, so it must not be swallowed by the label sitting on top of it. Only
        /// the shape answers.
        #[unsafe(method(hitTest:))]
        fn hit_test(&self, point: NSPoint) -> Option<&NSView> {
            // AppKit supplies this point in the superview's coordinates while it
            // walks the hierarchy, so compare it with the frame, not the local
            // bounds. Using bounds makes every control away from the origin miss.
            let frame = self.frame();
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
                let explain = self.ivars().on_unavailable.borrow().clone();
                if let Some(explain) = explain {
                    let this: &NSView = self;
                    explain(this);
                }
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
    /// A pill with `text` on it. A `width` of `None` fits the text plus `pad` on
    /// either side.
    #[allow(clippy::too_many_arguments)]
    fn new(
        mtm: MainThreadMarker,
        text: &str,
        size: f64,
        look: Look,
        radius: f64,
        (width, height): (Option<f64>, f64),
        pad: f64,
        on_click: Option<Rc<dyn Fn()>>,
    ) -> Retained<Self> {
        let ink = match look {
            Look::Selected => Tone::OnAccent,
            Look::Plain => Tone::Ink,
        };
        // Regular weight, as the native controls in the rows around it are.
        let label = text_label(mtm, text, size, 0.0, ink);
        let line = label.frame().size;
        let width = width.unwrap_or(line.width.ceil() + pad * 2.0);
        let this = Self::alloc(mtm).set_ivars(PillIvars {
            look,
            radius,
            enabled: Cell::new(true),
            on_click: RefCell::new(on_click),
            on_unavailable: RefCell::new(None),
        });
        let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, height));
        let pill: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
        label.setAlignment(NSTextAlignment::Center);
        label.setFrame(NSRect::new(
            NSPoint::new(0.0, ((height - line.height) / 2.0).floor()),
            NSSize::new(width, line.height),
        ));
        pill.addSubview(&label);
        pill
    }

    /// An unavailable choice remains a hit target so it can explain itself.
    fn set_enabled(&self, on: bool, dim: f64) {
        self.ivars().enabled.set(on);
        self.setAlphaValue(if on { 1.0 } else { dim });
    }
}

struct PlateIvars {
    fill: Tone,
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
            self.ivars().fill.color().setFill();
            NSBezierPath::fillRect(self.bounds());
        }
    }
);

impl Plate {
    fn new(mtm: MainThreadMarker, frame: NSRect, fill: Tone) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(PlateIvars { fill });
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }
}

/// The settings tab's own surface: see-through to the window's material, and it says when its size changes so
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

/// What the person did to the preview, in points: `dx`, `dy` and the zoom's centre
/// are measured from the card's top left with y growing downwards, and `width` is
/// the card's drawn width, which is the scale the model needs to turn them into a
/// framing.
#[derive(Clone, Copy)]
enum Gesture {
    Drag {
        dx: f64,
        dy: f64,
        width: f64,
    },
    Zoom {
        factor: f64,
        x: f64,
        y: f64,
        width: f64,
    },
}

type Report = Rc<dyn Fn(Gesture)>;

struct CanvasIvars {
    image: RefCell<Option<Retained<NSImage>>>,
    /// How wide the card is drawn; the picture is scaled down to it.
    card_w: Cell<f64>,
    none_yet: Retained<NSTextField>,
    hint: Retained<NSTextField>,
    /// Whether the model can frame the picture at all. When it cannot, a scroll over
    /// the preview belongs to whatever it scrolled before.
    framing: Cell<bool>,
    /// Where the pointer was at the last drag event, while a drag that began on the
    /// card is under way. The view has no handle to the tab, so it reports through
    /// this hook like `Pane` does.
    dragging: Cell<Option<NSPoint>>,
    on_gesture: RefCell<Option<Report>>,
}

define_class!(
    // SAFETY: as for `Pill`.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[ivars = CanvasIvars]
    struct Canvas;

    impl Canvas {
        /// A drag starts on the first click, as on the shelf: the window is hardly
        /// ever the active one.
        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            let at = self.convertPoint_fromView(event.locationInWindow(), None);
            let on_card = self.over_card(at);
            self.ivars().dragging.set(on_card.then_some(at));
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            let Some(last) = self.ivars().dragging.get() else {
                return;
            };
            let at = self.convertPoint_fromView(event.locationInWindow(), None);
            self.ivars().dragging.set(Some(at));
            // The view counts y upwards and the model downwards.
            self.report(Gesture::Drag {
                dx: at.x - last.x,
                dy: last.y - at.y,
                width: self.ivars().card_w.get(),
            });
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, _event: &NSEvent) {
            self.ivars().dragging.set(None);
        }

        #[unsafe(method(scrollWheel:))]
        fn scroll_wheel(&self, event: &NSEvent) {
            let at = self.convertPoint_fromView(event.locationInWindow(), None);
            if !self.ivars().framing.get() || !self.over_card(at) {
                // Not ours: the column or the window scrolls as it always did.
                let _: () = unsafe { msg_send![super(self), scrollWheel: event] };
                return;
            }
            let step = if event.hasPreciseScrollingDeltas() {
                ZOOM_PER_POINT
            } else {
                ZOOM_PER_NOTCH
            };
            self.zoom(at, (event.scrollingDeltaY() * step).exp());
        }

        #[unsafe(method(magnifyWithEvent:))]
        fn magnify(&self, event: &NSEvent) {
            let at = self.convertPoint_fromView(event.locationInWindow(), None);
            if !self.ivars().framing.get() || !self.over_card(at) {
                let _: () = unsafe { msg_send![super(self), magnifyWithEvent: event] };
                return;
            }
            self.zoom(at, 1.0 + event.magnification());
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            let card = self.card();
            if card.size.height < 1.0 {
                return;
            }
            let path = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(card, 10.0, 10.0);

            Tone::Solid.color().setFill();
            path.fill();
            let edge = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(
                shrink(card, 0.5),
                9.5,
                9.5,
            );
            edge.setLineWidth(1.0);
            Tone::Line.color().setStroke();
            edge.stroke();

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
                    Tone::Placeholder.color().setFill();
                    path.fill();
                }
            }
        }
    }
);

impl Canvas {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let none_yet = text_label(mtm, "No picture yet", 13.0, 0.0, Tone::Muted);
        none_yet.setAlignment(NSTextAlignment::Center);
        let hint = text_label(mtm, "", 12.0, 0.0, Tone::Muted);
        hint.setHidden(true);
        let this = Self::alloc(mtm).set_ivars(CanvasIvars {
            image: RefCell::new(None),
            card_w: Cell::new(PREVIEW_MIN),
            none_yet,
            hint,
            framing: Cell::new(false),
            dragging: Cell::new(None),
            on_gesture: RefCell::new(None),
        });
        let canvas: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: NSRect::ZERO] };
        canvas.addSubview(&canvas.ivars().none_yet);
        canvas.addSubview(&canvas.ivars().hint);
        canvas
    }

    fn over_card(&self, at: NSPoint) -> bool {
        let card = self.card();
        at.x >= card.origin.x
            && at.x <= card.origin.x + card.size.width
            && at.y >= card.origin.y
            && at.y <= card.origin.y + card.size.height
    }

    fn report(&self, gesture: Gesture) {
        let hook = self.ivars().on_gesture.borrow().clone();
        if let Some(hook) = hook {
            hook(gesture);
        }
    }

    /// Zooms about `at`, a point of this view, which the model wants from the
    /// card's top left.
    fn zoom(&self, at: NSPoint, factor: f64) {
        let card = self.card();
        self.report(Gesture::Zoom {
            factor,
            x: at.x - card.origin.x,
            y: card.origin.y + card.size.height - at.y,
            width: card.size.width,
        });
    }

    /// Says whether the picture can be framed, and the line that tells the person so.
    fn set_framing(&self, can_frame: bool, hint: Option<&str>) {
        self.ivars().framing.set(can_frame);
        let hint_view = &self.ivars().hint;
        hint_view.setHidden(hint.is_none());
        if let Some(hint) = hint {
            hint_view.setStringValue(&NSString::from_str(hint));
        }
    }

    /// Where the card itself sits in the view.
    fn card(&self) -> NSRect {
        let height = self.bounds().size.height;
        NSRect::new(
            NSPoint::new(OUTER, PREVIEW_BOTTOM + HINT_H),
            NSSize::new(
                self.ivars().card_w.get(),
                (height - OUTER - PREVIEW_BOTTOM - HINT_H).max(0.0),
            ),
        )
    }

    /// Sizes the view to a card of `card` size, hung from the top of `above`.
    fn place(&self, above: f64, card: NSSize) {
        let total = OUTER + card.height + HINT_H + PREVIEW_BOTTOM;
        self.ivars().card_w.set(card.width);
        self.setFrame(NSRect::new(
            NSPoint::new(0.0, above - total),
            NSSize::new(OUTER + card.width, total),
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
        self.ivars().hint.setFrame(NSRect::new(
            NSPoint::new(card.origin.x, PREVIEW_BOTTOM),
            NSSize::new(card.size.width, 16.0),
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

        #[unsafe(method(placementChanged:))]
        fn placement_changed(&self, sender: Option<&AnyObject>) {
            let Some(control) = sender.and_then(|s| s.downcast_ref::<NSSegmentedControl>()) else {
                return;
            };
            let selected = control.selectedSegment();
            let Some(kind) = usize::try_from(selected)
                .ok()
                .and_then(|index| StyleKind::ALL.get(index))
                .copied()
            else {
                return;
            };
            if let Some(ui) = self.ui() {
                ui.change(|pending| pending.set_style(kind));
            }
        }

        #[unsafe(method(applyPressed:))]
        fn apply_pressed(&self, _sender: Option<&AnyObject>) {
            if let Some(ui) = self.ui() {
                ui.apply_staged();
            }
        }

        #[unsafe(method(backPressed:))]
        fn back_pressed(&self, _sender: Option<&AnyObject>) {
            if let Some(ui) = self.ui() {
                ui.leave_browser();
            }
        }

        #[unsafe(method(nextPressed:))]
        fn next_pressed(&self, _sender: Option<&AnyObject>) {
            if let Some(ui) = self.ui() {
                ui.ask_for_next();
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

/// One stretch of the scrolling column, in the order it is laid out.
enum Block {
    Section(Retained<NSView>),
    Divider(Retained<NSView>),
    Row {
        label: Retained<NSView>,
        views: Vec<Retained<NSView>>,
        wraps: bool,
    },
}

/// The settings tab: the model, and the views drawn from it.
struct Ui {
    pending: RefCell<Pending>,
    on_pick: Rc<dyn Fn(Pick)>,
    on_control: Rc<dyn Fn(Control)>,
    mtm: MainThreadMarker,
    actions: Retained<Actions>,
    fav: Retained<NSView>,
    settings: Retained<Pane>,
    segments: Retained<NSSegmentedControl>,
    canvas: Retained<Canvas>,
    scroll: Retained<NSScrollView>,
    /// The artist browser, laid over the rows and the preview when open. It looks
    /// like the favourites tab on purpose: the same pair of views, other words.
    browser: Retained<NSView>,
    artists: Retained<Shelf>,
    back: Retained<NSButton>,
    /// Whether the browser has been opened yet, which is when its thumbnails are
    /// first made — not when the window opens.
    listed: Cell<bool>,
    doc: Retained<Doc>,
    /// Built once and kept through every rebuild, because its own click is what
    /// causes one and a control cannot be taken apart while it is answering.
    religious: Retained<NSButton>,
    placement: Retained<NSSegmentedControl>,
    next: Retained<NSButton>,
    apply: Retained<NSButton>,
    note: Retained<NSTextField>,
    popover: RefCell<Option<Retained<NSPopover>>>,
    blocks: RefCell<Vec<Block>>,
}

impl Ui {
    fn build(
        mtm: MainThreadMarker,
        frame: NSRect,
        on_pick: Rc<dyn Fn(Pick)>,
        on_control: Rc<dyn Fn(Control)>,
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

        // The canvas and the scroll view are placed by `arrange`, which owns their
        // frames, so neither autoresizes.
        let canvas = Canvas::new(mtm);

        let scroll = NSScrollView::initWithFrame(
            NSScrollView::alloc(mtm),
            NSRect::new(
                NSPoint::new(OUTER + PREVIEW_MIN, BAR + 1.0),
                NSSize::new(
                    (size.width - OUTER - PREVIEW_MIN).max(1.0),
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

        let placement_labels: Vec<_> = StyleKind::ALL
            .iter()
            .map(|kind| NSString::from_str(kind.label()))
            .collect();
        let placement_labels = NSArray::from_retained_slice(&placement_labels);
        let placement = unsafe {
            NSSegmentedControl::segmentedControlWithLabels_trackingMode_target_action(
                &placement_labels,
                NSSegmentSwitchTracking::SelectOne,
                Some(target),
                Some(sel!(placementChanged:)),
                mtm,
            )
        };
        placement.setSegmentStyle(NSSegmentStyle::Rounded);
        placement.setSegmentDistribution(NSSegmentDistribution::FillEqually);
        // As tall as it draws and no taller: a frame with air in it is a row that
        // sits further from its neighbours than the others do.
        placement.sizeToFit();
        placement.setFrameSize(NSSize::new(280.0, placement.frame().size.height));
        doc.addSubview(&placement);

        let hairline = Plate::new(
            mtm,
            NSRect::new(NSPoint::new(0.0, BAR), NSSize::new(size.width, 1.0)),
            Tone::Line,
        );
        hairline.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewMaxYMargin,
        );

        let action_w = 120.0;
        let action_gap = 8.0;
        let apply_x = size.width - OUTER - action_w;
        let next_x = apply_x - action_gap - action_w;

        let next = action_button(
            mtm,
            "Next picture",
            (next_x, action_w),
            BAR / 2.0,
            ButtonRole::Secondary,
        );
        unsafe {
            next.setTarget(Some(target));
            next.setAction(Some(sel!(nextPressed:)));
        }
        next.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewMinXMargin | NSAutoresizingMaskOptions::ViewMaxYMargin,
        );

        let apply = action_button(
            mtm,
            "Apply changes",
            (apply_x, action_w),
            BAR / 2.0,
            ButtonRole::Primary,
        );
        unsafe {
            apply.setTarget(Some(target));
            apply.setAction(Some(sel!(applyPressed:)));
        }
        apply.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewMinXMargin | NSAutoresizingMaskOptions::ViewMaxYMargin,
        );

        let note = text_label(mtm, "", 13.0, 0.0, Tone::Muted);
        note.setAlignment(NSTextAlignment::Right);
        note.setFrame(NSRect::new(
            NSPoint::new(OUTER, (BAR - 16.0) / 2.0),
            NSSize::new((next_x - OUTER - 16.0).max(1.0), 16.0),
        ));
        note.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewMaxYMargin,
        );

        // The browser fills what the bar leaves, and the bar stays under it.
        let browser = NSView::initWithFrame(
            NSView::alloc(mtm),
            NSRect::new(
                NSPoint::new(0.0, BAR + 1.0),
                NSSize::new(size.width, (size.height - BAR - 1.0).max(1.0)),
            ),
        );
        browser.setAutoresizingMask(fill);
        browser.setHidden(true);

        // What the artist shelf's buttons reach is the tab, which does not exist
        // until the shelf does; the slot is filled the moment it does.
        let slot: Rc<RefCell<Weak<Ui>>> = Rc::default();
        let reach = |act: fn(&Rc<Ui>, &Card)| -> Act {
            let slot = slot.clone();
            Rc::new(move |card| {
                let ui = slot.borrow().upgrade();
                if let Some(ui) = ui {
                    act(&ui, card);
                }
            })
        };
        let explain = {
            let slot = slot.clone();
            Rc::new(move |anchor: &NSView, reason: &str| {
                let ui = slot.borrow().upgrade();
                if let Some(ui) = ui {
                    ui.show_reason(anchor, reason);
                }
            })
        };
        let kit = Kit {
            empty: (
                "No painters yet",
                "The catalogue names none to choose from.",
            ),
            primary_title: |chosen| if chosen { "Remove" } else { "Choose" },
            secondary_title: "Read more",
            secondary_role: ButtonRole::Secondary,
            widths: (96.0, 96.0),
            on_primary: reach(|ui, card| ui.change(|p| p.toggle_artist(&card.key))),
            on_secondary: reach(|ui, card| {
                if let Some(url) = &card.art.details_url {
                    (ui.on_pick)(Pick::Read(url.clone()));
                }
            }),
            on_double: None,
            explain: Some(explain),
        };
        let artists = hang_shelf(mtm, &browser, kit);
        // In the bar and not on the shelf, so the browser's shelf starts exactly
        // where the favourites' does. Shown only while browsing.
        let back = action_button(
            mtm,
            "Back",
            (OUTER, BACK_W),
            BAR / 2.0,
            ButtonRole::Secondary,
        );
        unsafe {
            back.setTarget(Some(target));
            back.setAction(Some(sel!(backPressed:)));
        }
        back.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewMaxXMargin | NSAutoresizingMaskOptions::ViewMaxYMargin,
        );
        back.setHidden(true);

        settings.addSubview(&canvas);
        settings.addSubview(&scroll);
        settings.addSubview(&browser);
        settings.addSubview(&hairline);
        settings.addSubview(&back);
        settings.addSubview(&next);
        settings.addSubview(&apply);
        settings.addSubview(&note);
        settings.setHidden(true);

        let ui = Rc::new(Self {
            pending: RefCell::new(Pending::new(Default::default(), true, 16.0 / 10.0)),
            on_pick,
            on_control,
            mtm,
            actions,
            fav,
            settings,
            segments,
            canvas,
            scroll,
            browser,
            artists,
            back,
            listed: Cell::new(false),
            doc,
            religious,
            placement,
            next,
            apply,
            note,
            popover: RefCell::new(None),
            blocks: RefCell::new(Vec::new()),
        });
        *ui.actions.ivars().ui.borrow_mut() = Rc::downgrade(&ui);
        *slot.borrow_mut() = Rc::downgrade(&ui);
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
        let weak = Rc::downgrade(&ui);
        *ui.canvas.ivars().on_gesture.borrow_mut() = Some(Rc::new(move |gesture| {
            if let Some(ui) = weak.upgrade() {
                ui.change_light(|p| match gesture {
                    Gesture::Drag { dx, dy, width } => p.drag(dx, dy, width),
                    Gesture::Zoom {
                        factor,
                        x,
                        y,
                        width,
                    } => p.zoom_about(factor, x, y, width),
                });
            }
        }));
        ui.rebuild();
        ui
    }

    /// Hands the staged settings to the loop, if there is anything to apply.
    fn apply_staged(&self) {
        let staged = {
            let pending = self.pending.borrow();
            if !pending.can_apply() {
                return;
            }
            pending.staged().clone()
        };
        (self.on_pick)(Pick::Apply(staged));
    }

    fn ask_for_next(&self) {
        (self.on_control)(Control::Next);
    }

    fn set_fetching(&self, fetching: bool) {
        self.next.setEnabled(!fetching);
    }

    fn show_tab(&self, tab: Tab) {
        if let Some(popover) = self.popover.borrow_mut().take() {
            popover.close();
        }
        // Whatever tab it was left on, Settings opens on its rows.
        self.leave_browser();
        self.fav.setHidden(tab != Tab::Favourites);
        self.settings.setHidden(tab != Tab::Settings);
        self.segments
            .setSelectedSegment(if tab == Tab::Settings { 1 } else { 0 });
    }

    /// Swaps the rows and the preview for the artist browser, on the first chosen
    /// painter or else the first of them.
    fn open_browser(&self) {
        let cards = self.artist_cards();
        let first = cards
            .iter()
            .find(|card| card.chosen)
            .or(cards.first())
            .map(|card| card.key.clone());
        self.artists.adopt(cards);
        if let Some(key) = first {
            self.artists.select_key(&key);
        }
        self.listed.set(true);
        self.canvas.setHidden(true);
        self.scroll.setHidden(true);
        self.browser.setHidden(false);
        self.show_back(true);
    }

    /// *Back* takes the left of the bar while browsing, and the note gives way to
    /// it so that a long one cannot run underneath.
    fn show_back(&self, on: bool) {
        self.back.setHidden(!on);
        let frame = self.note.frame();
        let right = frame.origin.x + frame.size.width;
        let left = if on { OUTER + BACK_W + 16.0 } else { OUTER };
        self.note.setFrame(NSRect::new(
            NSPoint::new(left, frame.origin.y),
            NSSize::new((right - left).max(1.0), frame.size.height),
        ));
    }

    fn leave_browser(&self) {
        if let Some(popover) = self.popover.borrow_mut().take() {
            popover.close();
        }
        self.browser.setHidden(true);
        self.show_back(false);
        self.canvas.setHidden(false);
        self.scroll.setHidden(false);
    }

    /// The painters as the shelf has them. Thumbnails already made are kept by
    /// `adopt`, so asking again after every choice costs no decoding.
    fn artist_cards(&self) -> Vec<Card> {
        self.pending
            .borrow()
            .artist_cards()
            .into_iter()
            .map(|card| Card {
                key: card.name,
                art: card.art,
                chosen: card.selected,
                blocked: card.disabled_reason,
            })
            .collect()
    }

    /// Takes what the loop has applied and the picture now on the desktop, and
    /// redraws.
    fn adopt(self: &Rc<Self>, snapshot: &Snapshot) {
        {
            let mut pending = self.pending.borrow_mut();
            pending.unpack_artists_into(&snapshot.artist_pictures);
            pending.adopt(&snapshot.settings, snapshot.filters_apply, snapshot.aspect);
            pending.set_picture(snapshot.shown.as_ref().map(|art| art.path.as_path()));
        }
        self.set_fetching(snapshot.fetching);
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
        text: &str,
        selected: bool,
        disabled_reason: Option<String>,
        act: impl Fn(&Rc<Ui>) + 'static,
    ) -> Retained<NSView> {
        let pill = Pill::new(
            self.mtm,
            text,
            13.0,
            if selected {
                Look::Selected
            } else {
                Look::Plain
            },
            CHIP_RADIUS,
            (None, CHIP_H),
            12.0,
            Some(self.hook(act)),
        );
        if let Some(reason) = disabled_reason {
            pill.set_enabled(false, if selected { 0.62 } else { 0.32 });
            let weak = Rc::downgrade(self);
            pill.ivars()
                .on_unavailable
                .replace(Some(Rc::new(move |anchor| {
                    if let Some(ui) = weak.upgrade() {
                        ui.show_reason(anchor, &reason);
                    }
                })));
        }
        as_view(pill)
    }

    fn section(&self, text: &str) -> Block {
        Block::Section(as_view(text_label(
            self.mtm,
            text,
            15.0,
            SEMIBOLD,
            Tone::Ink,
        )))
    }

    /// A row's label starts on the same left edge as the section title above it.
    fn row(&self, label: &str, views: Vec<Retained<NSView>>, wraps: bool) -> Block {
        let label = text_label(self.mtm, label, 13.0, 0.0, Tone::Muted);
        Block::Row {
            label: as_view(label),
            views,
            wraps,
        }
    }

    fn divider(&self) -> Block {
        Block::Divider(as_view(Plate::new(
            self.mtm,
            NSRect::new(NSPoint::ZERO, NSSize::new(1.0, 1.0)),
            Tone::Line,
        )))
    }

    fn show_reason(&self, anchor: &NSView, text: &str) {
        if let Some(previous) = self.popover.borrow_mut().take() {
            previous.close();
        }
        let (wide, pad) = (280.0, 12.0);
        let label = NSTextField::wrappingLabelWithString(&NSString::from_str(text), self.mtm);
        label.setFont(Some(&NSFont::systemFontOfSize(13.0)));
        label.setTextColor(Some(&NSColor::labelColor()));
        // As tall as the sentence turns out to be at this width, so that no reason
        // is ever cut off at the popover's edge.
        let lines = NSSize::new(wide - pad * 2.0, 10_000.0);
        let text_h = label.sizeThatFits(lines).height.ceil();
        let size = NSSize::new(wide, text_h + pad * 2.0);
        label.setFrame(NSRect::new(
            NSPoint::new(pad, pad),
            NSSize::new(lines.width, text_h),
        ));
        let content =
            NSView::initWithFrame(NSView::alloc(self.mtm), NSRect::new(NSPoint::ZERO, size));
        content.addSubview(&label);
        let controller = NSViewController::new(self.mtm);
        controller.setView(&content);
        let popover = NSPopover::init(NSPopover::alloc(self.mtm));
        popover.setBehavior(NSPopoverBehavior::Transient);
        popover.setAnimates(true);
        popover.setContentSize(size);
        popover.setContentViewController(Some(&controller));
        popover.showRelativeToRect_ofView_preferredEdge(anchor.bounds(), anchor, NSRectEdge::MaxY);
        self.popover.replace(Some(popover));
    }

    /// Takes every row apart and builds it again from the model.
    fn rebuild(self: &Rc<Self>) {
        let blocks = self.blocks_from_model();

        let religious: &NSView = &self.religious;
        let placement: &NSView = &self.placement;
        // Collected first: `subviews` may hand back the live array, and removing
        // from it mid-enumeration panics.
        let old: Vec<_> = self.doc.subviews().iter().collect();
        for view in old {
            if !std::ptr::eq(&*view, religious) && !std::ptr::eq(&*view, placement) {
                view.removeFromSuperview();
            }
        }
        for block in &blocks {
            match block {
                Block::Section(view) | Block::Divider(view) => self.doc.addSubview(view),
                Block::Row { label, views, .. } => {
                    self.doc.addSubview(label);
                    for view in views {
                        if !std::ptr::eq(&**view, religious) && !std::ptr::eq(&**view, placement) {
                            self.doc.addSubview(view);
                        }
                    }
                }
            }
        }
        *self.blocks.borrow_mut() = blocks;
        self.layout();
        self.refresh();
        // The shelf follows the staged choices too, once there is one to follow.
        if self.listed.get() {
            self.artists.adopt(self.artist_cards());
        }
    }

    fn blocks_from_model(self: &Rc<Self>) -> Vec<Block> {
        let p = self.pending.borrow();
        let on = p.filters_apply();
        let mut blocks = vec![self.section("Wallpaper")];

        let selected = StyleKind::ALL
            .iter()
            .position(|kind| *kind == p.style_kind())
            .unwrap_or_default();
        self.placement.setSelectedSegment(selected as isize);
        blocks.push(self.row("Placement", vec![as_view(self.placement.clone())], false));

        match p.style_kind() {
            StyleKind::Borders => {
                let border = p.border();
                blocks.push(
                    self.row(
                        "Borders",
                        p.border_chips()
                            .into_iter()
                            .map(|c| {
                                let border = c.value;
                                self.chip(&c.label, c.selected, None, move |ui| {
                                    ui.change(|p| p.set_border(border))
                                })
                            })
                            .collect(),
                        true,
                    ),
                );
                if matches!(border, Border::Custom { .. }) {
                    let [r, g, b] = p.custom_colour();
                    let well = NSColorWell::initWithFrame(
                        NSColorWell::alloc(self.mtm),
                        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(44.0, CHIP_H)),
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
                    blocks.push(self.row("Colour", vec![as_view(well)], false));
                }
            }
            StyleKind::Blur => {
                let (_, strength) = p.blur();
                blocks.push(
                    self.row(
                        "Mode",
                        p.blur_chips()
                            .into_iter()
                            .map(|c| {
                                let variant = c.value;
                                self.chip(&c.label, c.selected, None, move |ui| {
                                    ui.change(|p| p.set_blur_variant(variant))
                                })
                            })
                            .collect(),
                        true,
                    ),
                );
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
                blocks.push(self.row("Strength", vec![as_view(slider)], false));
            }
            StyleKind::Zoom | StyleKind::Stretch => {}
        }

        blocks.push(self.divider());
        blocks.push(self.section("Painting filters"));
        blocks.push(
            self.row(
                "Shape",
                p.shapes()
                    .into_iter()
                    .map(|c| {
                        let shape = c.value;
                        self.chip(&c.label, c.selected, c.disabled_reason, move |ui| {
                            ui.change(|p| p.set_shape(shape))
                        })
                    })
                    .collect(),
                true,
            ),
        );

        blocks.push(
            self.row(
                "Origin",
                p.regions()
                    .into_iter()
                    .map(|c| {
                        let region = c.value;
                        self.chip(&c.label, c.selected, c.disabled_reason, move |ui| {
                            ui.change(|p| p.toggle_region(region))
                        })
                    })
                    .collect(),
                true,
            ),
        );

        blocks.push(
            self.row(
                "Subject",
                p.subjects()
                    .into_iter()
                    .map(|c| {
                        let subject = c.value;
                        self.chip(&c.label, c.selected, c.disabled_reason, move |ui| {
                            ui.change(|p| p.toggle_subject(subject))
                        })
                    })
                    .collect(),
                true,
            ),
        );

        // Only those already chosen get a chip of their own; choosing among the
        // rest is the browser's work.
        let row = p.artist_row();
        let mut views: Vec<_> = row
            .chosen
            .into_iter()
            .map(|c| {
                let artist = c.value;
                self.chip(&c.label, c.selected, c.disabled_reason, move |ui| {
                    ui.change(|p| p.toggle_artist(&artist))
                })
            })
            .collect();
        views.push(self.chip(row.browse, false, row.disabled_reason, |ui| {
            ui.open_browser()
        }));
        blocks.push(self.row("Artist", views, true));

        self.religious.setState(p.hide_religious() as isize);
        self.religious.setEnabled(on);
        self.religious.setAlphaValue(if on { 1.0 } else { 0.5 });
        blocks.push(self.row("Content", vec![as_view(self.religious.clone())], false));
        blocks
    }

    /// Puts every view of the column where the current width says it goes.
    ///
    /// Only moves things, so it is safe to call while the column is being resized.
    fn layout(&self) {
        let width = self.doc.frame().size.width;
        let inner = (width - OUTER * 2.0).max(60.0);
        let value_x = OUTER + LABEL_W + LABEL_GAP;
        let value_w = (inner - LABEL_W - LABEL_GAP).max(60.0);
        // Every gap is added by the block that follows it, never by the one before:
        // a row that left air beneath itself would leave it under the last row of a
        // section too, and the line between two sections would sit off-centre.
        let mut y = OUTER;
        for (index, block) in self.blocks.borrow().iter().enumerate() {
            match block {
                Block::Section(view) => {
                    if index > 0 {
                        y += SECTION_GAP;
                    }
                    // The first title's capitals are level with the top of the
                    // preview beside it.
                    y -= CAP_INSET;
                    view.setFrameOrigin(NSPoint::new(OUTER, y));
                    y += view.frame().size.height;
                }
                Block::Divider(view) => {
                    y += SECTION_GAP;
                    view.setFrame(NSRect::new(NSPoint::new(OUTER, y), NSSize::new(inner, 1.0)));
                    y += 1.0;
                }
                Block::Row {
                    label,
                    views,
                    wraps,
                } => {
                    y += ROW_GAP;
                    let (mut x, mut row) = (0.0, 0.0_f64);
                    let top = y;
                    for view in views {
                        let size = view.frame().size;
                        let (w, h) = (size.width, size.height);
                        if *wraps && x > 0.0 && x + w > value_w {
                            x = 0.0;
                            y += row + GAP;
                            row = 0.0;
                        }
                        view.setFrameOrigin(NSPoint::new(value_x + x, y));
                        x += w + GAP;
                        row = row.max(h);
                    }
                    y += row;
                    let label_h = label.frame().size.height;
                    let first_line = views
                        .first()
                        .map(|view| view.frame().size.height)
                        .unwrap_or(label_h);
                    label.setFrame(NSRect::new(
                        NSPoint::new(OUTER, top + ((first_line - label_h) / 2.0).max(0.0).floor()),
                        NSSize::new(LABEL_W, label_h),
                    ));
                }
            }
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
        let room = (size.height - BAR - 1.0 - OUTER - HINT_H - PREVIEW_BOTTOM).max(1.0);
        let card_w = wide.min(room * aspect);
        self.canvas
            .place(size.height, NSSize::new(card_w, card_w / aspect));

        // The column insets its own contents by `OUTER`, and that is the whole of
        // the gutter between it and the preview.
        let left = OUTER + wide;
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
        self.canvas.set_framing(p.can_frame(), p.frame_hint());
        self.arrange();

        self.apply.setEnabled(p.can_apply());
        self.note
            .setStringValue(&NSString::from_str(&p.note().unwrap_or_default()));
    }
}
