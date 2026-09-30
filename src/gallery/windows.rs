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

use super::{Control, Pick, Snapshot};
use crate::art::Artwork;
use crate::favourites::Favourites;
use anyhow::{anyhow, Result};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::size_of;
use std::path::Path;
use std::rc::Rc;
use tao::platform::windows::WindowExtWindows;
use tao::window::Window;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleBitmap, CreateCompatibleDC, CreateFontIndirectW, DeleteDC, DeleteObject,
    FillRect, GetDC, GetObjectW, GetStockObject, GetSysColorBrush, InvalidateRect, ReleaseDC,
    SelectObject, SetBrushOrgEx, SetStretchBltMode, StretchBlt, BITMAP, BLACK_BRUSH, COLOR_BTNFACE,
    COLOR_WINDOW, FW_BOLD, HALFTONE, HBITMAP, HBRUSH, HDC, HFONT, HGDIOBJ, LOGFONTW, SRCCOPY,
};
use windows::Win32::System::Com::{CoInitializeEx, IBindCtx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::SystemServices::{SS_ENDELLIPSIS, SS_LEFT, SS_OWNERDRAW};
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
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows::Win32::UI::Shell::{
    DefSubclassProc, IShellItemImageFactory, RemoveWindowSubclass, SHCreateItemFromParsingName,
    SetWindowSubclass, SIIGBF_BIGGERSIZEOK, SIIGBF_RESIZETOFIT, SIIGBF_THUMBNAILONLY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, GetClientRect, MoveWindow, SendMessageW as post, SetWindowTextW, BN_CLICKED,
    BS_PUSHBUTTON, HMENU, NONCLIENTMETRICSW, SPI_GETNONCLIENTMETRICS, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_COMMAND, WM_DRAWITEM, WM_ERASEBKGND, WM_NCDESTROY, WM_NOTIFY, WM_SETFONT,
    WM_SIZE, WS_CHILD, WS_TABSTOP, WS_VISIBLE,
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

const SHOW_ID: i32 = 1001;
const FORGET_ID: i32 = 1002;
const CANVAS_ID: i32 = 1003;
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

/// One kept picture, as the window has it.
struct Card {
    key: String,
    art: Artwork,
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
    list: HWND,
    canvas: HWND,
    title: HWND,
    byline: HWND,
    show: HWND,
    forget: HWND,
    fonts: Fonts,
    shown: RefCell<Shown>,
    selected: Cell<Option<usize>>,
    /// The one painting held at its full size, and only ever one: choosing another
    /// is what lets go of this.
    preview: RefCell<Option<Bitmap>>,
    /// Set while this is changing the list itself, so that the control's
    /// notifications about it are not mistaken for somebody choosing a picture.
    quiet: Cell<bool>,
    on_pick: Rc<dyn Fn(Pick)>,
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
        let pad = self.px(PAD);
        let shelf = self.px(SHELF);
        let left = shelf + pad;
        let wide = (width - left - pad).max(1);

        let button_h = self.px(BUTTON_H);
        let floor = height - pad - button_h;
        let byline_y = floor - self.px(14) - self.px(LINE);
        let title_y = byline_y - self.px(2) - self.px(TITLE_H);

        let place = |hwnd: HWND, x: i32, y: i32, w: i32, h: i32| {
            // SAFETY: `hwnd` is one of this window's own children.
            let _ = unsafe { MoveWindow(hwnd, x, y, w.max(1), h.max(1), true) };
        };
        place(self.list, 0, 0, shelf, height);
        place(self.show, left, floor, self.px(SHOW_W), button_h);
        place(
            self.forget,
            left + self.px(SHOW_W) + self.px(8),
            floor,
            self.px(FORGET_W),
            button_h,
        );
        place(self.byline, left, byline_y, wide, self.px(LINE));
        place(self.title, left, title_y, wide, self.px(TITLE_H));
        place(self.canvas, left, pad, wide, title_y - self.px(8) - pad);
    }

    /// Takes a new list, keeping what can be kept: the selection stays on the same
    /// painting where that painting is still there, and a thumbnail already made is
    /// never made twice.
    fn adopt(&self, cards: Vec<Card>) {
        let was = self.selected_key();
        let cell = self.px(THUMB);

        self.quiet.set(true);
        {
            let mut shown = self.shown.borrow_mut();
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
        self.refill_list();
        self.quiet.set(false);

        let row = {
            let shown = self.shown.borrow();
            was.and_then(|key| shown.cards.iter().position(|card| card.key == key))
                .or_else(|| (!shown.cards.is_empty()).then_some(0))
        };
        self.choose(row, true);
    }

    /// Rebuilds the control's contents from `shown`: a fresh image list, in card
    /// order, and one item per card pointing into it.
    ///
    /// The image list is rebuilt whole rather than patched, because an item names
    /// its picture by index and an index is only meaningful for as long as nothing
    /// before it is removed.
    fn refill_list(&self) {
        let mut shown = self.shown.borrow_mut();
        let cell = shown.cell;
        // SAFETY: every handle is ours or the list control's; the old image list is
        // destroyed only after the control has stopped using it, and the control was
        // made with LVS_SHAREIMAGELISTS so it never destroys ours behind our back.
        unsafe {
            send(self.list, LVM_DELETEALLITEMS, 0, 0);
            let images =
                ImageList_Create(cell, cell, ILC_COLOR24, shown.cards.len().max(1) as i32, 4);
            send(
                self.list,
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
                self.list,
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
                send(
                    self.list,
                    LVM_INSERTITEMW,
                    0,
                    &item as *const LVITEMW as isize,
                );
            }
        }
    }

    /// Picks out a row: lights it, if `light` says the control does not already
    /// have it lit, and fills the pane beside it.
    fn choose(&self, row: Option<usize>, light: bool) {
        self.selected.set(row);
        if let (true, Some(row)) = (light, row) {
            let state = LIST_VIEW_ITEM_STATE_FLAGS(LVIS_SELECTED.0 | LVIS_FOCUSED.0);
            let item = LVITEMW {
                stateMask: state,
                state,
                ..LVITEMW::default()
            };
            self.quiet.set(true);
            // `item` outlives the call, which does not keep the pointer.
            send(
                self.list,
                LVM_SETITEMSTATE,
                row,
                &item as *const LVITEMW as isize,
            );
            send(self.list, LVM_ENSUREVISIBLE, row, 0);
            self.quiet.set(false);
        }
        let art = row.and_then(|row| {
            self.shown
                .borrow()
                .cards
                .get(row)
                .map(|card| card.art.clone())
        });
        self.point_at(art.as_ref());
    }

    /// Points the pane at a picture, or empties it when there is none.
    fn point_at(&self, art: Option<&Artwork>) {
        match art {
            Some(art) => {
                *self.preview.borrow_mut() = self.preview_of(&art.path);
                set_text(self.title, &art.title);
                set_text(
                    self.byline,
                    if art.byline.is_empty() {
                        &art.attribution
                    } else {
                        &art.byline
                    },
                );
                enable(self.show, true);
                enable(self.forget, true);
            }
            None => {
                *self.preview.borrow_mut() = None;
                set_text(self.title, "Nothing kept yet");
                set_text(
                    self.byline,
                    "Add to favourites keeps the picture on the desktop",
                );
                enable(self.show, false);
                enable(self.forget, false);
            }
        }
        // SAFETY: repaints one of our own children.
        let _ = unsafe { InvalidateRect(Some(self.canvas), None, false) };
    }

    fn preview_of(&self, path: &Path) -> Option<Bitmap> {
        let mut area = RECT::default();
        // SAFETY: `area` is a RECT.
        let _ = unsafe { GetClientRect(self.canvas, &mut area) };
        shell_image(
            path,
            area.right.max(PREVIEW_AT_LEAST),
            area.bottom.max(PREVIEW_AT_LEAST),
        )
    }

    /// The key of the picture in the pane, if there is one.
    fn selected_key(&self) -> Option<String> {
        let row = self.selected.get()?;
        let shown = self.shown.borrow();
        shown.cards.get(row).map(|card| card.key.clone())
    }

    /// Says what was asked of the picture in the pane.
    ///
    /// By key, and read out before anybody is told, because answering this will take
    /// the list apart underneath us — and no borrow is held while they are.
    fn ask(&self, what: fn(String) -> Pick) {
        let Some(key) = self.selected_key() else {
            return;
        };
        (self.on_pick)(what(key));
    }

    /// Draws the picture, and black wherever it does not reach.
    fn paint_canvas(&self, item: &DRAWITEMSTRUCT) {
        let area = item.rcItem;
        let picture = self
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

    fn on_item_changed(&self, change: &NMLISTVIEW) {
        let lit = LVIS_SELECTED.0;
        let newly_selected = change.uNewState & lit != 0 && change.uOldState & lit == 0;
        if self.quiet.get() || !newly_selected || change.iItem < 0 {
            return;
        }
        let row = change.iItem as usize;
        if self.selected.get() != Some(row) {
            self.choose(Some(row), false);
        }
    }

    fn on_double_click(&self, click: &NMITEMACTIVATE) {
        // A double-click on a picture is impatience, and means the button beside it.
        // The first click of the pair has already chosen it; one on bare ground
        // names nothing.
        if click.iItem >= 0 {
            self.ask(Pick::Show);
        }
    }
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
            if let Some(images) = self.shown.borrow_mut().images.take() {
                let _ = ImageList_Destroy(Some(images));
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
        WM_DRAWITEM if wparam.0 as i32 == CANVAS_ID => {
            // SAFETY: for WM_DRAWITEM, `lparam` points to a DRAWITEMSTRUCT.
            inner.paint_canvas(unsafe { &*(lparam.0 as *const DRAWITEMSTRUCT) });
            return LRESULT(1);
        }
        WM_NOTIFY => {
            // SAFETY: for WM_NOTIFY, `lparam` points to an NMHDR, and to the larger
            // structure its `code` names.
            let header = unsafe { &*(lparam.0 as *const NMHDR) };
            if header.hwndFrom == inner.list {
                match header.code {
                    LVN_ITEMCHANGED => {
                        inner.on_item_changed(unsafe { &*(lparam.0 as *const NMLISTVIEW) })
                    }
                    NM_DBLCLK => {
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
        .map_err(|e| anyhow!("building the favourites list: {e}"))?;
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
            CANVAS_ID,
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
        let button = |label: PCWSTR, id: i32| {
            child(
                parent,
                w!("BUTTON"),
                label,
                WS_TABSTOP.0 | BS_PUSHBUTTON as u32,
                id,
                fonts.body,
            )
        };
        let show = button(w!("Set as wallpaper"), SHOW_ID)?;
        let forget = button(w!("Forget"), FORGET_ID)?;

        let inner = Rc::new(Inner {
            parent,
            list,
            canvas,
            title,
            byline,
            show,
            forget,
            fonts,
            shown: RefCell::new(Shown {
                cards: Vec::new(),
                thumbs: HashMap::new(),
                cell: 0,
                images: None,
            }),
            selected: Cell::new(None),
            preview: RefCell::new(None),
            quiet: Cell::new(false),
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

        inner.layout();
        inner.point_at(None);
        Ok(Self { inner })
    }

    pub fn relist(&self, favourites: &Favourites) {
        self.inner.adopt(
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
