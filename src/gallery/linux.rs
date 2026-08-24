use super::Pick;
use crate::favourites::Favourites;
use anyhow::Result;
use std::rc::Rc;
use tao::window::Window;

/// Compile-time bridge until the GTK gallery is installed.
pub(super) struct Content;

impl Content {
    pub(super) fn install(_window: &Window, _on_pick: Rc<dyn Fn(Pick)>) -> Result<Self> {
        // Keep the shared vocabulary live until the GTK controls replace this
        // compile-only bridge.
        let _ = (Pick::Show(String::new()), Pick::Forget(String::new()));
        Ok(Self)
    }

    pub(super) fn relist(&self, favourites: &Favourites) {
        let _ = favourites.iter().count();
    }
}
