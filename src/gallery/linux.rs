use super::{Chip, Control, Pending, Pick, Snapshot, StyleKind, Tab};
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

#[derive(Clone)]
struct Card {
    key: String,
    art: Artwork,
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
    list: gtk::ListBox,
    cards: Rc<RefCell<Vec<Card>>>,
    thumbs: Rc<RefCell<HashMap<String, Pixbuf>>>,
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

        let split = gtk::Paned::new(gtk::Orientation::Horizontal);
        split.set_position(240);
        split.set_wide_handle(true);
        root.pack_start(&split, true, true, 0);

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
        let favourite_title = gtk::Label::new(Some("Nothing kept yet"));
        favourite_title.set_xalign(0.0);
        favourite_title.set_selectable(true);
        let favourite_byline =
            gtk::Label::new(Some("Add to favourites keeps the picture on the desktop"));
        favourite_byline.set_xalign(0.0);
        favourite_byline.set_selectable(true);
        let favourite_actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let show = gtk::Button::with_label("Set as wallpaper");
        let forget = gtk::Button::with_label("Forget");
        show.set_sensitive(false);
        forget.set_sensitive(false);
        favourite_actions.pack_start(&show, false, false, 0);
        favourite_actions.pack_start(&forget, false, false, 0);
        detail.pack_start(&canvas, true, true, 0);
        detail.pack_start(&favourite_title, false, false, 0);
        detail.pack_start(&favourite_byline, false, false, 0);
        detail.pack_start(&favourite_actions, false, false, 0);
        split.add2(&detail);

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

        let cards = Rc::new(RefCell::new(Vec::<Card>::new()));
        let selected_cards = cards.clone();
        let selected_canvas = canvas.clone();
        let selected_title = favourite_title.clone();
        let selected_byline = favourite_byline.clone();
        let selected_show = show.clone();
        let selected_forget = forget.clone();
        list.connect_row_selected(move |_, row| {
            let artwork = row
                .and_then(|row| selected_cards.borrow().get(row.index() as usize).cloned())
                .map(|card| card.art);
            match artwork {
                Some(art) => {
                    match Pixbuf::from_file_at_scale(&art.path, PREVIEW_WIDTH, PREVIEW_HEIGHT, true)
                    {
                        Ok(preview) => selected_canvas.set_from_pixbuf(Some(&preview)),
                        Err(_) => selected_canvas.clear(),
                    }
                    selected_title.set_text(&art.title);
                    selected_byline.set_text(if art.byline.is_empty() {
                        &art.attribution
                    } else {
                        &art.byline
                    });
                    selected_show.set_sensitive(true);
                    selected_forget.set_sensitive(true);
                }
                None => {
                    selected_canvas.clear();
                    selected_title.set_text("Nothing kept yet");
                    selected_byline.set_text("Add to favourites keeps the picture on the desktop");
                    selected_show.set_sensitive(false);
                    selected_forget.set_sensitive(false);
                }
            }
        });

        let activated_cards = cards.clone();
        let activated_pick = on_pick.clone();
        list.connect_row_activated(move |_, row| {
            if let Some(card) = activated_cards.borrow().get(row.index() as usize) {
                activated_pick(Pick::Show(card.key.clone()));
            }
        });

        connect_pick(&show, &list, cards.clone(), on_pick.clone(), Pick::Show);
        connect_pick(&forget, &list, cards.clone(), on_pick.clone(), Pick::Forget);

        let settings = SettingsView::build(on_pick);
        let stack = gtk::Stack::new();
        stack.add_titled(&root, "favourites", "Favourites");
        stack.add_titled(&settings.page, "settings", "Settings");
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
        let selected = self
            .list
            .selected_row()
            .and_then(|row| self.cards.borrow().get(row.index() as usize).cloned())
            .map(|card| card.key);
        let cards: Vec<Card> = favourites
            .iter()
            .map(|(key, art)| Card {
                key: key.to_string(),
                art: art.clone(),
            })
            .collect();

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
            let label = gtk::Label::new(Some(&card.art.title));
            label.set_line_wrap(true);
            label.set_max_width_chars(24);
            content.pack_start(&image, false, false, 0);
            content.pack_start(&label, false, false, 0);
            row.add(&content);
            self.list.add(&row);
        }
        self.list.show_all();

        let row = selected
            .and_then(|key| self.cards.borrow().iter().position(|card| card.key == key))
            .or_else(|| (!self.cards.borrow().is_empty()).then_some(0))
            .and_then(|index| self.list.row_at_index(index as i32));
        match row {
            Some(row) => self.list.select_row(Some(&row)),
            None => self.list.unselect_all(),
        }
    }
}

fn connect_control(button: &gtk::Button, on_control: Rc<dyn Fn(Control)>, action: fn() -> Control) {
    button.connect_clicked(move |_| on_control(action()));
}

fn connect_pick(
    button: &gtk::Button,
    list: &gtk::ListBox,
    cards: Rc<RefCell<Vec<Card>>>,
    on_pick: Rc<dyn Fn(Pick)>,
    action: fn(String) -> Pick,
) {
    let list = list.clone();
    button.connect_clicked(move |_| {
        let Some(row) = list.selected_row() else {
            return;
        };
        if let Some(card) = cards.borrow().get(row.index() as usize) {
            on_pick(action(card.key.clone()));
        }
    });
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
    artists_section: gtk::Box,
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

        let page = gtk::Box::new(gtk::Orientation::Horizontal, 24);
        add_class(&page, "aw-page");
        page.set_margin_top(8);
        page.set_margin_bottom(8);
        page.set_margin_start(8);
        page.set_margin_end(8);

        // Left: the preview card. The margin leaves room for the shadow to be seen.
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
        page.pack_start(&card, false, false, 0);

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

        // Right: sections that scroll, and the bar that does not.
        let right = gtk::Box::new(gtk::Orientation::Vertical, 0);
        right.set_hexpand(true);
        let scroll = gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_shadow_type(gtk::ShadowType::None);
        scroll.set_vexpand(true);
        let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
        column.set_margin_end(12);
        scroll.add(&column);
        right.pack_start(&scroll, true, true, 0);

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
        let artists_section = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let artists = chip_flow();
        artists_section.pack_start(&section_title("Artist"), false, false, 0);
        artists_section.pack_start(&artists, false, false, 0);
        artists_section.set_no_show_all(true);
        filters.pack_start(&artists_section, false, false, 0);

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
        bar.pack_start(&note, true, true, 0);
        bar.pack_end(&apply, false, false, 0);
        right.pack_start(&bar, false, false, 0);
        page.pack_start(&right, true, true, 0);

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
            artists_section,
            religious,
            canvas,
            empty,
            surface,
            note,
            apply,
        });
        view.connect();
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
        self.note.set_text(pending.note().unwrap_or(""));
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
            pending.artists(),
        );
        drop(pending);

        self.rebuild_style_options(kind, border, variant, strength, custom);
        self.fill(&self.shapes, shapes, |p, v: &Shape| p.set_shape(*v));
        self.fill(&self.regions, regions, |p, v: &Region| p.toggle_region(*v));
        self.fill(&self.subjects, subjects, |p, v: &Subject| {
            p.toggle_subject(*v)
        });
        self.artists_section.set_visible(!artists.is_empty());
        self.fill(&self.artists, artists, |p, v: &String| p.toggle_artist(v));
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
                    },
                    Chip {
                        value: Border::Auto,
                        label: "Automatic".to_owned(),
                        selected: border == Border::Auto,
                    },
                    Chip {
                        value: Border::Custom { rgb: custom },
                        label: "Custom".to_owned(),
                        selected: matches!(border, Border::Custom { .. }),
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
                    },
                    Chip {
                        value: BlurVariant::WholeImage,
                        label: "Whole picture".to_owned(),
                        selected: variant == BlurVariant::WholeImage,
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
