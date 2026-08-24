use super::{Control, Pick, Snapshot};
use crate::art::Artwork;
use crate::favourites::Favourites;
use anyhow::Result;
use gdk_pixbuf::Pixbuf;
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
        connect_pick(&forget, &list, cards.clone(), on_pick, Pick::Forget);

        root.show_all();
        Ok(Self {
            title,
            byline,
            open,
            next,
            keep,
            today,
            reapply,
            login,
            list,
            cards,
            thumbs: Rc::new(RefCell::new(HashMap::new())),
            updating_login,
        })
    }

    pub(super) fn describe(&self, snapshot: &Snapshot, favourites: &Favourites) {
        self.describe_status(snapshot);
        self.set_login(snapshot.starts_at_login);
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
