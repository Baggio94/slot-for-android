//! How PokeDaisy shows a game, chosen with SELECT + Up / Down in place of
//! slot.'s screen sizes (it draws its own canvas at the panel's shape):
//! ZOOM (one whole scale less, more of the map), EXPAND (the game's scale,
//! the map rebuilt out to the screen's edges) or ORIGINAL (the game's own
//! picture). Kept per game, as the screen size is.
//!
//! Based on PokeDaisy by Lidor Itzhari - https://github.com/lidor30/pokedaisy
use std::path::Path;

pub const VIEW_FILE: &str = "Config/pokedaisy_view.ini";

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum View {
    Zoom,
    #[default]
    Expand,
    Original,
}

impl View {
    pub const ORDER: [View; 3] = [View::Zoom, View::Expand, View::Original];

    /// The core's `pokedaisy_aspect` value.
    pub fn option(self) -> &'static str {
        match self {
            View::Zoom => "zoom",
            View::Expand => "expand",
            View::Original => "original",
        }
    }

    pub fn parse(text: &str) -> Option<View> {
        let text = text.trim();
        View::ORDER.into_iter().find(|view| text.eq_ignore_ascii_case(view.option()))
    }

    /// SELECT + Up goes toward ORIGINAL, Down toward ZOOM; the ends stay,
    /// as the screen sizes do.
    pub fn step(self, up: bool) -> View {
        let at = View::ORDER.iter().position(|v| *v == self).unwrap_or(1);
        let to = if up { (at + 1).min(View::ORDER.len() - 1) } else { at.saturating_sub(1) };
        View::ORDER[to]
    }
}

pub fn view_for(root: &Path, stem: &str) -> View {
    slot_store::ini::value(root, VIEW_FILE, stem).as_deref().and_then(View::parse).unwrap_or_default()
}

pub fn write_view(root: &Path, stem: &str, view: View) -> std::io::Result<()> {
    slot_store::ini::write(root, VIEW_FILE, stem, view.option())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn up_goes_toward_the_original_picture_and_stops_at_the_ends() {
        assert_eq!(View::default(), View::Expand);
        assert_eq!(View::Expand.step(true), View::Original);
        assert_eq!(View::Original.step(true), View::Original);
        assert_eq!(View::Expand.step(false), View::Zoom);
        assert_eq!(View::Zoom.step(false), View::Zoom);
    }

    #[test]
    fn each_game_keeps_its_own_view() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("Config")).unwrap();
        assert_eq!(view_for(d.path(), "Pokemon Emerald"), View::Expand);
        write_view(d.path(), "Pokemon Emerald", View::Zoom).unwrap();
        assert_eq!(view_for(d.path(), "Pokemon Emerald"), View::Zoom);
        assert_eq!(view_for(d.path(), "Pokemon Crystal"), View::Expand);
        assert_eq!(View::parse("ORIGINAL"), Some(View::Original));
    }
}
