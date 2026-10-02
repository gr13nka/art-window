//! What is inside the favourites window.
//!
//! The same arrangement as the Mac's: a column of thumbnails on the left, and on the
//! right whichever of them is being looked at, large, with what it is called and
//! the two things that can be done about it.
//!
//! Here the column is the system's own list-view in icon mode rather than a view
//! drawn by hand. Windows already knows how to scroll, select and double-click a
//! list of pictures, and a control that does it is one less thing to keep true to
//! the platform. Everything is a child of tao's window and is laid out from its
//! `WM_SIZE`; the window's messages are read by a subclass, which is how a click
//! arrives without tao having to know there is anything in there to click.

use super::{Control, Pending, Pick, Snapshot, StyleKind, Tab};
use crate::art::Artwork;
use crate::favourites::Favourites;
use crate::settings::{BlurVariant, Border, Region, Shape, Subject};
use anyhow::{anyhow, Result};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::size_of;
use std::path::Path;
use std::ptr::null_mut;
use std::rc::Rc;
use std::slice;
use tao::platform::windows::WindowExtWindows;
use tao::window::Window;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateDIBSection,
    CreateFontIndirectW, CreateFontW, CreatePen, CreateRectRgn, CreateRoundRectRgn,
    CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW, EndPaint, FillRect, GetDC, GetObjectW,
    GetStockObject, GetSysColorBrush, GetTextExtentPoint32W, InvalidateRect, ReleaseDC, RoundRect,
    SelectClipRgn, SelectObject, SetBkMode, SetBrushOrgEx, SetStretchBltMode, SetTextColor,
    StretchBlt, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLACK_BRUSH, CLEARTYPE_QUALITY,
    CLIP_DEFAULT_PRECIS, COLOR_BTNFACE, COLOR_WINDOW, DEFAULT_CHARSET, DIB_RGB_COLORS,
    DRAW_TEXT_FORMAT, DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX, DT_RIGHT, DT_SINGLELINE,
    DT_VCENTER, FW_BOLD, HALFTONE, HBITMAP, HBRUSH, HDC, HFONT, HGDIOBJ, LOGFONTW,
    OUT_DEFAULT_PRECIS, PAINTSTRUCT, PS_NULL, PS_SOLID, SRCCOPY, TRANSPARENT,
};
use windows::Win32::System::Com::{CoInitializeEx, IBindCtx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::SystemServices::{SS_ENDELLIPSIS, SS_LEFT, SS_OWNERDRAW};
use windows::Win32::UI::Controls::Dialogs::{ChooseColorW, CC_FULLOPEN, CC_RGBINIT, CHOOSECOLORW};
use windows::Win32::UI::Controls::{
    ImageList_Add, ImageList_Create, ImageList_Destroy, InitCommonControlsEx, SetWindowTheme,
    DRAWITEMSTRUCT, HIMAGELIST, ICC_LISTVIEW_CLASSES, ILC_COLOR24, INITCOMMONCONTROLSEX,
    I_IMAGENONE, LIST_VIEW_ITEM_STATE_FLAGS, LVIF_IMAGE, LVIS_FOCUSED, LVIS_SELECTED, LVITEMW,
    LVM_DELETEALLITEMS, LVM_ENSUREVISIBLE, LVM_INSERTITEMW, LVM_SETEXTENDEDLISTVIEWSTYLE,
    LVM_SETICONSPACING, LVM_SETIMAGELIST, LVM_SETITEMSTATE, LVN_ITEMCHANGED, LVSIL_NORMAL,
    LVS_AUTOARRANGE, LVS_EX_DOUBLEBUFFER, LVS_ICON, LVS_SHAREIMAGELISTS, LVS_SHOWSELALWAYS,
    LVS_SINGLESEL, NMHDR, NMITEMACTIVATE, NMLISTVIEW, NM_DBLCLK, WC_LISTVIEWW,
};
use windows::Win32::UI::HiDpi::{GetDpiForWindow, SystemParametersInfoForDpi};
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, ReleaseCapture, SetCapture};
use windows::Win32::UI::Shell::{
    DefSubclassProc, IShellItemImageFactory, RemoveWindowSubclass, SHCreateItemFromParsingName,
    SetWindowSubclass, SIIGBF_BIGGERSIZEOK, SIIGBF_RESIZETOFIT, SIIGBF_THUMBNAILONLY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetClientRect, GetWindowLongPtrW, LoadCursorW, MoveWindow,
    RegisterClassExW, SendMessageW as post, SetWindowLongPtrW, SetWindowTextW, ShowWindow,
    BN_CLICKED, BS_PUSHBUTTON, CS_HREDRAW, CS_VREDRAW, GWLP_USERDATA, HMENU, IDC_ARROW,
    NONCLIENTMETRICSW, SPI_GETNONCLIENTMETRICS, SW_HIDE, SW_SHOW, WINDOW_EX_STYLE, WINDOW_STYLE,
    WM_COMMAND, WM_DRAWITEM, WM_ERASEBKGND, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
    WM_MOUSEWHEEL, WM_NCDESTROY, WM_NOTIFY, WM_PAINT, WM_SETFONT, WM_SIZE, WNDCLASSEXW, WS_CHILD,
    WS_TABSTOP, WS_VISIBLE,
};

pub(super) fn present(window: &Window) {
    window.set_visible(true);
    window.set_focus();
}

pub(super) fn close(_window: &Window) -> bool {
    false
}

// Every measure below is in logical pixels — what it would be at 96 dpi — and is
// scaled by the window's own dpi when it is used, so the layout means the same thing
// on every display.

/// How wide the column of thumbnails is. Fixed, so that the picture beside it gets
/// every pixel the window gains.
const SHELF: i32 = 160;
/// The side of the square each thumbnail is fitted inside.
const THUMB: i32 = 120;
/// One row of the column: a thumbnail and the air around it. Narrow enough that the
/// scroll bar, when it comes, does not push a second column into being.
const CELL: i32 = THUMB + 16;
const PAD: i32 = 16;
const BUTTON_H: i32 = 28;
const SHOW_W: i32 = 168;
const FORGET_W: i32 = 96;
const LINE: i32 = 18;
const TITLE_H: i32 = 22;
/// The least the preview is asked for, in pixels. The window can be dragged larger
/// than it was when a picture was chosen, and a preview made at the size of the
/// moment would then be stretched; this much is enough for any window that fits on
/// a screen without decoding the painting again on every drag.
const PREVIEW_AT_LEAST: i32 = 1280;

/// The height of the strip of tabs above both pages.
const STRIP: i32 = 52;
const TAB_W: i32 = 104;
const TAB_H: i32 = 28;
const PREVIEW_W: i32 = 300;
const CARD_W: i32 = 78;
const CARD_H: i32 = 56;
const CHIP_H: i32 = 30;
const CHIP_PAD: i32 = 14;
const GAP: i32 = 8;
const SETTINGS_PAD: i32 = 24;

const INK: [u8; 3] = [0x1d, 0x1d, 0x1f];
const MUTED: [u8; 3] = [0x6e, 0x6e, 0x73];
const CHIP: [u8; 3] = [0xee, 0xf0, 0xf3];
const BLUE: [u8; 3] = [0x1a, 0x73, 0xe8];
const WHITE: [u8; 3] = [0xff, 0xff, 0xff];
const SURFACE_CLASS: PCWSTR = w!("ArtWindowSettings");

const SHOW_ID: i32 = 1001;
const FORGET_ID: i32 = 1002;
const CANVAS_ID: i32 = 1003;
// The artist browser's controls: the same four as the favourites'.
const ARTIST_CANVAS_ID: i32 = 1011;
const CHOOSE_ID: i32 = 1012;
const READ_ID: i32 = 1013;
const CHOOSE_W: i32 = 112;
const READ_W: i32 = 112;
/// Names this file's subclass on the window, so that it can be taken off again
/// without disturbing tao's own.
/// `LVS_NOLABELS`, which the bindings do not carry: the pictures are named in the
/// pane, and a caption under each thumbnail would be the Met's catalogue prose again.
const LVS_NOLABELS: u32 = 0x80;
const SUBCLASS_ID: usize = 0x4157;

/// A GDI bitmap that is deleted when it goes out of scope, with its size in pixels.
struct Bitmap {
    handle: HBITMAP,
    width: i32,
    height: i32,
}

impl Bitmap {
    /// Takes ownership of `handle` and reads how big it is. A bitmap that cannot be
    /// measured is deleted rather than leaked.
    fn measure(handle: HBITMAP) -> Option<Self> {
        let mut this = Self {
            handle,
            width: 0,
            height: 0,
        };
        let mut info = BITMAP::default();
        // SAFETY: `info` is a BITMAP and the size passed is exactly its size.
        let read = unsafe {
            GetObjectW(
                HGDIOBJ(handle.0),
                size_of::<BITMAP>() as i32,
                Some(&mut info as *mut BITMAP as *mut c_void),
            )
        };
        if read == 0 || info.bmWidth < 1 || info.bmHeight < 1 {
            return None;
        }
        this.width = info.bmWidth;
        this.height = info.bmHeight;
        Some(this)
    }

    /// An empty bitmap of the screen's own format, `side` pixels square.
    fn blank(side: i32) -> Option<Self> {
        // SAFETY: the screen DC is released before it can be used again.
        let handle = unsafe {
            let screen = GetDC(None);
            let handle = CreateCompatibleBitmap(screen, side, side);
            ReleaseDC(None, screen);
            handle
        };
        (!handle.is_invalid()).then(|| Self {
            handle,
            width: side,
            height: side,
        })
    }
}

impl Drop for Bitmap {
    fn drop(&mut self) {
        // SAFETY: the bitmap is ours and, by the order things are dropped in below,
        // no longer selected into any DC.
        unsafe {
            let _ = DeleteObject(HGDIOBJ(self.handle.0));
        }
    }
}

/// A memory DC with one bitmap selected into it, for as long as it is held.
struct MemDc {
    dc: HDC,
    previous: HGDIOBJ,
}

impl MemDc {
    fn holding(like: HDC, bitmap: &Bitmap) -> Option<Self> {
        // SAFETY: the DC is created here and deleted in `drop`; the bitmap outlives
        // it because every caller drops the DC first.
        unsafe {
            let dc = CreateCompatibleDC(Some(like));
            if dc.is_invalid() {
                return None;
            }
            let previous = SelectObject(dc, HGDIOBJ(bitmap.handle.0));
            Some(Self { dc, previous })
        }
    }
}

impl Drop for MemDc {
    fn drop(&mut self) {
        // SAFETY: puts back what was there so that the bitmap can be deleted, then
        // deletes the DC this created.
        unsafe {
            SelectObject(self.dc, self.previous);
            let _ = DeleteDC(self.dc);
        }
    }
}

/// Draws `bitmap` as large as fits inside `area`, centred and in proportion, and
/// says where it went — the caller owns whatever is left of the area.
///
/// Fit and letterbox, which is the same thing the desktop does with it.
fn blit_fitted(hdc: HDC, area: &RECT, bitmap: &Bitmap) -> Option<RECT> {
    let (area_w, area_h) = (area.right - area.left, area.bottom - area.top);
    if area_w < 1 || area_h < 1 {
        return None;
    }
    let scale = (area_w as f64 / bitmap.width as f64).min(area_h as f64 / bitmap.height as f64);
    let width = ((bitmap.width as f64 * scale).round() as i32).max(1);
    let height = ((bitmap.height as f64 * scale).round() as i32).max(1);
    let left = area.left + (area_w - width) / 2;
    let top = area.top + (area_h - height) / 2;

    let source = MemDc::holding(hdc, bitmap)?;
    // SAFETY: both DCs are live for the call. HALFTONE is what makes a shrunken
    // painting smooth rather than a scatter of dropped pixels, and it asks for the
    // brush origin to be reset.
    unsafe {
        SetStretchBltMode(hdc, HALFTONE);
        let _ = SetBrushOrgEx(hdc, 0, 0, None);
        let _ = StretchBlt(
            hdc,
            left,
            top,
            width,
            height,
            Some(source.dc),
            0,
            0,
            bitmap.width,
            bitmap.height,
            SRCCOPY,
        );
    }
    Some(RECT {
        left,
        top,
        right: left + width,
        bottom: top + height,
    })
}

/// A path as the shell wants it: NUL-ended UTF-16, without the `\\?\` that
/// `canonicalize` puts in front and `SHCreateItemFromParsingName` does not read.
fn shell_path(path: &Path) -> Vec<u16> {
    let text = path.to_string_lossy();
    let plain = match text.strip_prefix(r"\\?\UNC\") {
        Some(rest) => format!(r"\\{rest}"),
        None => text.strip_prefix(r"\\?\").unwrap_or(&text).to_string(),
    };
    plain.encode_utf16().chain(Some(0)).collect()
}

/// The picture at `path`, scaled by the shell to fit inside `width` by `height`.
///
/// The shell decodes and scales, so this program has no image decoder and wants
/// none — the same bargain the Mac half makes with AppKit. Thumbnails only: without
/// that, a file the shell cannot read comes back as its icon, and a generic page
/// glyph in place of a painting is worse than an empty slot.
fn shell_image(path: &Path, width: i32, height: i32) -> Option<Bitmap> {
    let wide = shell_path(path);
    // SAFETY: `wide` is NUL-terminated and outlives both calls.
    let handle = unsafe {
        let factory: IShellItemImageFactory =
            SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None::<&IBindCtx>).ok()?;
        factory
            .GetImage(
                SIZE {
                    cx: width,
                    cy: height,
                },
                SIIGBF_RESIZETOFIT | SIIGBF_BIGGERSIZEOK | SIIGBF_THUMBNAILONLY,
            )
            .ok()?
    };
    Bitmap::measure(handle)
}

/// A small copy of the picture at `path` on a `cell`-pixel square of the list's own
/// background, or nothing if it cannot be read.
///
/// Square because an image list has one size for every image it holds; the painting
/// is centred in it, which is what makes a tall one hang in the middle of its row.
/// Made at the pixels the list will show it at. Windows lays out in physical pixels
/// and an image list draws one-to-one, so the Mac's two-per-point is not needed and
/// would only make the list draw a picture twice the size it should be.
fn thumbnail(path: &Path, cell: i32) -> Option<Bitmap> {
    let raw = shell_image(path, cell, cell)?;
    let square = Bitmap::blank(cell)?;
    {
        // SAFETY: the screen DC is released at once; the two memory DCs are dropped
        // at the end of this block, before either bitmap is.
        let screen = unsafe { GetDC(None) };
        let target = MemDc::holding(screen, &square);
        unsafe { ReleaseDC(None, screen) };
        let target = target?;
        let whole = RECT {
            left: 0,
            top: 0,
            right: cell,
            bottom: cell,
        };
        // SAFETY: `target` holds the bitmap being filled.
        unsafe { FillRect(target.dc, &whole, GetSysColorBrush(COLOR_WINDOW)) };
        blit_fitted(target.dc, &whole, &raw)?;
    }
    Some(square)
}

/// One picture on a shelf, as the window has it.
#[derive(Clone)]
struct Card {
    key: String,
    art: Artwork,
    /// Whether the painter is already chosen. Always false on the favourites' shelf.
    on: bool,
    /// Why the card's main button is inert, if it is. Always `None` on the
    /// favourites' shelf.
    note: Option<String>,
}

/// Which of the two shelves a [`Browser`] is. The only thing that differs between
/// them is what the pane says and what its two buttons mean.
#[derive(Clone, Copy, PartialEq)]
enum Shelf {
    Favourites,
    Artists,
}

/// The list as it stands, and what was made for it.
///
/// One cell and not several, because every reader of any of it wants the rest.
struct Shown {
    cards: Vec<Card>,
    /// Kept by key rather than by position, so that a list rebuilt around a
    /// deletion does not silently pair a picture with somebody else's thumbnail.
    /// Keys are unique by construction — see `Favourites::free_name`. The list
    /// control's image list is rebuilt from these each time, which is why what is
    /// kept is the bitmap and not the index it happened to get.
    thumbs: HashMap<String, Bitmap>,
    /// The side, in pixels, the thumbnails were made at; a different dpi means all
    /// of them are the wrong size.
    cell: i32,
    images: Option<HIMAGELIST>,
}

struct Fonts {
    body: HFONT,
    heading: HFONT,
}

/// Everything the window's messages need to reach, at an address that stays put.
///
/// The subclass procedure is handed a pointer to this and the `Rc` in [`Content`]
/// keeps it alive until the subclass is taken off. Its interior is all `Cell` and
/// `RefCell` because a message can arrive while a method of this is on the stack —
/// deleting the list's items notifies us that they were unselected.
struct Inner {
    parent: HWND,
    favourites: Browser,
    artists: Browser,
    /// The strip of tabs, and the settings page under it. Both are windows of
    /// one class of ours, painted by hand, and both find this through their
    /// `GWLP_USERDATA`.
    strip: HWND,
    page: HWND,
    tab: Cell<Tab>,
    /// Whether the artist browser stands where the settings page would. Only ever
    /// true on the Settings tab; the page is then no more than the strip along the
    /// bottom that holds *Apply changes*.
    browsing: Cell<bool>,
    pending: RefCell<Pending>,
    /// The settings preview as a bitmap, at the pixels it is drawn at.
    picture: RefCell<Option<Bitmap>>,
    /// Set when the staged style changed while the settings page was hidden, so
    /// that nobody renders a preview nobody sees.
    stale: Cell<bool>,
    /// How far the settings column is scrolled, in pixels.
    scroll: Cell<i32>,
    dragging: Cell<bool>,
    fonts: Fonts,
    on_pick: Rc<dyn Fn(Pick)>,
}

/// One shelf and its pane: the controls, and what they currently hold. The window
/// has two, built identically — the kept pictures, and the painters to choose from.
struct Browser {
    shelf: Shelf,
    list: HWND,
    canvas: HWND,
    title: HWND,
    byline: HWND,
    /// *Set as wallpaper*, or *Choose* / *Remove*.
    primary: HWND,
    /// *Forget*, or *Read more*.
    secondary: HWND,
    shown: RefCell<Shown>,
    selected: Cell<Option<usize>>,
    /// The one painting held at its full size, and only ever one: choosing another
    /// is what lets go of this.
    preview: RefCell<Option<Bitmap>>,
    /// Set while this is changing the list itself, so that the control's
    /// notifications about it are not mistaken for somebody choosing a picture.
    quiet: Cell<bool>,
}

impl Browser {
    fn controls(&self) -> [HWND; 6] {
        [
            self.list,
            self.canvas,
            self.title,
            self.byline,
            self.primary,
            self.secondary,
        ]
    }
}

impl Inner {
    fn dpi(&self) -> f64 {
        // SAFETY: a plain query; an invalid window answers 0.
        match unsafe { GetDpiForWindow(self.parent) } {
            0 => 96.0,
            dpi => dpi as f64,
        }
    }

    fn px(&self, logical: i32) -> i32 {
        (logical as f64 * self.dpi() / 96.0).round() as i32
    }

    /// Lays every child out in the window's client area. The column keeps its width
    /// and takes the full height; the pane takes the rest, with the buttons on its
    /// floor and the picture over whatever room is left above the two lines.
    fn layout(&self) {
        let mut client = RECT::default();
        // SAFETY: `client` is a RECT.
        if unsafe { GetClientRect(self.parent, &mut client) }.is_err() {
            return;
        }
        let (width, height) = (client.right, client.bottom);
        let top = self.px(STRIP);

        let place = |hwnd: HWND, x: i32, y: i32, w: i32, h: i32| {
            // SAFETY: `hwnd` is one of this window's own children.
            let _ = unsafe { MoveWindow(hwnd, x, y, w.max(1), h.max(1), true) };
        };
        place(self.strip, 0, 0, width, top);
        // The page is the whole area under the strip, unless the artist browser is
        // standing in it: then it is only the floor, where *Apply changes* is.
        let foot = self.foot();
        if self.browsing.get() {
            place(self.page, 0, height - foot, width, foot);
        } else {
            place(self.page, 0, top, width, height - top);
        }
        self.place_browser(
            &self.favourites,
            (top, height),
            top,
            (self.px(SHOW_W), self.px(FORGET_W)),
        );
        // The artists' shelf stops above the floor, where the page's strip is.
        self.place_browser(
            &self.artists,
            (top, height - foot),
            top,
            (self.px(CHOOSE_W), self.px(READ_W)),
        );
    }

    /// The height of the strip of the page that stays up while the artist browser
    /// is open: the same air and the same button as `build` gives *Apply changes*.
    fn foot(&self) -> i32 {
        2 * self.px(SETTINGS_PAD) + self.px(CHIP_H + 6)
    }

    /// Places one browser's controls in the band `top..bottom` (the column keeps its
    /// width and takes the band from `list_top`; the pane takes the rest, with the
    /// buttons on its floor and the picture over whatever room is left above the
    /// two lines).
    fn place_browser(
        &self,
        b: &Browser,
        (top, bottom): (i32, i32),
        list_top: i32,
        (primary_w, secondary_w): (i32, i32),
    ) {
        let mut client = RECT::default();
        // SAFETY: `client` is a RECT.
        if unsafe { GetClientRect(self.parent, &mut client) }.is_err() {
            return;
        }
        let width = client.right;
        let pad = self.px(PAD);
        let shelf = self.px(SHELF);
        let left = shelf + pad;
        let wide = (width - left - pad).max(1);

        let button_h = self.px(BUTTON_H);
        let floor = bottom - pad - button_h;
        let byline_y = floor - self.px(14) - self.px(LINE);
        let title_y = byline_y - self.px(2) - self.px(TITLE_H);

        let place = |hwnd: HWND, x: i32, y: i32, w: i32, h: i32| {
            // SAFETY: `hwnd` is one of this window's own children.
            let _ = unsafe { MoveWindow(hwnd, x, y, w.max(1), h.max(1), true) };
        };
        place(b.list, 0, list_top, shelf, bottom - list_top);
        place(b.primary, left, floor, primary_w, button_h);
        place(
            b.secondary,
            left + primary_w + self.px(8),
            floor,
            secondary_w,
            button_h,
        );
        place(b.byline, left, byline_y, wide, self.px(LINE));
        place(b.title, left, title_y, wide, self.px(TITLE_H));
        place(
            b.canvas,
            left,
            top + pad,
            wide,
            title_y - self.px(8) - pad - top,
        );
    }

    /// Takes a new list, keeping what can be kept: the selection stays on the same
    /// painting where that painting is still there, and a thumbnail already made is
    /// never made twice.
    ///
    /// `want` names the card to land on instead, when it is still there.
    fn adopt(&self, b: &Browser, cards: Vec<Card>, want: Option<String>) {
        let was = want.or_else(|| self.selected_key(b));
        let cell = self.px(THUMB);

        b.quiet.set(true);
        {
            let mut shown = b.shown.borrow_mut();
            if shown.cell != cell {
                shown.thumbs.clear();
                shown.cell = cell;
            }
            shown
                .thumbs
                .retain(|key, _| cards.iter().any(|card| &card.key == key));
            for card in &cards {
                if !shown.thumbs.contains_key(&card.key) {
                    if let Some(thumb) = thumbnail(&card.art.path, cell) {
                        shown.thumbs.insert(card.key.clone(), thumb);
                    }
                }
            }
            shown.cards = cards;
        }
        self.refill_list(b);
        b.quiet.set(false);

        let row = {
            let shown = b.shown.borrow();
            was.and_then(|key| shown.cards.iter().position(|card| card.key == key))
                .or_else(|| (!shown.cards.is_empty()).then_some(0))
        };
        self.choose(b, row, true);
    }

    /// Rebuilds the control's contents from `shown`: a fresh image list, in card
    /// order, and one item per card pointing into it.
    ///
    /// The image list is rebuilt whole rather than patched, because an item names
    /// its picture by index and an index is only meaningful for as long as nothing
    /// before it is removed.
    fn refill_list(&self, b: &Browser) {
        let mut shown = b.shown.borrow_mut();
        let cell = shown.cell;
        // SAFETY: every handle is ours or the list control's; the old image list is
        // destroyed only after the control has stopped using it, and the control was
        // made with LVS_SHAREIMAGELISTS so it never destroys ours behind our back.
        unsafe {
            send(b.list, LVM_DELETEALLITEMS, 0, 0);
            let images =
                ImageList_Create(cell, cell, ILC_COLOR24, shown.cards.len().max(1) as i32, 4);
            send(
                b.list,
                LVM_SETIMAGELIST,
                LVSIL_NORMAL as usize,
                images.0 as isize,
            );
            if let Some(old) = shown.images.replace(images) {
                let _ = ImageList_Destroy(Some(old));
            }
            // The spacing is measured from one icon's left edge to the next's, and
            // is reset by swapping the image list.
            let spacing = self.px(CELL) as u32;
            send(
                b.list,
                LVM_SETICONSPACING,
                0,
                ((spacing << 16) | spacing) as isize,
            );

            for (row, card) in shown.cards.iter().enumerate() {
                let image = match shown.thumbs.get(&card.key) {
                    Some(thumb) => match ImageList_Add(images, thumb.handle, None) {
                        -1 => I_IMAGENONE,
                        index => index,
                    },
                    None => I_IMAGENONE,
                };
                let item = LVITEMW {
                    mask: LVIF_IMAGE,
                    iItem: row as i32,
                    iImage: image,
                    ..LVITEMW::default()
                };
                send(b.list, LVM_INSERTITEMW, 0, &item as *const LVITEMW as isize);
            }
        }
    }

    /// Picks out a row: lights it, if `light` says the control does not already
    /// have it lit, and fills the pane beside it.
    fn choose(&self, b: &Browser, row: Option<usize>, light: bool) {
        b.selected.set(row);
        if let (true, Some(row)) = (light, row) {
            let state = LIST_VIEW_ITEM_STATE_FLAGS(LVIS_SELECTED.0 | LVIS_FOCUSED.0);
            let item = LVITEMW {
                stateMask: state,
                state,
                ..LVITEMW::default()
            };
            b.quiet.set(true);
            // `item` outlives the call, which does not keep the pointer.
            send(
                b.list,
                LVM_SETITEMSTATE,
                row,
                &item as *const LVITEMW as isize,
            );
            send(b.list, LVM_ENSUREVISIBLE, row, 0);
            b.quiet.set(false);
        }
        let card = row.and_then(|row| b.shown.borrow().cards.get(row).cloned());
        self.point_at(b, card.as_ref());
    }

    /// Points the pane at a picture, or empties it when there is none.
    fn point_at(&self, b: &Browser, card: Option<&Card>) {
        *b.preview.borrow_mut() = card.and_then(|card| self.preview_of(b, &card.art.path));
        self.describe_card(b, card);
        // SAFETY: repaints one of our own children.
        let _ = unsafe { InvalidateRect(Some(b.canvas), None, false) };
    }

    /// Everything in the pane except the picture: the two lines, and what the two
    /// buttons say and whether they can be pressed. Apart from `point_at` so that a
    /// painter being chosen can change them without the painting being decoded again.
    fn describe_card(&self, b: &Browser, card: Option<&Card>) {
        match (b.shelf, card) {
            (Shelf::Favourites, Some(card)) => {
                let art = &card.art;
                set_text(b.title, &art.title);
                set_text(
                    b.byline,
                    if art.byline.is_empty() {
                        &art.attribution
                    } else {
                        &art.byline
                    },
                );
                enable(b.primary, true);
                enable(b.secondary, true);
            }
            (Shelf::Favourites, None) => {
                set_text(b.title, "Nothing kept yet");
                set_text(
                    b.byline,
                    "Add to favourites keeps the picture on the desktop",
                );
                enable(b.primary, false);
                enable(b.secondary, false);
            }
            (Shelf::Artists, Some(card)) => {
                let art = &card.art;
                set_text(b.title, &art.title);
                // The reason is shown where the byline is: it is the one place on
                // this window that has room to say it.
                set_text(
                    b.byline,
                    match &card.note {
                        Some(reason) => reason,
                        None if art.byline.is_empty() => &art.attribution,
                        None => &art.byline,
                    },
                );
                set_text(b.primary, if card.on { "Remove" } else { "Choose" });
                enable(b.primary, card.note.is_none());
                enable(b.secondary, art.details_url.is_some());
            }
            (Shelf::Artists, None) => {
                set_text(b.title, "No painters yet");
                set_text(b.byline, "");
                enable(b.primary, false);
                enable(b.secondary, false);
            }
        }
    }

    fn preview_of(&self, b: &Browser, path: &Path) -> Option<Bitmap> {
        let mut area = RECT::default();
        // SAFETY: `area` is a RECT.
        let _ = unsafe { GetClientRect(b.canvas, &mut area) };
        shell_image(
            path,
            area.right.max(PREVIEW_AT_LEAST),
            area.bottom.max(PREVIEW_AT_LEAST),
        )
    }

    /// The key of the picture in the pane, if there is one.
    fn selected_key(&self, b: &Browser) -> Option<String> {
        self.selected_card(b).map(|card| card.key)
    }

    fn selected_card(&self, b: &Browser) -> Option<Card> {
        let row = b.selected.get()?;
        let shown = b.shown.borrow();
        shown.cards.get(row).cloned()
    }

    /// Says what was asked of the picture in the favourites' pane.
    ///
    /// By key, and read out before anybody is told, because answering this will take
    /// the list apart underneath us — and no borrow is held while they are.
    fn ask(&self, what: fn(String) -> Pick) {
        let Some(key) = self.selected_key(&self.favourites) else {
            return;
        };
        (self.on_pick)(what(key));
    }

    /// Draws the picture, and black wherever it does not reach.
    fn paint_canvas(&self, b: &Browser, item: &DRAWITEMSTRUCT) {
        let area = item.rcItem;
        let picture = b
            .preview
            .borrow()
            .as_ref()
            .and_then(|preview| blit_fitted(item.hDC, &area, preview));
        // SAFETY: a stock brush is not ours to delete and is valid for good.
        let black = HBRUSH(unsafe { GetStockObject(BLACK_BRUSH) }.0);
        // Only the margins are filled, so that a resize does not flash the picture
        // itself through black on its way to being redrawn.
        let bands = match picture {
            Some(at) => [
                RECT {
                    bottom: at.top,
                    ..area
                },
                RECT {
                    top: at.bottom,
                    ..area
                },
                RECT {
                    top: at.top,
                    right: at.left,
                    bottom: at.bottom,
                    ..area
                },
                RECT {
                    top: at.top,
                    left: at.right,
                    bottom: at.bottom,
                    ..area
                },
            ],
            None => [area, RECT::default(), RECT::default(), RECT::default()],
        };
        for band in &bands {
            // SAFETY: `hDC` is the one the control asked us to draw on.
            unsafe { FillRect(item.hDC, band, black) };
        }
    }

    fn on_item_changed(&self, b: &Browser, change: &NMLISTVIEW) {
        let lit = LVIS_SELECTED.0;
        let newly_selected = change.uNewState & lit != 0 && change.uOldState & lit == 0;
        if b.quiet.get() || !newly_selected || change.iItem < 0 {
            return;
        }
        let row = change.iItem as usize;
        if b.selected.get() != Some(row) {
            self.choose(b, Some(row), false);
        }
    }

    fn on_double_click(&self, click: &NMITEMACTIVATE) {
        // A double-click on a picture is impatience, and means the button beside it.
        // The first click of the pair has already chosen it; one on bare ground
        // names nothing. Only the favourites' shelf listens: choosing a painter
        // twice over by a stray double-click would be an undo nobody asked for.
        if click.iItem >= 0 {
            self.ask(Pick::Show);
        }
    }
}

// ---- The settings page ------------------------------------------------------
//
// One window painted by hand, because what it holds — pills, cards, a slider, a
// switch, a preview with rounded corners — is not in the system's control set, and
// wrapping each in an owner-drawn child would put a window per chip on the screen.
// `Inner::build` says what is on the page and where; painting walks the result and
// a click walks it again, so what is drawn and what can be pressed cannot part ways.

/// What pressing an item means.
#[derive(Clone)]
enum Act {
    Style(StyleKind),
    Border(Border),
    Colour,
    Blur(BlurVariant),
    Shape(Shape),
    Region(Region),
    Subject(Subject),
    Artist(String),
    /// Opens the artist browser in place of the page.
    BrowseArtists,
    /// Closes the artist browser, back to the page.
    CloseBrowser,
    Religious,
    Slider,
    Apply,
}

enum Kind {
    Heading(String),
    Card(String, bool),
    Chip(String, bool),
    /// The custom border colour, which opens the system's colour dialog.
    Swatch([u8; 3]),
    Label(String),
    Slider(u8),
    Switch(String, bool),
    Preview,
    Note(String),
    Apply(String),
}

struct Item {
    rect: RECT,
    kind: Kind,
    act: Option<Act>,
    enabled: bool,
    /// Whether it moves with the column; the preview and the bar do not.
    scrolls: bool,
}

struct Built {
    items: Vec<Item>,
    viewport: RECT,
}

struct PageFonts {
    chip: HFONT,
    section: HFONT,
    apply: HFONT,
    note: HFONT,
}

impl PageFonts {
    fn new(px: impl Fn(i32) -> i32) -> Self {
        let make = |size: i32, weight: i32| {
            // SAFETY: a complete description; a font that cannot be made comes
            // back as a null handle, which selecting into a DC ignores.
            unsafe {
                CreateFontW(
                    -px(size),
                    0,
                    0,
                    0,
                    weight,
                    0,
                    0,
                    0,
                    DEFAULT_CHARSET,
                    OUT_DEFAULT_PRECIS,
                    CLIP_DEFAULT_PRECIS,
                    CLEARTYPE_QUALITY,
                    0,
                    w!("Segoe UI"),
                )
            }
        };
        Self {
            chip: make(13, 500),
            section: make(13, 600),
            apply: make(15, 600),
            note: make(13, 400),
        }
    }
}

impl Drop for PageFonts {
    fn drop(&mut self) {
        for font in [self.chip, self.section, self.apply, self.note] {
            // SAFETY: made in `new` and no longer selected anywhere.
            unsafe {
                let _ = DeleteObject(HGDIOBJ(font.0));
            }
        }
    }
}

fn colour(rgb: [u8; 3]) -> COLORREF {
    COLORREF(rgb[0] as u32 | (rgb[1] as u32) << 8 | (rgb[2] as u32) << 16)
}

/// `rgb` blended towards white, keeping `keep` of it — GDI has no opacity, and a
/// dimmed control on a white page is exactly this.
fn mix(rgb: [u8; 3], keep: f32) -> [u8; 3] {
    rgb.map(|c| (c as f32 * keep + 255.0 * (1.0 - keep)).round() as u8)
}

fn fill_round(hdc: HDC, r: &RECT, radius: i32, fill: [u8; 3], edge: Option<[u8; 3]>) {
    // SAFETY: every object made here is put back out of the DC and deleted.
    unsafe {
        let brush = CreateSolidBrush(colour(fill));
        let pen = match edge {
            Some(e) => CreatePen(PS_SOLID, 1, colour(e)),
            None => CreatePen(PS_NULL, 0, COLORREF(0)),
        };
        let old_brush = SelectObject(hdc, HGDIOBJ(brush.0));
        let old_pen = SelectObject(hdc, HGDIOBJ(pen.0));
        // A null pen leaves the right and bottom edge unpainted; a real one draws
        // on them. Either way the rectangle is the one asked for.
        let extra = i32::from(edge.is_none());
        let _ = RoundRect(
            hdc,
            r.left,
            r.top,
            r.right + extra,
            r.bottom + extra,
            radius * 2,
            radius * 2,
        );
        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        let _ = DeleteObject(HGDIOBJ(brush.0));
        let _ = DeleteObject(HGDIOBJ(pen.0));
    }
}

fn draw_text(hdc: HDC, text: &str, r: &RECT, font: HFONT, rgb: [u8; 3], format: DRAW_TEXT_FORMAT) {
    let mut buffer: Vec<u16> = text.encode_utf16().collect();
    let mut r = *r;
    // SAFETY: `buffer` and `r` are ours for the call; the old font is put back.
    unsafe {
        let old = SelectObject(hdc, HGDIOBJ(font.0));
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, colour(rgb));
        DrawTextW(hdc, &mut buffer, &mut r, format | DT_NOPREFIX);
        SelectObject(hdc, old);
    }
}

fn text_width(hdc: HDC, font: HFONT, text: &str) -> i32 {
    let buffer: Vec<u16> = text.encode_utf16().collect();
    let mut size = SIZE::default();
    // SAFETY: as `draw_text`.
    unsafe {
        let old = SelectObject(hdc, HGDIOBJ(font.0));
        let _ = GetTextExtentPoint32W(hdc, &buffer, &mut size);
        SelectObject(hdc, old);
    }
    size.cx
}

/// A 32-bit DIB of `image`, top-down, for `StretchBlt`.
fn dib_from_rgba(image: &image::RgbaImage) -> Option<Bitmap> {
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 {
        return None;
    }
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..BITMAPINFOHEADER::default()
        },
        ..BITMAPINFO::default()
    };
    let mut bits: *mut c_void = null_mut();
    // SAFETY: `info` describes the section; `bits` receives its pixels, which are
    // `width * height * 4` bytes and stay valid until the bitmap is deleted.
    let handle =
        unsafe { CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0) }.ok()?;
    if bits.is_null() {
        // SAFETY: made just now and not selected anywhere.
        unsafe {
            let _ = DeleteObject(HGDIOBJ(handle.0));
        }
        return None;
    }
    // SAFETY: see above; the bitmap is not in a DC, so nothing else reads it yet.
    let pixels =
        unsafe { slice::from_raw_parts_mut(bits as *mut u8, width as usize * height as usize * 4) };
    for (to, from) in pixels
        .chunks_exact_mut(4)
        .zip(image.as_raw().chunks_exact(4))
    {
        to.copy_from_slice(&[from[2], from[1], from[0], 255]);
    }
    Bitmap::measure(handle)
}

fn stretch_to(hdc: HDC, area: &RECT, bitmap: &Bitmap) {
    let Some(source) = MemDc::holding(hdc, bitmap) else {
        return;
    };
    // SAFETY: both DCs are live for the call.
    unsafe {
        SetStretchBltMode(hdc, HALFTONE);
        let _ = SetBrushOrgEx(hdc, 0, 0, None);
        let _ = StretchBlt(
            hdc,
            area.left,
            area.top,
            area.right - area.left,
            area.bottom - area.top,
            Some(source.dc),
            0,
            0,
            bitmap.width,
            bitmap.height,
            SRCCOPY,
        );
    }
}

fn inside(r: &RECT, x: i32, y: i32) -> bool {
    x >= r.left && x < r.right && y >= r.top && y < r.bottom
}

fn mouse(lparam: LPARAM) -> (i32, i32) {
    (
        (lparam.0 & 0xffff) as i16 as i32,
        ((lparam.0 >> 16) & 0xffff) as i16 as i32,
    )
}

fn wheel_delta(wparam: WPARAM) -> i32 {
    ((wparam.0 >> 16) & 0xffff) as i16 as i32
}

/// Lays out a column of pills and rows, top to bottom, in one width.
struct Column<'a> {
    hdc: HDC,
    fonts: &'a PageFonts,
    scale: f64,
    x0: i32,
    width: i32,
    y: i32,
    items: Vec<Item>,
}

impl Column<'_> {
    fn px(&self, logical: i32) -> i32 {
        (logical as f64 * self.scale).round() as i32
    }

    fn push(&mut self, rect: RECT, kind: Kind, act: Option<Act>, enabled: bool) {
        self.items.push(Item {
            rect,
            kind,
            act,
            enabled,
            scrolls: true,
        });
    }

    fn heading(&mut self, text: &str, gap_above: bool) {
        if gap_above {
            self.y += self.px(24);
        }
        let h = self.px(18);
        let rect = RECT {
            left: self.x0,
            top: self.y,
            right: self.x0 + self.width,
            bottom: self.y + h,
        };
        self.push(rect, Kind::Heading(text.to_owned()), None, true);
        self.y += h + self.px(10);
    }

    fn chips(&mut self, chips: Vec<(String, bool, Act, bool)>, enabled: bool) {
        let (h, gap, pad) = (self.px(CHIP_H), self.px(GAP), self.px(CHIP_PAD));
        let (mut x, mut y) = (0, self.y);
        for (label, selected, act, available) in chips {
            let w = text_width(self.hdc, self.fonts.chip, &label) + 2 * pad;
            if x > 0 && x + w > self.width {
                x = 0;
                y += h + gap;
            }
            let rect = RECT {
                left: self.x0 + x,
                top: y,
                right: self.x0 + x + w,
                bottom: y + h,
            };
            self.push(
                rect,
                Kind::Chip(label, selected),
                Some(act),
                enabled && available,
            );
            x += w + gap;
        }
        self.y = y + h;
    }
}

impl Inner {
    fn with_page_dc<R>(&self, f: impl FnOnce(HDC) -> R) -> R {
        // SAFETY: released before returning.
        let dc = unsafe { GetDC(Some(self.page)) };
        let out = f(dc);
        unsafe { ReleaseDC(Some(self.page), dc) };
        out
    }

    /// Everything on the settings page, placed for the window as it is now and the
    /// column scrolled as far as it is. Clamps the scroll to what there is to
    /// scroll through, since only here is the length of the column known.
    fn build(&self, hdc: HDC, fonts: &PageFonts) -> Built {
        let mut client = RECT::default();
        // SAFETY: `client` is a RECT.
        let _ = unsafe { GetClientRect(self.page, &mut client) };
        let browsing = self.browsing.get();
        let pad = self.px(SETTINGS_PAD);
        let preview_w = self.px(PREVIEW_W);
        let x0 = pad + preview_w + pad;
        let width = (client.right - x0 - pad).max(self.px(160));
        let apply_h = self.px(CHIP_H + 6);
        let bar_top = client.bottom - pad - apply_h;
        let viewport = RECT {
            left: x0,
            top: 0,
            right: x0 + width,
            bottom: (bar_top - self.px(12)).max(1),
        };

        let pending = self.pending.borrow();
        let filters = pending.filters_apply();
        let mut fixed = Vec::new();

        let preview_h = (preview_w as f64 / pending.aspect().max(0.1)).round() as i32;
        if !browsing {
            fixed.push(Item {
                rect: RECT {
                    left: pad,
                    top: pad,
                    right: pad + preview_w,
                    bottom: pad + preview_h.max(1),
                },
                kind: Kind::Preview,
                act: None,
                enabled: true,
                scrolls: false,
            });
        }
        let label = "Apply changes";
        let apply_w = text_width(hdc, fonts.apply, label) + 2 * self.px(20);
        let apply = RECT {
            left: x0 + width - apply_w,
            top: bar_top,
            right: x0 + width,
            bottom: bar_top + apply_h,
        };
        // While the artist browser is up the floor of the page also holds the way
        // out of it, at the left edge, level with *Apply changes*.
        let mut note_left = x0;
        if browsing {
            let label = "Back";
            let inset = (apply_h - self.px(CHIP_H)) / 2;
            let back = RECT {
                left: pad,
                top: bar_top + inset,
                right: pad + text_width(hdc, fonts.chip, label) + 2 * self.px(CHIP_PAD),
                bottom: bar_top + inset + self.px(CHIP_H),
            };
            note_left = note_left.max(back.right + self.px(16));
            fixed.push(Item {
                rect: back,
                kind: Kind::Chip(label.to_owned(), false),
                act: Some(Act::CloseBrowser),
                enabled: true,
                scrolls: false,
            });
        }
        if let Some(note) = pending.note() {
            fixed.push(Item {
                rect: RECT {
                    left: note_left,
                    right: apply.left - self.px(16),
                    ..apply
                },
                kind: Kind::Note(note),
                act: None,
                enabled: true,
                scrolls: false,
            });
        }
        fixed.push(Item {
            rect: apply,
            kind: Kind::Apply(label.to_owned()),
            act: Some(Act::Apply),
            enabled: pending.can_apply(),
            scrolls: false,
        });
        if browsing {
            // The page is only its floor now; the column is not on screen and the
            // scroll position is left as it was for the way back.
            return Built {
                items: fixed,
                viewport,
            };
        }

        let mut col = Column {
            hdc,
            fonts,
            scale: self.dpi() / 96.0,
            x0,
            width,
            y: pad,
            items: Vec::new(),
        };

        col.heading("Style", false);
        let selected = pending.style_kind();
        let (card_w, card_h, gap) = (col.px(CARD_W), col.px(CARD_H), col.px(GAP));
        for (i, kind) in StyleKind::ALL.into_iter().enumerate() {
            let left = x0 + i as i32 * (card_w + gap);
            let rect = RECT {
                left,
                top: col.y,
                right: left + card_w,
                bottom: col.y + card_h,
            };
            col.push(
                rect,
                Kind::Card(kind.label().to_owned(), kind == selected),
                Some(Act::Style(kind)),
                true,
            );
        }
        col.y += card_h;

        match selected {
            StyleKind::Borders => {
                col.y += col.px(12);
                let border = pending.border();
                let custom = pending.custom_colour();
                col.chips(
                    vec![
                        (
                            "Black".into(),
                            border == Border::Black,
                            Act::Border(Border::Black),
                            true,
                        ),
                        (
                            "Automatic".into(),
                            border == Border::Auto,
                            Act::Border(Border::Auto),
                            true,
                        ),
                        (
                            "Custom".into(),
                            matches!(border, Border::Custom { .. }),
                            Act::Border(Border::Custom { rgb: custom }),
                            true,
                        ),
                    ],
                    true,
                );
                if matches!(border, Border::Custom { .. }) {
                    col.y += col.px(10);
                    let (w, h) = (col.px(120), col.px(CHIP_H));
                    let rect = RECT {
                        left: x0,
                        top: col.y,
                        right: x0 + w,
                        bottom: col.y + h,
                    };
                    col.push(rect, Kind::Swatch(custom), Some(Act::Colour), true);
                    col.y += h;
                }
            }
            StyleKind::Blur => {
                col.y += col.px(12);
                let (variant, strength) = pending.blur();
                col.chips(
                    vec![
                        (
                            "Behind the picture".into(),
                            variant == BlurVariant::Backdrop,
                            Act::Blur(BlurVariant::Backdrop),
                            true,
                        ),
                        (
                            "Whole picture".into(),
                            variant == BlurVariant::WholeImage,
                            Act::Blur(BlurVariant::WholeImage),
                            true,
                        ),
                    ],
                    true,
                );
                col.y += col.px(10);
                let h = col.px(24);
                let label = RECT {
                    left: x0,
                    top: col.y,
                    right: x0 + col.px(72),
                    bottom: col.y + h,
                };
                let track = RECT {
                    left: label.right + col.px(8),
                    top: col.y,
                    right: (label.right + col.px(8) + col.px(220)).min(x0 + width),
                    bottom: col.y + h,
                };
                col.push(label, Kind::Label("Strength".into()), None, true);
                col.push(track, Kind::Slider(strength), Some(Act::Slider), true);
                col.y += h;
            }
            StyleKind::Zoom | StyleKind::Stretch => {}
        }

        col.heading("Shape", true);
        let chips = pending
            .shapes()
            .into_iter()
            .map(|c| {
                let available = c.disabled_reason.is_none();
                (c.label, c.selected, Act::Shape(c.value), available)
            })
            .collect();
        col.chips(chips, filters);
        col.heading("Origin", true);
        let chips = pending
            .regions()
            .into_iter()
            .map(|c| {
                let available = c.disabled_reason.is_none();
                (c.label, c.selected, Act::Region(c.value), available)
            })
            .collect();
        col.chips(chips, filters);
        col.heading("Subject", true);
        let chips = pending
            .subjects()
            .into_iter()
            .map(|c| {
                let available = c.disabled_reason.is_none();
                (c.label, c.selected, Act::Subject(c.value), available)
            })
            .collect();
        col.chips(chips, filters);
        // Shown even when nobody is chosen: the button is how anybody gets to be.
        let row = pending.artist_row();
        col.heading("Artist", true);
        let browse_available = row.disabled_reason.is_none();
        let mut chips: Vec<_> = row
            .chosen
            .into_iter()
            .map(|c| {
                let available = c.disabled_reason.is_none();
                (c.label, c.selected, Act::Artist(c.value), available)
            })
            .collect();
        chips.push((
            row.browse.to_owned(),
            false,
            Act::BrowseArtists,
            browse_available,
        ));
        col.chips(chips, filters);
        col.y += col.px(24);
        let h = col.px(28);
        let row = RECT {
            left: x0,
            top: col.y,
            right: x0 + width,
            bottom: col.y + h,
        };
        col.push(
            row,
            Kind::Switch("Hide religious scenes".into(), pending.hide_religious()),
            Some(Act::Religious),
            filters,
        );
        col.y += h + pad;

        let longest = (col.y - viewport.bottom).max(0);
        let scroll = self.scroll.get().clamp(0, longest);
        self.scroll.set(scroll);
        let mut items = col.items;
        for item in &mut items {
            item.rect.top -= scroll;
            item.rect.bottom -= scroll;
        }
        items.extend(fixed);
        Built { items, viewport }
    }

    fn changed(&self) {
        self.refresh_preview();
        // SAFETY: repaints one of our own children.
        let _ = unsafe { InvalidateRect(Some(self.page), None, false) };
    }

    /// Renders the staged style at the pixels the card will be drawn at. Skipped
    /// while the page is hidden, and made up for when it is shown.
    fn refresh_preview(&self) {
        if self.tab.get() != Tab::Settings {
            self.stale.set(true);
            return;
        }
        self.stale.set(false);
        let image = self.pending.borrow().preview(self.px(PREVIEW_W) as u32);
        *self.picture.borrow_mut() = image.as_ref().and_then(dib_from_rgba);
    }

    fn set_tab(&self, tab: Tab) {
        self.tab.set(tab);
        // Every way of arriving on a tab, including the one already up, lands on
        // its front page.
        self.close_browser();
        let favourites = tab == Tab::Favourites;
        // SAFETY: our own children.
        unsafe {
            let _ = InvalidateRect(Some(self.strip), None, false);
            let _ = InvalidateRect(Some(self.parent), None, true);
        }
        self.show_controls();
        if !favourites && self.stale.get() {
            self.refresh_preview();
        }
    }

    /// Shows what the tab and the browser say should be showing, and hides the rest:
    /// the favourites' shelf on its tab, on the other the page — or, in its place,
    /// the artists' shelf above the strip of the page that is left.
    fn show_controls(&self) {
        let settings = self.tab.get() == Tab::Settings;
        let browsing = settings && self.browsing.get();
        let show = |hwnd: HWND, on: bool| {
            // SAFETY: our own children.
            let _ = unsafe { ShowWindow(hwnd, if on { SW_SHOW } else { SW_HIDE }) };
        };
        for hwnd in self.favourites.controls() {
            show(hwnd, !settings);
        }
        for hwnd in self.artists.controls() {
            show(hwnd, browsing);
        }
        show(self.page, settings);
    }

    /// Puts the artist browser where the page was, on a shelf freshly made from what
    /// is chosen now.
    ///
    /// The thumbnails are made here, the first time, and not when the window opens.
    /// They are then kept by name, so the next visit pays for none of them.
    fn browse_artists(&self) {
        // Placed before it is filled: the preview is made at the pane's own size.
        self.browsing.set(true);
        self.layout();
        self.show_controls();

        let cards = self.artist_cards();
        let first_chosen = cards
            .iter()
            .find(|card| card.on)
            .map(|card| card.key.clone());
        // Without a name to land on, `adopt` would keep wherever the last visit left
        // off; a visit begins at the first painter chosen, or else the first.
        let want = first_chosen.or_else(|| cards.first().map(|card| card.key.clone()));
        self.adopt(&self.artists, cards, want);
        self.browser_moved();
    }

    /// Takes the artist browser away again, if it is up, and lets go of the one
    /// painting it holds at full size.
    fn close_browser(&self) {
        if !self.browsing.get() {
            return;
        }
        self.browsing.set(false);
        *self.artists.preview.borrow_mut() = None;
        self.layout();
        self.show_controls();
        self.browser_moved();
    }

    /// Repaints what the browser's coming and going changes the look of: the strip
    /// above it and the page below.
    fn browser_moved(&self) {
        // SAFETY: repaints our own children.
        unsafe {
            let _ = InvalidateRect(Some(self.strip), None, false);
            let _ = InvalidateRect(Some(self.page), None, true);
            let _ = InvalidateRect(Some(self.parent), None, true);
        }
    }

    /// Every painter who can be chosen, as the shelf's cards.
    fn artist_cards(&self) -> Vec<Card> {
        self.pending
            .borrow()
            .artist_cards()
            .into_iter()
            .map(|card| Card {
                key: card.art.title.clone(),
                art: card.art,
                on: card.selected,
                note: card.disabled_reason,
            })
            .collect()
    }

    /// Chooses the painter in the pane, or takes them out again.
    fn toggle_painter(&self) {
        let Some(key) = self.selected_key(&self.artists) else {
            return;
        };
        // The borrow ends with the statement: `changed` and `retag_artists` read
        // `pending` again.
        self.pending.borrow_mut().toggle_artist(&key);
        self.changed();
        self.retag_artists();
    }

    /// Brings the shelf's cards in line with what is chosen, without remaking the
    /// list or the picture: choosing one painter can make another unavailable, so
    /// every card is asked again, but only the pane's words and buttons change.
    fn retag_artists(&self) {
        let fresh = self.artist_cards();
        {
            let mut shown = self.artists.shown.borrow_mut();
            for card in &mut shown.cards {
                if let Some(now) = fresh.iter().find(|now| now.key == card.key) {
                    card.on = now.on;
                    card.note = now.note.clone();
                }
            }
        }
        let card = self.selected_card(&self.artists);
        self.describe_card(&self.artists, card.as_ref());
    }

    /// Asks for the painter's page to be read. The window is left for it, so this
    /// is the loop's to do.
    fn read_painter(&self) {
        let Some(url) = self
            .selected_card(&self.artists)
            .and_then(|card| card.art.details_url)
        else {
            return;
        };
        (self.on_pick)(Pick::Read(url));
    }

    fn strip_segments(&self, client: &RECT) -> [RECT; 2] {
        let (w, h) = (self.px(TAB_W), self.px(TAB_H));
        let left = (client.right - 2 * w) / 2;
        let top = (client.bottom - h) / 2;
        [0, 1].map(|i| RECT {
            left: left + i * w,
            top,
            right: left + (i + 1) * w,
            bottom: top + h,
        })
    }

    fn strip_click(&self, x: i32, y: i32) {
        let mut client = RECT::default();
        // SAFETY: `client` is a RECT.
        let _ = unsafe { GetClientRect(self.strip, &mut client) };
        let [favourites, settings] = self.strip_segments(&client);
        if inside(&favourites, x, y) {
            self.set_tab(Tab::Favourites);
        } else if inside(&settings, x, y) {
            self.set_tab(Tab::Settings);
        }
    }

    fn wheel(&self, delta: i32) {
        // A notch is 120; three lines of the column's own rhythm to a notch.
        self.scroll
            .set((self.scroll.get() - delta * self.px(48) / 120).max(0));
        // SAFETY: repaints one of our own children.
        let _ = unsafe { InvalidateRect(Some(self.page), None, false) };
    }

    fn page_press(&self, x: i32, y: i32) {
        let fonts = PageFonts::new(|s| self.px(s));
        let built = self.with_page_dc(|dc| self.build(dc, &fonts));
        let hit = built.items.iter().rev().find(|item| {
            item.enabled
                && item.act.is_some()
                && inside(&item.rect, x, y)
                && (!item.scrolls || inside(&built.viewport, x, y))
        });
        let Some(Item { act: Some(act), .. }) = hit else {
            return;
        };
        let act = act.clone();
        drop(built);
        match act {
            Act::Slider => {
                self.dragging.set(true);
                // SAFETY: capture is released on button-up.
                unsafe { SetCapture(self.page) };
                self.slide(x);
                return;
            }
            Act::Apply => {
                let staged = self.pending.borrow().staged().clone();
                (self.on_pick)(Pick::Apply(staged));
                return;
            }
            Act::Colour => {
                let current = self.pending.borrow().custom_colour();
                let Some(rgb) = self.choose_colour(current) else {
                    return;
                };
                self.pending.borrow_mut().set_border(Border::Custom { rgb });
            }
            Act::Style(kind) => self.pending.borrow_mut().set_style(kind),
            Act::Border(border) => self.pending.borrow_mut().set_border(border),
            Act::Blur(variant) => self.pending.borrow_mut().set_blur_variant(variant),
            Act::Shape(shape) => self.pending.borrow_mut().set_shape(shape),
            Act::Region(region) => self.pending.borrow_mut().toggle_region(region),
            Act::Subject(subject) => self.pending.borrow_mut().toggle_subject(subject),
            Act::Artist(name) => self.pending.borrow_mut().toggle_artist(&name),
            Act::BrowseArtists => {
                self.browse_artists();
                return;
            }
            Act::CloseBrowser => {
                self.close_browser();
                return;
            }
            Act::Religious => {
                let hide = self.pending.borrow().hide_religious();
                self.pending.borrow_mut().set_hide_religious(!hide);
            }
        }
        self.changed();
    }

    /// Moves the strength to where the pointer is along the track.
    fn slide(&self, x: i32) {
        let fonts = PageFonts::new(|s| self.px(s));
        let built = self.with_page_dc(|dc| self.build(dc, &fonts));
        let Some(track) = built
            .items
            .iter()
            .find(|item| matches!(item.kind, Kind::Slider(_)))
            .map(|item| item.rect)
        else {
            return;
        };
        let along = (x - track.left) as f64 / (track.right - track.left).max(1) as f64;
        let strength = (along.clamp(0.0, 1.0) * 100.0).round() as u8;
        self.pending.borrow_mut().set_blur_strength(strength);
        self.changed();
    }

    fn choose_colour(&self, current: [u8; 3]) -> Option<[u8; 3]> {
        let mut custom = [COLORREF(0x00ff_ffff); 16];
        let mut dialog = CHOOSECOLORW {
            lStructSize: size_of::<CHOOSECOLORW>() as u32,
            hwndOwner: self.parent,
            rgbResult: colour(current),
            lpCustColors: custom.as_mut_ptr(),
            Flags: CC_RGBINIT | CC_FULLOPEN,
            ..CHOOSECOLORW::default()
        };
        // SAFETY: `dialog` and the custom-colour table outlive the modal call, and
        // no borrow of `pending` is held across it.
        if !unsafe { ChooseColorW(&mut dialog) }.as_bool() {
            return None;
        }
        let c = dialog.rgbResult.0;
        Some([c as u8, (c >> 8) as u8, (c >> 16) as u8])
    }

    fn paint_page(&self, dc: HDC, client: &RECT) {
        // SAFETY: the brush is made, used and deleted here; the system's is not ours.
        unsafe {
            if self.browsing.get() {
                // The floor of the page sits under a shelf drawn on the window's own
                // face, and matches it.
                FillRect(dc, client, GetSysColorBrush(COLOR_BTNFACE));
            } else {
                let white = CreateSolidBrush(colour(WHITE));
                FillRect(dc, client, white);
                let _ = DeleteObject(HGDIOBJ(white.0));
            }
        }
        let fonts = PageFonts::new(|s| self.px(s));
        let built = self.build(dc, &fonts);
        let v = built.viewport;
        // SAFETY: the region is deleted below, after being taken out of the DC.
        let clip = unsafe { CreateRectRgn(v.left, v.top, v.right, v.bottom) };
        for item in &built.items {
            if item.scrolls && (item.rect.bottom < v.top || item.rect.top > v.bottom) {
                continue;
            }
            // SAFETY: as above.
            unsafe {
                SelectClipRgn(dc, if item.scrolls { Some(clip) } else { None });
            }
            self.paint_item(dc, item, &fonts);
        }
        // SAFETY: as above.
        unsafe {
            SelectClipRgn(dc, None);
            let _ = DeleteObject(HGDIOBJ(clip.0));
        }
    }

    fn paint_item(&self, dc: HDC, item: &Item, fonts: &PageFonts) {
        let r = &item.rect;
        let keep = if item.enabled { 1.0 } else { 0.5 };
        let h = r.bottom - r.top;
        let centred = DT_CENTER | DT_VCENTER | DT_SINGLELINE;
        match &item.kind {
            Kind::Heading(text) => draw_text(
                dc,
                text,
                r,
                fonts.section,
                MUTED,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE,
            ),
            Kind::Label(text) => draw_text(
                dc,
                text,
                r,
                fonts.chip,
                INK,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE,
            ),
            Kind::Card(text, on) | Kind::Chip(text, on) => {
                let radius = if matches!(item.kind, Kind::Card(..)) {
                    self.px(10)
                } else {
                    h / 2
                };
                let (fill, ink) = if *on { (INK, WHITE) } else { (CHIP, INK) };
                fill_round(dc, r, radius, mix(fill, keep), None);
                draw_text(dc, text, r, fonts.chip, mix(ink, keep), centred);
            }
            Kind::Swatch(rgb) => {
                fill_round(dc, r, h / 2, *rgb, Some([0xd0, 0xd0, 0xd5]));
                let luma = 0.299 * rgb[0] as f32 + 0.587 * rgb[1] as f32 + 0.114 * rgb[2] as f32;
                let ink = if luma > 150.0 { INK } else { WHITE };
                draw_text(dc, "Choose colour", r, fonts.chip, ink, centred);
            }
            Kind::Slider(value) => {
                let knob = self.px(16);
                let cy = (r.top + r.bottom) / 2;
                let track = self.px(4);
                let x = r.left + knob / 2 + ((r.right - r.left - knob) * *value as i32) / 100;
                let bar = |left, right, rgb| {
                    let rect = RECT {
                        left,
                        top: cy - track / 2,
                        right,
                        bottom: cy + track / 2,
                    };
                    fill_round(dc, &rect, track / 2, rgb, None);
                };
                bar(r.left, r.right, [0xd8, 0xda, 0xdf]);
                bar(r.left, x, INK);
                let ball = RECT {
                    left: x - knob / 2,
                    top: cy - knob / 2,
                    right: x + knob / 2,
                    bottom: cy + knob / 2,
                };
                fill_round(dc, &ball, knob / 2, INK, None);
            }
            Kind::Switch(text, on) => {
                draw_text(
                    dc,
                    text,
                    r,
                    fonts.chip,
                    mix(INK, keep),
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE,
                );
                let (w, sh) = (self.px(44), self.px(26));
                let cy = (r.top + r.bottom) / 2;
                let track = RECT {
                    left: r.right - w,
                    top: cy - sh / 2,
                    right: r.right,
                    bottom: cy + sh / 2,
                };
                let fill = if *on { INK } else { [0xd1, 0xd1, 0xd6] };
                fill_round(dc, &track, sh / 2, mix(fill, keep), None);
                let (inset, d) = (self.px(2), sh - 2 * self.px(2));
                let left = if *on {
                    track.right - inset - d
                } else {
                    track.left + inset
                };
                let knob = RECT {
                    left,
                    top: track.top + inset,
                    right: left + d,
                    bottom: track.top + inset + d,
                };
                fill_round(dc, &knob, d / 2, WHITE, None);
            }
            Kind::Preview => {
                let radius = self.px(12);
                // A soft shadow, as stacked translucent-looking rounded rectangles.
                for i in (1..=5).rev() {
                    let grow = self.px(3) * i;
                    let shade = 255 - (6 - i as u8) * 2;
                    let ring = RECT {
                        left: r.left - grow,
                        top: r.top - grow + self.px(10),
                        right: r.right + grow,
                        bottom: r.bottom + grow + self.px(10),
                    };
                    fill_round(dc, &ring, radius + grow, [shade; 3], None);
                }
                match self.picture.borrow().as_ref() {
                    Some(bitmap) => {
                        // SAFETY: the region is taken out of the DC before it is
                        // deleted.
                        unsafe {
                            let clip = CreateRoundRectRgn(
                                r.left,
                                r.top,
                                r.right + 1,
                                r.bottom + 1,
                                radius * 2,
                                radius * 2,
                            );
                            SelectClipRgn(dc, Some(clip));
                            stretch_to(dc, r, bitmap);
                            SelectClipRgn(dc, None);
                            let _ = DeleteObject(HGDIOBJ(clip.0));
                        }
                    }
                    None => {
                        fill_round(dc, r, radius, CHIP, None);
                        draw_text(dc, "No picture yet", r, fonts.note, MUTED, centred);
                    }
                }
            }
            Kind::Note(text) => draw_text(
                dc,
                text,
                r,
                fonts.note,
                MUTED,
                DT_RIGHT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
            ),
            Kind::Apply(text) => {
                let keep = if item.enabled { 1.0 } else { 0.4 };
                fill_round(dc, r, h / 2, mix(BLUE, keep), None);
                draw_text(dc, text, r, fonts.apply, WHITE, centred);
            }
        }
    }

    fn paint_strip(&self, dc: HDC, client: &RECT) {
        let settings = self.tab.get() == Tab::Settings;
        // SAFETY: the system brush is not ours; the white one is deleted after use.
        unsafe {
            if settings && !self.browsing.get() {
                let white = CreateSolidBrush(colour(WHITE));
                FillRect(dc, client, white);
                let _ = DeleteObject(HGDIOBJ(white.0));
            } else {
                FillRect(dc, client, GetSysColorBrush(COLOR_BTNFACE));
            }
        }
        let fonts = PageFonts::new(|s| self.px(s));
        let [a, b] = self.strip_segments(client);
        let pill = RECT {
            right: b.right,
            ..a
        };
        let radius = (a.bottom - a.top) / 2;
        fill_round(dc, &pill, radius, CHIP, None);
        for (segment, label, on) in [(a, "Favourites", !settings), (b, "Settings", settings)] {
            if on {
                fill_round(dc, &segment, radius, INK, None);
            }
            draw_text(
                dc,
                label,
                &segment,
                fonts.chip,
                if on { WHITE } else { INK },
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
        }
    }
}

fn register_surface_class() -> Result<()> {
    // SAFETY: the class is described in full; registering it twice fails harmlessly
    // with "already exists", which is why the result is not read.
    unsafe {
        let module = GetModuleHandleW(None)?;
        let class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(surface_proc),
            hInstance: HINSTANCE(module.0),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            lpszClassName: SURFACE_CLASS,
            ..WNDCLASSEXW::default()
        };
        RegisterClassExW(&class);
    }
    Ok(())
}

/// A window of the hand-painted class, unplaced.
fn surface(parent: HWND, visible: bool) -> Result<HWND> {
    // SAFETY: the class was registered by `register_surface_class`.
    unsafe {
        let module = GetModuleHandleW(None)?;
        let style = WS_CHILD.0 | if visible { WS_VISIBLE.0 } else { 0 };
        Ok(CreateWindowExW(
            WINDOW_EX_STYLE(0),
            SURFACE_CLASS,
            PCWSTR::null(),
            WINDOW_STYLE(style),
            0,
            0,
            0,
            0,
            Some(parent),
            None,
            Some(module.into()),
            None,
        )?)
    }
}

/// Paints into a memory bitmap and copies it over in one go, so that scrolling and
/// dragging do not flicker.
fn paint_buffered(hwnd: HWND, draw: impl FnOnce(HDC, &RECT)) {
    let mut ps = PAINTSTRUCT::default();
    let mut client = RECT::default();
    // SAFETY: BeginPaint/EndPaint bracket everything; the bitmap and memory DC are
    // dropped (DC first) before EndPaint.
    unsafe {
        let hdc = BeginPaint(hwnd, &mut ps);
        let _ = GetClientRect(hwnd, &mut client);
        let handle = CreateCompatibleBitmap(hdc, client.right.max(1), client.bottom.max(1));
        if let Some(bitmap) = Bitmap::measure(handle) {
            if let Some(mem) = MemDc::holding(hdc, &bitmap) {
                draw(mem.dc, &client);
                let _ = BitBlt(
                    hdc,
                    0,
                    0,
                    client.right,
                    client.bottom,
                    Some(mem.dc),
                    0,
                    0,
                    SRCCOPY,
                );
            }
        }
        let _ = EndPaint(hwnd, &ps);
    }
}

/// The window procedure of the strip and the page. A click here says what was asked
/// through `Inner`, which answers by `on_pick` where it leaves the window at all.
unsafe extern "system" fn surface_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: the pointer is set after both windows exist and cleared when either
    // is destroyed; `Inner` outlives them.
    let inner = unsafe { (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Inner).as_ref() };
    if let Some(inner) = inner {
        let strip = hwnd == inner.strip;
        match message {
            WM_ERASEBKGND => return LRESULT(1),
            WM_PAINT => {
                paint_buffered(hwnd, |dc, client| {
                    if strip {
                        inner.paint_strip(dc, client)
                    } else {
                        inner.paint_page(dc, client)
                    }
                });
                return LRESULT(0);
            }
            WM_LBUTTONDOWN => {
                let (x, y) = mouse(lparam);
                if strip {
                    inner.strip_click(x, y);
                } else {
                    inner.page_press(x, y);
                }
                return LRESULT(0);
            }
            WM_MOUSEMOVE if !strip && inner.dragging.get() => {
                inner.slide(mouse(lparam).0);
                return LRESULT(0);
            }
            WM_LBUTTONUP if inner.dragging.get() => {
                inner.dragging.set(false);
                // SAFETY: releases the capture taken on button-down.
                let _ = unsafe { ReleaseCapture() };
                return LRESULT(0);
            }
            WM_MOUSEWHEEL if !strip => {
                inner.wheel(wheel_delta(wparam));
                return LRESULT(0);
            }
            WM_NCDESTROY => {
                // SAFETY: nothing may reach `Inner` through a dying window.
                unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) };
            }
            _ => {}
        }
    }
    // SAFETY: the default handling of the same message.
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

impl Drop for Inner {
    fn drop(&mut self) {
        // SAFETY: the window's children are already gone or going with it; what is
        // left is what was made here and is not theirs to free. Fonts and the image
        // list are ours because the list control shares rather than owns them.
        unsafe {
            let _ = RemoveWindowSubclass(self.parent, Some(subclass_proc), SUBCLASS_ID);
            let _ = DeleteObject(HGDIOBJ(self.fonts.body.0));
            let _ = DeleteObject(HGDIOBJ(self.fonts.heading.0));
            for browser in [&self.favourites, &self.artists] {
                if let Some(images) = browser.shown.borrow_mut().images.take() {
                    let _ = ImageList_Destroy(Some(images));
                }
            }
        }
    }
}

/// `SendMessageW` with the parameters spelled as the numbers they are.
fn send(hwnd: HWND, message: u32, wparam: usize, lparam: isize) -> LRESULT {
    // SAFETY: every caller passes a message and a parameter that belong together.
    unsafe { post(hwnd, message, Some(WPARAM(wparam)), Some(LPARAM(lparam))) }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16()
        .filter(|&unit| unit != 0)
        .chain(Some(0))
        .collect()
}

fn set_text(hwnd: HWND, text: &str) {
    let text = wide(text);
    // SAFETY: NUL-terminated, and copied by the call.
    let _ = unsafe { SetWindowTextW(hwnd, PCWSTR(text.as_ptr())) };
}

fn enable(hwnd: HWND, on: bool) {
    // SAFETY: one of our own children.
    let _ = unsafe { EnableWindow(hwnd, on) };
}

/// The messages this file cares about, on the way to tao's own handling.
///
/// A click in the window is answered by the loop, not where it lands: this runs in
/// the middle of one, so it says what was asked through `on_pick` and does nothing
/// about it.
unsafe extern "system" fn subclass_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    data: usize,
) -> LRESULT {
    // SAFETY: `data` is the pointer `Content::install` took from an `Rc<Inner>` that
    // it keeps alive until this subclass is removed.
    let inner = unsafe { &*(data as *const Inner) };
    match message {
        WM_SIZE => inner.layout(),
        // The wheel goes to whichever window has the keyboard, which is never the
        // page; it arrives here, and means the settings column when that is up.
        WM_MOUSEWHEEL if inner.tab.get() == Tab::Settings && !inner.browsing.get() => {
            inner.wheel(wheel_delta(wparam));
            return LRESULT(0);
        }
        // tao's class has no background brush and only paints one when the program
        // asked for a colour, so what lies between the children is ours to paint.
        WM_ERASEBKGND => {
            let mut client = RECT::default();
            // SAFETY: `wparam` is the DC to paint on for this message.
            unsafe {
                let _ = GetClientRect(hwnd, &mut client);
                FillRect(
                    windows::Win32::Graphics::Gdi::HDC(wparam.0 as *mut c_void),
                    &client,
                    GetSysColorBrush(COLOR_BTNFACE),
                );
            }
            return LRESULT(1);
        }
        WM_DRAWITEM if wparam.0 as i32 == CANVAS_ID || wparam.0 as i32 == ARTIST_CANVAS_ID => {
            let browser = if wparam.0 as i32 == CANVAS_ID {
                &inner.favourites
            } else {
                &inner.artists
            };
            // SAFETY: for WM_DRAWITEM, `lparam` points to a DRAWITEMSTRUCT.
            inner.paint_canvas(browser, unsafe { &*(lparam.0 as *const DRAWITEMSTRUCT) });
            return LRESULT(1);
        }
        WM_NOTIFY => {
            // SAFETY: for WM_NOTIFY, `lparam` points to an NMHDR, and to the larger
            // structure its `code` names.
            let header = unsafe { &*(lparam.0 as *const NMHDR) };
            for browser in [&inner.favourites, &inner.artists] {
                if header.hwndFrom != browser.list {
                    continue;
                }
                match header.code {
                    LVN_ITEMCHANGED => {
                        inner.on_item_changed(browser, unsafe { &*(lparam.0 as *const NMLISTVIEW) })
                    }
                    NM_DBLCLK if browser.shelf == Shelf::Favourites => {
                        inner.on_double_click(unsafe { &*(lparam.0 as *const NMITEMACTIVATE) })
                    }
                    _ => {}
                }
            }
        }
        WM_COMMAND if (wparam.0 >> 16) as u32 == BN_CLICKED => {
            match (wparam.0 & 0xffff) as i32 {
                SHOW_ID => inner.ask(Pick::Show),
                FORGET_ID => inner.ask(Pick::Forget),
                CHOOSE_ID => inner.toggle_painter(),
                READ_ID => inner.read_painter(),
                _ => {}
            }
            return LRESULT(0);
        }
        // The window is going and takes its children with it; nothing here may be
        // called on them again.
        WM_NCDESTROY => {
            // SAFETY: removing the subclass that is running is what comctl32 allows.
            let _ = unsafe { RemoveWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID) };
        }
        _ => {}
    }
    // SAFETY: passes the message on unchanged.
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

/// The body font and a bold one a quarter larger, at `dpi` — what the rest of the
/// system's dialogs use, so that the window does not look like Windows 95.
fn system_fonts(dpi: u32) -> Fonts {
    let mut metrics = NONCLIENTMETRICSW {
        cbSize: size_of::<NONCLIENTMETRICSW>() as u32,
        ..NONCLIENTMETRICSW::default()
    };
    // SAFETY: `metrics` is a NONCLIENTMETRICSW with its size stated. If the query
    // fails the zeroed font description that is left still names a usable default.
    unsafe {
        let _ = SystemParametersInfoForDpi(
            SPI_GETNONCLIENTMETRICS.0,
            metrics.cbSize,
            Some(&mut metrics as *mut NONCLIENTMETRICSW as *mut c_void),
            0,
            dpi,
        );
    }
    let body: LOGFONTW = metrics.lfMessageFont;
    let heading = LOGFONTW {
        lfWeight: FW_BOLD.0 as i32,
        lfHeight: body.lfHeight * 5 / 4,
        ..body
    };
    // SAFETY: both descriptions are complete.
    unsafe {
        Fonts {
            body: CreateFontIndirectW(&body),
            heading: CreateFontIndirectW(&heading),
        }
    }
}

/// A child of `parent` of the given class, unplaced — `Inner::layout` does that.
fn child(
    parent: HWND,
    class: PCWSTR,
    text: PCWSTR,
    style: u32,
    id: i32,
    font: HFONT,
) -> Result<HWND> {
    // SAFETY: the class names are the system's own, `id` is passed as the child's
    // control id, and the module handle is this program's.
    unsafe {
        let module = GetModuleHandleW(None)?;
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            text,
            WS_CHILD | WS_VISIBLE | WINDOW_STYLE(style),
            0,
            0,
            0,
            0,
            Some(parent),
            Some(HMENU(id as usize as *mut c_void)),
            Some(module.into()),
            None,
        )?;
        send(hwnd, WM_SETFONT, font.0 as usize, 1);
        Ok(hwnd)
    }
}

/// One shelf with its pane — the list, the canvas, the two lines and the two
/// buttons — as children of `parent`, unplaced.
fn browser(
    parent: HWND,
    fonts: &Fonts,
    shelf: Shelf,
    canvas_id: i32,
    primary: (PCWSTR, i32),
    secondary: (PCWSTR, i32),
) -> Result<Browser> {
    let list = child(
        parent,
        WC_LISTVIEWW,
        PCWSTR::null(),
        LVS_ICON
            | LVS_SINGLESEL
            | LVS_SHOWSELALWAYS
            | LVS_NOLABELS
            | LVS_AUTOARRANGE
            | LVS_SHAREIMAGELISTS
            | WS_TABSTOP.0,
        0,
        fonts.body,
    )
    .map_err(|e| anyhow!("building the picture list: {e}"))?;
    // SAFETY: both are messages the list control defines. Double buffering stops
    // the column flickering as it is refilled; the Explorer theme is what gives
    // a selected picture the system's own highlight rather than a grey box.
    unsafe {
        send(
            list,
            LVM_SETEXTENDEDLISTVIEWSTYLE,
            LVS_EX_DOUBLEBUFFER as usize,
            LVS_EX_DOUBLEBUFFER as isize,
        );
        let _ = SetWindowTheme(list, w!("Explorer"), PCWSTR::null());
    }

    let canvas = child(
        parent,
        w!("STATIC"),
        PCWSTR::null(),
        SS_OWNERDRAW.0,
        canvas_id,
        fonts.body,
    )?;
    let title = child(
        parent,
        w!("STATIC"),
        PCWSTR::null(),
        SS_LEFT.0 | SS_ENDELLIPSIS.0,
        0,
        fonts.heading,
    )?;
    let byline = child(
        parent,
        w!("STATIC"),
        PCWSTR::null(),
        SS_LEFT.0 | SS_ENDELLIPSIS.0,
        0,
        fonts.body,
    )?;
    let button = |(label, id): (PCWSTR, i32)| {
        child(
            parent,
            w!("BUTTON"),
            label,
            WS_TABSTOP.0 | BS_PUSHBUTTON as u32,
            id,
            fonts.body,
        )
    };
    let primary = button(primary)?;
    let secondary = button(secondary)?;

    Ok(Browser {
        shelf,
        list,
        canvas,
        title,
        byline,
        primary,
        secondary,
        shown: RefCell::new(Shown {
            cards: Vec::new(),
            thumbs: HashMap::new(),
            cell: 0,
            images: None,
        }),
        selected: Cell::new(None),
        preview: RefCell::new(None),
        quiet: Cell::new(false),
    })
}

/// The window's contents. Dropping it takes the subclass off and frees what was
/// made; the children themselves go with tao's window.
pub struct Content {
    inner: Rc<Inner>,
}

impl Content {
    /// Fills `window` with the list, the pane and the buttons, and aims the clicks
    /// at `on_pick`.
    pub fn install(
        window: &Window,
        on_pick: Rc<dyn Fn(Pick)>,
        _on_control: Rc<dyn Fn(Control)>,
    ) -> Result<Self> {
        // COM is what the shell's thumbnail factory speaks. Whatever this answers —
        // newly started, already started, or started in another mode by somebody
        // else — COM is usable afterwards, so none of it is an error. It is never
        // uninitialised: the thread is the event loop's and lives as long as the
        // program does.
        // SAFETY: called on the thread that will make the shell calls.
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        let icc = INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_LISTVIEW_CLASSES,
        };
        // SAFETY: `icc` is filled in.
        unsafe { InitCommonControlsEx(&icc) }
            .ok()
            .map_err(|e| anyhow!("starting the list control: {e}"))?;

        // tao owns the window; everything below is its child and goes when it goes.
        let parent = HWND(window.hwnd() as *mut c_void);
        // SAFETY: a plain query.
        let dpi = match unsafe { GetDpiForWindow(parent) } {
            0 => 96,
            dpi => dpi,
        };
        let fonts = system_fonts(dpi);

        let favourites = browser(
            parent,
            &fonts,
            Shelf::Favourites,
            CANVAS_ID,
            (w!("Set as wallpaper"), SHOW_ID),
            (w!("Forget"), FORGET_ID),
        )?;
        let artists = browser(
            parent,
            &fonts,
            Shelf::Artists,
            ARTIST_CANVAS_ID,
            (w!("Choose"), CHOOSE_ID),
            (w!("Read more"), READ_ID),
        )?;
        register_surface_class()?;
        let strip = surface(parent, true)?;
        let page = surface(parent, false)?;

        let inner = Rc::new(Inner {
            parent,
            favourites,
            artists,
            strip,
            page,
            tab: Cell::new(Tab::Favourites),
            browsing: Cell::new(false),
            pending: RefCell::new(Pending::new(
                crate::settings::Settings::default(),
                true,
                16.0 / 10.0,
            )),
            picture: RefCell::new(None),
            stale: Cell::new(true),
            scroll: Cell::new(0),
            dragging: Cell::new(false),
            fonts,
            on_pick,
        });

        // SAFETY: the pointer stays valid for as long as the subclass is on the
        // window, which `Inner`'s `Drop` (and `WM_NCDESTROY`) end.
        unsafe {
            SetWindowSubclass(
                parent,
                Some(subclass_proc),
                SUBCLASS_ID,
                Rc::as_ptr(&inner) as usize,
            )
        }
        .ok()
        .map_err(|e| anyhow!("listening to the favourites window: {e}"))?;

        // SAFETY: the pointer stays valid while either window lives; `Inner` is
        // dropped after tao's window, and `WM_NCDESTROY` clears it besides.
        unsafe {
            for hwnd in [strip, page] {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, Rc::as_ptr(&inner) as isize);
            }
        }
        inner.layout();
        inner.point_at(&inner.favourites, None);
        inner.point_at(&inner.artists, None);
        inner.set_tab(Tab::Favourites);
        Ok(Self { inner })
    }

    pub fn relist(&self, favourites: &Favourites) {
        self.inner.adopt(
            &self.inner.favourites,
            favourites
                .iter()
                .map(|(key, art)| Card {
                    key: key.to_string(),
                    art: art.clone(),
                    on: false,
                    note: None,
                })
                .collect(),
            None,
        );
    }

    pub fn describe(&self, snapshot: &Snapshot, favourites: &Favourites) {
        {
            let mut pending = self.inner.pending.borrow_mut();
            pending.keep_pictures_in(&snapshot.pictures);
            pending.adopt(&snapshot.settings, snapshot.filters_apply, snapshot.aspect);
            pending.set_picture(snapshot.shown.as_ref().map(|art| art.path.as_path()));
        }
        self.inner.changed();
        // The shelf is made when it is first opened; one that is open already is
        // told only what a new painting or new settings can change about it.
        if self.inner.browsing.get() {
            self.inner.retag_artists();
        }
        self.relist(favourites);
    }

    pub fn show_tab(&self, tab: Tab) {
        self.inner.set_tab(tab);
    }

    pub fn describe_status(&self, _snapshot: &Snapshot) {}

    pub fn set_login(&self, _enabled: bool) {}
}
