use super::{ArtistRow, Chip, Control, Pending, Pick, Snapshot, StyleKind, Tab};
use crate::art::Artwork;
use crate::favourites::Favourites;
use crate::settings::{BlurVariant, Border, Region, Shape, Subject};
use anyhow::Result;
use gdk::prelude::GdkPixbufExt;
use gdk_pixbuf::{Colorspace, Pixbuf};
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use tao::platform::unix::WindowExtUnix;
use tao::window::Window;

const THUMBNAIL: i32 = 180;
const PREVIEW_WIDTH: i32 = 1100;
const PREVIEW_HEIGHT: i32 = 760;

pub(super) fn present(window: &Window) {
    window.gtk_window().present();
}

pub(super) fn close(window: &Window) -> bool {
    window.set_minimized(true);
    true
}

/// One row on a shelf. Everything a shelf varies by — what the primary button
/// says and whether it may be pressed — is carried here, so [`Browser`] itself
/// never learns whether it is showing favourites or painters.
#[derive(Clone)]
struct Card {
    /// What the selection is remembered by across a rebuild, and what the actions
    /// are handed back: the favourite's key, the painter's name.
    key: String,
    art: Artwork,
    /// Drawn before the name on the shelf.
    marked: bool,
    primary: String,
    /// Why the primary button cannot be pressed for this card.
    blocked: Option<String>,
    can_secondary: bool,
}

/// A shelf of thumbnails beside a large preview and two buttons: the whole of
/// the favourites page, and the artist browser in the settings tab. What the
/// buttons do is wired by the owner, because that differs and the layout does not.
struct Browser {
    split: gtk::Paned,
    list: gtk::ListBox,
    scroll: gtk::ScrolledWindow,
    cards: Rc<RefCell<Vec<Card>>>,
    /// By key and never by position, so a list rebuilt around a deletion cannot
    /// pair a card with somebody else's picture.
    thumbs: RefCell<HashMap<String, Pixbuf>>,
    primary: gtk::Button,
    secondary: gtk::Button,
}

impl Browser {
    /// `empty` is the title and byline shown while the shelf has nothing on it.
    fn new(primary: &str, secondary: &str, empty: (&'static str, &'static str)) -> Rc<Self> {
        let split = gtk::Paned::new(gtk::Orientation::Horizontal);
        split.set_position(240);
        split.set_wide_handle(true);

        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::Single);
        list.set_activate_on_single_click(false);
        let scroll = gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_min_content_width(220);
        scroll.add(&list);
        split.add1(&scroll);

        let detail = gtk::Box::new(gtk::Orientation::Vertical, 8);
        detail.set_margin_start(12);
        let canvas = gtk::Image::new();
        canvas.set_hexpand(true);
        canvas.set_vexpand(true);
        let title = gtk::Label::new(Some(empty.0));
        title.set_xalign(0.0);
        title.set_selectable(true);
        let byline = gtk::Label::new(Some(empty.1));
        byline.set_xalign(0.0);
        byline.set_selectable(true);
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let primary = gtk::Button::with_label(primary);
        let secondary = gtk::Button::with_label(secondary);
        primary.set_sensitive(false);
        secondary.set_sensitive(false);
        actions.pack_start(&primary, false, false, 0);
        actions.pack_start(&secondary, false, false, 0);
        detail.pack_start(&canvas, true, true, 0);
        detail.pack_start(&title, false, false, 0);
        detail.pack_start(&byline, false, false, 0);
        detail.pack_start(&actions, false, false, 0);
        split.add2(&detail);

        let cards = Rc::new(RefCell::new(Vec::<Card>::new()));
        let selected_cards = cards.clone();
        let (selected_primary, selected_secondary) = (primary.clone(), secondary.clone());
        list.connect_row_selected(move |_, row| {
            let card =
                row.and_then(|row| selected_cards.borrow().get(row.index() as usize).cloned());
            match card {
                Some(card) => {
                    match Pixbuf::from_file_at_scale(
                        &card.art.path,
                        PREVIEW_WIDTH,
                        PREVIEW_HEIGHT,
                        true,
                    ) {
                        Ok(preview) => canvas.set_from_pixbuf(Some(&preview)),
                        Err(_) => canvas.clear(),
                    }
                    title.set_text(&card.art.title);
                    byline.set_text(if card.art.byline.is_empty() {
                        &card.art.attribution
                    } else {
                        &card.art.byline
                    });
                    selected_primary.set_label(&card.primary);
                    selected_primary.set_sensitive(card.blocked.is_none());
                    selected_primary.set_tooltip_text(card.blocked.as_deref());
                    selected_secondary.set_sensitive(card.can_secondary);
                }
                None => {
                    canvas.clear();
                    title.set_text(empty.0);
                    byline.set_text(empty.1);
                    selected_primary.set_sensitive(false);
                    selected_primary.set_tooltip_text(None);
                    selected_secondary.set_sensitive(false);
                }
            }
        });

        Rc::new(Self {
            split,
            list,
            scroll,
            cards,
            thumbs: RefCell::new(HashMap::new()),
            primary,
            secondary,
        })
    }

    /// Says what the buttons and a double-click do. Each is handed a copy of the
    /// selected card, because an action may well rebuild the shelf it came from.
    fn wire(
        self: &Rc<Self>,
        primary: Rc<dyn Fn(&Card)>,
        secondary: Rc<dyn Fn(&Card)>,
        activate: Rc<dyn Fn(&Card)>,
    ) {
        for (button, action) in [(&self.primary, primary), (&self.secondary, secondary)] {
            let weak = Rc::downgrade(self);
            button.connect_clicked(move |_| {
                if let Some(card) = weak.upgrade().and_then(|browser| browser.selected_card()) {
                    action(&card);
                }
            });
        }
        let weak = Rc::downgrade(self);
        self.list.connect_row_activated(move |_, row| {
            let card = weak
                .upgrade()
                .and_then(|browser| browser.cards.borrow().get(row.index() as usize).cloned());
            if let Some(card) = card {
                activate(&card);
            }
        });
    }

    fn selected_card(&self) -> Option<Card> {
        let row = self.list.selected_row()?;
        self.cards.borrow().get(row.index() as usize).cloned()
    }

    fn selected_key(&self) -> Option<String> {
        self.selected_card().map(|card| card.key)
    }

    /// Replaces the shelf. `want` is the key to leave selected, else the first
    /// card is. The scroll position is kept when `keep_scroll`, and otherwise the
    /// selected row is brought into view; either has to wait for the new rows to
    /// be allocated.
    fn set_cards(&self, cards: Vec<Card>, want: Option<String>, keep_scroll: bool) {
        let adjustment = self.scroll.vadjustment();
        let scrolled = adjustment.value();

        self.thumbs
            .borrow_mut()
            .retain(|key, _| cards.iter().any(|card| &card.key == key));
        for card in &cards {
            if !self.thumbs.borrow().contains_key(&card.key) {
                if let Ok(thumbnail) =
                    Pixbuf::from_file_at_scale(&card.art.path, THUMBNAIL, THUMBNAIL, true)
                {
                    self.thumbs.borrow_mut().insert(card.key.clone(), thumbnail);
                }
            }
        }

        for child in self.list.children() {
            self.list.remove(&child);
        }
        *self.cards.borrow_mut() = cards;
        for card in self.cards.borrow().iter() {
            let row = gtk::ListBoxRow::new();
            let content = gtk::Box::new(gtk::Orientation::Vertical, 4);
            content.set_margin_top(8);
            content.set_margin_bottom(8);
            content.set_margin_start(8);
            content.set_margin_end(8);
            let image = match self.thumbs.borrow().get(&card.key) {
                Some(thumbnail) => gtk::Image::from_pixbuf(Some(thumbnail)),
                None => gtk::Image::new(),
            };
            let name = if card.marked {
                format!("\u{2713} {}", card.art.title)
            } else {
                card.art.title.clone()
            };
            let label = gtk::Label::new(Some(&name));
            label.set_line_wrap(true);
            label.set_max_width_chars(24);
            content.pack_start(&image, false, false, 0);
            content.pack_start(&label, false, false, 0);
            row.add(&content);
            self.list.add(&row);
        }
        self.list.show_all();

        let row = want
            .and_then(|key| self.cards.borrow().iter().position(|card| card.key == key))
            .or_else(|| (!self.cards.borrow().is_empty()).then_some(0))
            .and_then(|index| self.list.row_at_index(index as i32));
        match row {
            Some(row) => self.list.select_row(Some(&row)),
            None => self.list.unselect_all(),
        }

        let list = self.list.clone();
        glib::idle_add_local_once(move || {
            let top = if keep_scroll {
                scrolled
            } else {
                list.selected_row()
                    .map_or(0.0, |row| row.allocation().y() as f64)
            };
            adjustment.set_value(top);
        });
    }
}

/// The GTK half of the one Linux window: daily controls above an accessible
/// favourites list and preview.
pub(super) struct Content {
    title: gtk::Label,
    byline: gtk::Label,
    open: gtk::Button,
    next: gtk::Button,
    keep: gtk::Button,
    today: gtk::Button,
    reapply: gtk::Button,
    login: gtk::CheckButton,
    stack: gtk::Stack,
    settings: Rc<SettingsView>,
    favourites: Rc<Browser>,
    updating_login: Rc<Cell<bool>>,
}

impl Content {
    pub(super) fn install(
        window: &Window,
        on_pick: Rc<dyn Fn(Pick)>,
        on_control: Rc<dyn Fn(Control)>,
    ) -> Result<Self> {
        let root = window
            .default_vbox()
            .expect("tao creates its GTK content box by default");
        root.set_spacing(10);
        root.set_margin_top(16);
        root.set_margin_bottom(16);
        root.set_margin_start(16);
        root.set_margin_end(16);

        // Two pages behind a switcher. Everything below up to the stack is the
        // favourites page, which is why `root` is shadowed rather than renamed.
        let outer = root;
        let root = gtk::Box::new(gtk::Orientation::Vertical, 10);

        let title = gtk::Label::new(None);
        title.set_xalign(0.0);
        title.set_selectable(true);
        let byline = gtk::Label::new(None);
        byline.set_xalign(0.0);
        byline.set_selectable(true);
        root.pack_start(&title, false, false, 0);
        root.pack_start(&byline, false, false, 0);

        let controls = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let open = gtk::Button::with_label("Open in browser");
        let next = gtk::Button::with_label("Next picture");
        let keep = gtk::Button::with_label("Add to favourites");
        let today = gtk::Button::with_label("Back to today's picture");
        let reapply = gtk::Button::with_label("Re-apply wallpaper");
        for button in [&open, &next, &keep, &today, &reapply] {
            controls.pack_start(button, false, false, 0);
        }
        root.pack_start(&controls, false, false, 0);

        let options = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let login = gtk::CheckButton::with_label("Start at login");
        let quit = gtk::Button::with_label("Quit Art Window");
        options.pack_start(&login, false, false, 0);
        options.pack_end(&quit, false, false, 0);
        root.pack_start(&options, false, false, 0);
        root.pack_start(
            &gtk::Separator::new(gtk::Orientation::Horizontal),
            false,
            false,
            0,
        );

        let favourites = Browser::new(
            "Set as wallpaper",
            "Forget",
            (
                "Nothing kept yet",
                "Add to favourites keeps the picture on the desktop",
            ),
        );
        root.pack_start(&favourites.split, true, true, 0);

        connect_control(&open, on_control.clone(), || Control::Browse);
        connect_control(&next, on_control.clone(), || Control::Next);
        connect_control(&keep, on_control.clone(), || Control::Keep);
        connect_control(&today, on_control.clone(), || Control::Today);
        connect_control(&reapply, on_control.clone(), || Control::Reapply);
        connect_control(&quit, on_control.clone(), || Control::Quit);

        let updating_login = Rc::new(Cell::new(false));
        let changing = updating_login.clone();
        let login_control = on_control.clone();
        login.connect_toggled(move |button| {
            if !changing.get() {
                login_control(Control::Login(button.is_active()));
            }
        });

        let (show_pick, forget_pick, activate_pick) =
            (on_pick.clone(), on_pick.clone(), on_pick.clone());
        favourites.wire(
            Rc::new(move |card: &Card| show_pick(Pick::Show(card.key.clone()))),
            Rc::new(move |card: &Card| forget_pick(Pick::Forget(card.key.clone()))),
            Rc::new(move |card: &Card| activate_pick(Pick::Show(card.key.clone()))),
        );

        let settings = SettingsView::build(on_pick);
        let stack = gtk::Stack::new();
        stack.add_titled(&root, "favourites", "Favourites");
        stack.add_titled(&settings.page, "settings", "Settings");
        // Leaving the settings tab closes the artist browser, so coming back lands
        // on the rows rather than in the middle of a choice made last time.
        let leaving = Rc::downgrade(&settings);
        stack.connect_visible_child_notify(move |stack| {
            if stack.visible_child_name().as_deref() != Some("settings") {
                if let Some(settings) = leaving.upgrade() {
                    settings.show_rows();
                }
            }
        });
        let switcher = gtk::StackSwitcher::new();
        switcher.set_stack(Some(&stack));
        switcher.set_halign(gtk::Align::Center);
        outer.pack_start(&switcher, false, false, 0);
        outer.pack_start(&stack, true, true, 0);
        outer.show_all();
        Ok(Self {
            title,
            byline,
            open,
            next,
            keep,
            today,
            reapply,
            login,
            stack,
            settings,
            list,
            cards,
            thumbs: Rc::new(RefCell::new(HashMap::new())),
            updating_login,
        })
    }

    pub(super) fn show_tab(&self, tab: Tab) {
        if tab == Tab::Settings {
            self.settings.show_rows();
        }
        self.stack.set_visible_child_name(match tab {
            Tab::Favourites => "favourites",
            Tab::Settings => "settings",
        });
    }

    pub(super) fn describe(&self, snapshot: &Snapshot, favourites: &Favourites) {
        self.describe_status(snapshot);
        self.set_login(snapshot.starts_at_login);
        self.settings.describe(snapshot);
        self.relist(favourites);
    }

    pub(super) fn describe_status(&self, snapshot: &Snapshot) {
        match &snapshot.shown {
            Some(art) => {
                self.title.set_text(&art.title);
                self.byline
                    .set_text(snapshot.status.as_deref().unwrap_or_else(|| {
                        if art.byline.is_empty() {
                            &art.attribution
                        } else {
                            &art.byline
                        }
                    }));
                self.open.set_sensitive(art.details_url.is_some());
                self.reapply.set_sensitive(true);
            }
            None => {
                self.title.set_text("No picture yet");
                self.byline.set_text(
                    snapshot
                        .status
                        .as_deref()
                        .unwrap_or("Waiting for the first one"),
                );
                self.open.set_sensitive(false);
                self.reapply.set_sensitive(false);
            }
        }
        self.next.set_sensitive(!snapshot.fetching);
        self.keep.set_sensitive(snapshot.can_keep);
        self.today.set_sensitive(snapshot.today.is_some());
    }

    pub(super) fn set_login(&self, enabled: bool) {
        self.updating_login.set(true);
        self.login.set_active(enabled);
        self.updating_login.set(false);
    }

    pub(super) fn relist(&self, favourites: &Favourites) {
        let cards = favourites
            .iter()
            .map(|(key, art)| Card {
                key: key.to_string(),
                art: art.clone(),
                marked: false,
                primary: "Set as wallpaper".to_owned(),
                blocked: None,
                can_secondary: true,
            })
            .collect();
        self.favourites
            .set_cards(cards, self.favourites.selected_key(), true);
    }
}

fn connect_control(button: &gtk::Button, on_control: Rc<dyn Fn(Control)>, action: fn() -> Control) {
    button.connect_clicked(move |_| on_control(action()));
}

/// The preview card's width, in logical pixels.
const SETTINGS_PREVIEW: i32 = 300;

const SETTINGS_CSS: &str = "
.aw-page, .aw-page viewport { background-color: #ffffff; color: #1d1d1f; }
.aw-section { font-size: 13px; font-weight: 600; color: #6e6e73; }
.aw-note, .aw-secondary { font-size: 13px; color: #6e6e73; }
.aw-label { font-size: 13px; color: #1d1d1f; }
.aw-previewcard { border-radius: 12px; background-color: #eef0f3;
  box-shadow: 0 10px 30px rgba(0, 0, 0, 0.08); }
button.aw-chip, button.aw-card {
  background-image: none; background-color: #eef0f3; color: #1d1d1f;
  border: none; box-shadow: none; text-shadow: none; }
button.aw-chip { border-radius: 15px; min-height: 30px; padding: 0 14px;
  font-size: 13px; font-weight: 500; }
button.aw-card { border-radius: 10px; min-width: 78px; min-height: 56px;
  font-size: 13px; font-weight: 500; }
button.aw-chip:hover, button.aw-card:hover { background-color: #e3e5e9; }
button.aw-chip-selected, button.aw-card-selected,
button.aw-chip-selected:hover, button.aw-card-selected:hover {
  background-color: #1d1d1f; color: #ffffff; }
button.aw-apply { background-image: none; background-color: #1a73e8; color: #ffffff;
  border: none; box-shadow: none; text-shadow: none; border-radius: 18px;
  min-height: 36px; padding: 0 20px; font-size: 15px; font-weight: 600; }
button.aw-apply:disabled { opacity: 0.4; }
";

fn add_class(widget: &impl IsA<gtk::Widget>, class: &str) {
    widget.style_context().add_class(class);
}

fn set_class(widget: &impl IsA<gtk::Widget>, class: &str, on: bool) {
    let context = widget.style_context();
    if on {
        context.add_class(class);
    } else {
        context.remove_class(class);
    }
}

fn clear(container: &impl IsA<gtk::Container>) {
    for child in container.children() {
        container.remove(&child);
    }
}

fn section_title(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.set_margin_top(24);
    label.set_margin_bottom(10);
    add_class(&label, "aw-section");
    label
}

fn chip_button(label: &str, selected: bool) -> gtk::Button {
    let button = gtk::Button::with_label(label);
    add_class(&button, "aw-chip");
    set_class(&button, "aw-chip-selected", selected);
    button
}

fn chip_flow() -> gtk::FlowBox {
    let flow = gtk::FlowBox::new();
    flow.set_selection_mode(gtk::SelectionMode::None);
    flow.set_homogeneous(false);
    flow.set_column_spacing(8);
    flow.set_row_spacing(8);
    flow.set_min_children_per_line(1);
    flow.set_max_children_per_line(64);
    flow.set_halign(gtk::Align::Fill);
    flow
}

/// What a change needs redrawn. A slider or colour well is dragged through many
/// values, and rebuilding the control being dragged would end the drag, so those
/// redraw only what depends on the value.
#[derive(Clone, Copy)]
enum Redraw {
    Light,
    All,
}

/// The settings tab. What is chosen, and what follows from it, is [`Pending`]'s;
/// this draws it and forwards clicks into it. Chip rows and the options under the
/// selected style are rebuilt fresh from the model on every change, so no widget
/// ever has to be told its state programmatically — the two exceptions, the switch
/// and the style cards, are guarded or restyled rather than rebuilt.
struct SettingsView {
    page: gtk::Box,
    pending: RefCell<Pending>,
    /// Set while the switch is being moved by the model rather than by a person.
    updating: Cell<bool>,
    on_pick: Rc<dyn Fn(Pick)>,
    cards: Vec<(StyleKind, gtk::Button)>,
    style_options: gtk::Box,
    filters: gtk::Box,
    shapes: gtk::FlowBox,
    regions: gtk::FlowBox,
    subjects: gtk::FlowBox,
    artists: gtk::FlowBox,
    /// The settings sections ("rows") or the artist browser, with the bar below
    /// them staying put in both.
    inner: gtk::Stack,
    back: gtk::Button,
    browser: Rc<Browser>,
    religious: gtk::Switch,
    canvas: gtk::DrawingArea,
    empty: gtk::Label,
    surface: Rc<RefCell<Option<gtk::cairo::Surface>>>,
    note: gtk::Label,
    apply: gtk::Button,
}

impl SettingsView {
    fn build(on_pick: Rc<dyn Fn(Pick)>) -> Rc<Self> {
        if let (Some(screen), provider) = (gdk::Screen::default(), gtk::CssProvider::new()) {
            if provider.load_from_data(SETTINGS_CSS.as_bytes()).is_ok() {
                gtk::StyleContext::add_provider_for_screen(
                    &screen,
                    &provider,
                    gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
                );
            }
        }

        let page = gtk::Box::new(gtk::Orientation::Vertical, 0);
        add_class(&page, "aw-page");
        page.set_margin_top(8);
        page.set_margin_bottom(8);
        page.set_margin_start(8);
        page.set_margin_end(8);

        // Left: the preview card. The margin leaves room for the shadow to be seen.
        let rows = gtk::Box::new(gtk::Orientation::Horizontal, 24);
        let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
        add_class(&card, "aw-previewcard");
        card.set_valign(gtk::Align::Start);
        card.set_margin_top(12);
        card.set_margin_bottom(30);
        card.set_margin_start(12);
        card.set_margin_end(12);
        let canvas = gtk::DrawingArea::new();
        canvas.set_size_request(SETTINGS_PREVIEW, SETTINGS_PREVIEW * 10 / 16);
        let empty = gtk::Label::new(Some("No picture yet"));
        add_class(&empty, "aw-secondary");
        let overlay = gtk::Overlay::new();
        overlay.add(&canvas);
        overlay.add_overlay(&empty);
        overlay.set_overlay_pass_through(&empty, true);
        card.pack_start(&overlay, false, false, 0);
        rows.pack_start(&card, false, false, 0);

        let surface = Rc::new(RefCell::new(None::<gtk::cairo::Surface>));
        let drawn = surface.clone();
        canvas.connect_draw(move |area, cr| {
            use std::f64::consts::{FRAC_PI_2, PI};
            let (w, h, r) = (
                area.allocated_width() as f64,
                area.allocated_height() as f64,
                12.0,
            );
            cr.new_sub_path();
            cr.arc(w - r, r, r, -FRAC_PI_2, 0.0);
            cr.arc(w - r, h - r, r, 0.0, FRAC_PI_2);
            cr.arc(r, h - r, r, FRAC_PI_2, PI);
            cr.arc(r, r, r, PI, 3.0 * FRAC_PI_2);
            cr.close_path();
            cr.clip();
            cr.set_source_rgb(
                0xee as f64 / 255.0,
                0xf0 as f64 / 255.0,
                0xf3 as f64 / 255.0,
            );
            let _ = cr.paint();
            if let Some(surface) = drawn.borrow().as_ref() {
                if cr.set_source_surface(surface, 0.0, 0.0).is_ok() {
                    let _ = cr.paint();
                }
            }
            glib::Propagation::Proceed
        });

        // Right: sections that scroll. The bar that does not is further down.
        let scroll = gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_shadow_type(gtk::ShadowType::None);
        scroll.set_hexpand(true);
        scroll.set_vexpand(true);
        let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
        column.set_margin_end(12);
        scroll.add(&column);
        rows.pack_start(&scroll, true, true, 0);

        column.pack_start(&section_title("Style"), false, false, 0);
        let card_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let cards: Vec<(StyleKind, gtk::Button)> = StyleKind::ALL
            .into_iter()
            .map(|kind| {
                let button = gtk::Button::with_label(kind.label());
                add_class(&button, "aw-card");
                card_row.pack_start(&button, false, false, 0);
                (kind, button)
            })
            .collect();
        column.pack_start(&card_row, false, false, 0);
        let style_options = gtk::Box::new(gtk::Orientation::Vertical, 10);
        style_options.set_margin_top(12);
        column.pack_start(&style_options, false, false, 0);

        let filters = gtk::Box::new(gtk::Orientation::Vertical, 0);
        column.pack_start(&filters, false, false, 0);
        let flow_under = |title: &str| {
            let flow = chip_flow();
            filters.pack_start(&section_title(title), false, false, 0);
            filters.pack_start(&flow, false, false, 0);
            flow
        };
        let shapes = flow_under("Shape");
        let regions = flow_under("Origin");
        let subjects = flow_under("Subject");
        let artists = flow_under("Artist");

        let religious_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        religious_row.set_margin_top(24);
        religious_row.set_margin_bottom(24);
        let religious_label = gtk::Label::new(Some("Hide religious scenes"));
        add_class(&religious_label, "aw-label");
        let religious = gtk::Switch::new();
        religious.set_valign(gtk::Align::Center);
        religious_row.pack_start(&religious_label, false, false, 0);
        religious_row.pack_end(&religious, false, false, 0);
        filters.pack_start(&religious_row, false, false, 0);

        let bar = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        bar.set_margin_top(12);
        let note = gtk::Label::new(None);
        note.set_xalign(1.0);
        note.set_line_wrap(true);
        note.set_hexpand(true);
        add_class(&note, "aw-note");
        let apply = gtk::Button::with_label("Apply changes");
        add_class(&apply, "aw-apply");
        // Only while the artist browser is up; `set_visible` alone would be undone
        // by the `show_all` that opens the window.
        let back = gtk::Button::with_label("Back");
        back.set_no_show_all(true);
        back.set_visible(false);
        bar.pack_start(&back, false, false, 0);
        bar.pack_start(&note, true, true, 0);
        bar.pack_end(&apply, false, false, 0);

        let browser = Browser::new("Choose", "Read more", ("No painters", ""));

        let inner = gtk::Stack::new();
        inner.add_named(&rows, "rows");
        inner.add_named(&browser.split, "browser");
        page.pack_start(&inner, true, true, 0);
        page.pack_start(&bar, false, false, 0);

        let view = Rc::new(Self {
            page,
            pending: RefCell::new(Pending::new(
                crate::settings::Settings::default(),
                true,
                16.0 / 10.0,
            )),
            updating: Cell::new(false),
            on_pick,
            cards,
            style_options,
            filters,
            shapes,
            regions,
            subjects,
            artists,
            inner,
            back,
            browser,
            religious,
            canvas,
            empty,
            surface,
            note,
            apply,
        });
        view.connect();
        let weak = Rc::downgrade(&view);
        view.back.connect_clicked(move |_| {
            if let Some(view) = weak.upgrade() {
                view.show_rows();
            }
        });
        view.redraw(Redraw::All);
        view
    }

    /// Wires the widgets that outlive a redraw. Handlers hold the view weakly, so
    /// the widgets, which the view owns, do not keep it alive in a cycle.
    fn connect(self: &Rc<Self>) {
        for (kind, button) in &self.cards {
            let (weak, kind) = (Rc::downgrade(self), *kind);
            button.connect_clicked(move |_| {
                if let Some(view) = weak.upgrade() {
                    view.pending.borrow_mut().set_style(kind);
                    view.redraw(Redraw::All);
                }
            });
        }
        let weak = Rc::downgrade(self);
        self.religious.connect_active_notify(move |switch| {
            if let Some(view) = weak.upgrade() {
                if !view.updating.get() {
                    view.pending
                        .borrow_mut()
                        .set_hide_religious(switch.is_active());
                    view.redraw(Redraw::All);
                }
            }
        });

        let weak = Rc::downgrade(self);
        let choose: Rc<dyn Fn(&Card)> = Rc::new(move |card: &Card| {
            if let Some(view) = weak.upgrade() {
                view.pending.borrow_mut().toggle_artist(&card.key);
                view.redraw(Redraw::All);
            }
        });
        let (activate, on_pick) = (choose.clone(), self.on_pick.clone());
        self.browser.wire(
            choose,
            Rc::new(move |card: &Card| {
                if let Some(url) = &card.art.details_url {
                    on_pick(Pick::Read(url.clone()));
                }
            }),
            Rc::new(move |card: &Card| {
                if card.blocked.is_none() {
                    activate(card);
                }
            }),
        );

        let weak = Rc::downgrade(self);
        self.apply.connect_clicked(move |_| {
            if let Some(view) = weak.upgrade() {
                let staged = view.pending.borrow().staged().clone();
                (view.on_pick)(Pick::Apply(staged));
            }
        });
    }

    fn describe(self: &Rc<Self>, snapshot: &Snapshot) {
        {
            let mut pending = self.pending.borrow_mut();
            pending.keep_pictures_in(&snapshot.pictures);
            pending.adopt(&snapshot.settings, snapshot.filters_apply, snapshot.aspect);
            pending.set_picture(snapshot.shown.as_ref().map(|art| art.path.as_path()));
        }
        self.redraw(Redraw::All);
    }

    fn redraw(self: &Rc<Self>, what: Redraw) {
        self.redraw_preview();
        let pending = self.pending.borrow();
        let kind = pending.style_kind();
        for (card, button) in &self.cards {
            set_class(button, "aw-card-selected", *card == kind);
        }
        self.apply.set_sensitive(pending.can_apply());
        self.note.set_text(&pending.note().unwrap_or_default());
        if matches!(what, Redraw::Light) {
            return;
        }

        self.filters.set_sensitive(pending.filters_apply());
        self.updating.set(true);
        self.religious.set_active(pending.hide_religious());
        self.updating.set(false);
        let (border, (variant, strength), custom) =
            (pending.border(), pending.blur(), pending.custom_colour());
        let (shapes, regions, subjects, artists) = (
            pending.shapes(),
            pending.regions(),
            pending.subjects(),
            pending.artist_row(),
        );
        drop(pending);

        self.rebuild_style_options(kind, border, variant, strength, custom);
        self.fill(&self.shapes, shapes, |p, v: &Shape| p.set_shape(*v));
        self.fill(&self.regions, regions, |p, v: &Region| p.toggle_region(*v));
        self.fill(&self.subjects, subjects, |p, v: &Subject| {
            p.toggle_subject(*v)
        });
        self.fill_artists(artists);
        // Choosing a painter changes the mark on the shelf and the button under the
        // picture, so a shelf that is being looked at is rebuilt like the chips.
        if self.inner.visible_child_name().as_deref() == Some("browser") {
            self.browser
                .set_cards(self.artist_cards(), self.browser.selected_key(), true);
        }
    }

    /// The chosen painters as chips, then the button that opens the browser.
    fn fill_artists(self: &Rc<Self>, row: ArtistRow) {
        self.fill(&self.artists, row.chosen, |p, v: &String| {
            p.toggle_artist(v)
        });
        let browse = chip_button(row.browse, false);
        browse.set_sensitive(row.disabled_reason.is_none());
        browse.set_tooltip_text(row.disabled_reason.as_deref());
        let weak = Rc::downgrade(self);
        browse.connect_clicked(move |_| {
            if let Some(view) = weak.upgrade() {
                view.open_browser();
            }
        });
        self.artists.insert(&browse, -1);
        self.artists.show_all();
    }

    fn artist_cards(&self) -> Vec<Card> {
        self.pending
            .borrow()
            .artist_cards()
            .into_iter()
            .map(|card| Card {
                key: card.art.title.clone(),
                marked: card.selected,
                primary: if card.selected { "Remove" } else { "Choose" }.to_owned(),
                blocked: card.disabled_reason,
                can_secondary: card.art.details_url.is_some(),
                art: card.art,
            })
            .collect()
    }

    /// Cards are built when the browser is first wanted, not when the window
    /// opens, because that is when their thumbnails get decoded.
    fn open_browser(&self) {
        let cards = self.artist_cards();
        let first_chosen = cards.iter().find(|card| card.marked).map(|c| c.key.clone());
        self.browser.set_cards(cards, first_chosen, false);
        self.inner.set_visible_child_name("browser");
        self.back.set_visible(true);
    }

    fn show_rows(&self) {
        self.inner.set_visible_child_name("rows");
        self.back.set_visible(false);
    }

    fn redraw_preview(&self) {
        let pending = self.pending.borrow();
        let scale = self.canvas.scale_factor().max(1);
        let height = (SETTINGS_PREVIEW as f64 / pending.aspect().max(0.1)).round() as i32;
        self.canvas
            .set_size_request(SETTINGS_PREVIEW, height.max(1));
        let surface = pending
            .preview((SETTINGS_PREVIEW * scale) as u32)
            .and_then(|image| {
                let (w, h) = image.dimensions();
                let pixbuf = Pixbuf::from_bytes(
                    &glib::Bytes::from_owned(image.into_raw()),
                    Colorspace::Rgb,
                    true,
                    8,
                    w as i32,
                    h as i32,
                    (w * 4) as i32,
                );
                pixbuf.create_surface(scale, None::<&gdk::Window>)
            });
        self.empty.set_visible(surface.is_none());
        *self.surface.borrow_mut() = surface;
        self.canvas.queue_draw();
    }

    /// Replaces a row of chips with the model's current ones. `pick` is what a
    /// click does to the model; every click then redraws everything, since a chip
    /// changes which other chips are on offer.
    fn fill<T: Clone + 'static>(
        self: &Rc<Self>,
        flow: &gtk::FlowBox,
        chips: Vec<Chip<T>>,
        pick: fn(&mut Pending, &T),
    ) {
        clear(flow);
        for chip in chips {
            let button = chip_button(&chip.label, chip.selected);
            button.set_sensitive(chip.disabled_reason.is_none());
            button.set_tooltip_text(chip.disabled_reason.as_deref());
            let (weak, value) = (Rc::downgrade(self), chip.value);
            button.connect_clicked(move |_| {
                if let Some(view) = weak.upgrade() {
                    pick(&mut view.pending.borrow_mut(), &value);
                    view.redraw(Redraw::All);
                }
            });
            flow.insert(&button, -1);
        }
        flow.show_all();
    }

    /// The options that belong to the selected style, and only those.
    fn rebuild_style_options(
        self: &Rc<Self>,
        kind: StyleKind,
        border: Border,
        variant: BlurVariant,
        strength: u8,
        custom: [u8; 3],
    ) {
        clear(&self.style_options);
        let flow = chip_flow();
        match kind {
            StyleKind::Borders => {
                let chips = vec![
                    Chip {
                        value: Border::Black,
                        label: "Black".to_owned(),
                        selected: border == Border::Black,
                        disabled_reason: None,
                    },
                    Chip {
                        value: Border::Auto,
                        label: "Automatic".to_owned(),
                        selected: border == Border::Auto,
                        disabled_reason: None,
                    },
                    Chip {
                        value: Border::Custom { rgb: custom },
                        label: "Custom".to_owned(),
                        selected: matches!(border, Border::Custom { .. }),
                        disabled_reason: None,
                    },
                ];
                self.fill(&flow, chips, |p, v: &Border| p.set_border(*v));
                self.style_options.pack_start(&flow, false, false, 0);
                if matches!(border, Border::Custom { .. }) {
                    let [r, g, b] = custom.map(|c| c as f64 / 255.0);
                    let well = gtk::ColorButton::new();
                    well.set_rgba(&gdk::RGBA::new(r, g, b, 1.0));
                    well.set_halign(gtk::Align::Start);
                    let weak = Rc::downgrade(self);
                    well.connect_color_set(move |well| {
                        if let Some(view) = weak.upgrade() {
                            let c = well.rgba();
                            let byte = |v: f64| (v * 255.0).round().clamp(0.0, 255.0) as u8;
                            let rgb = [byte(c.red()), byte(c.green()), byte(c.blue())];
                            view.pending.borrow_mut().set_border(Border::Custom { rgb });
                            view.redraw(Redraw::Light);
                        }
                    });
                    self.style_options.pack_start(&well, false, false, 0);
                }
            }
            StyleKind::Blur => {
                let chips = vec![
                    Chip {
                        value: BlurVariant::Backdrop,
                        label: "Behind the picture".to_owned(),
                        selected: variant == BlurVariant::Backdrop,
                        disabled_reason: None,
                    },
                    Chip {
                        value: BlurVariant::WholeImage,
                        label: "Whole picture".to_owned(),
                        selected: variant == BlurVariant::WholeImage,
                        disabled_reason: None,
                    },
                ];
                self.fill(&flow, chips, |p, v: &BlurVariant| p.set_blur_variant(*v));
                self.style_options.pack_start(&flow, false, false, 0);

                let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
                let label = gtk::Label::new(Some("Strength"));
                add_class(&label, "aw-label");
                let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
                scale.set_draw_value(false);
                scale.set_size_request(240, -1);
                // Set before the handler exists, so that seeding it is not a change.
                scale.set_value(strength as f64);
                let weak = Rc::downgrade(self);
                scale.connect_value_changed(move |scale| {
                    if let Some(view) = weak.upgrade() {
                        let strength = scale.value().round().clamp(0.0, 100.0) as u8;
                        view.pending.borrow_mut().set_blur_strength(strength);
                        view.redraw(Redraw::Light);
                    }
                });
                row.pack_start(&label, false, false, 0);
                row.pack_start(&scale, false, false, 0);
                self.style_options.pack_start(&row, false, false, 0);
            }
            StyleKind::Zoom | StyleKind::Stretch => {}
        }
        self.style_options.show_all();
    }
}
