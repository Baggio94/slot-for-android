//! How PokeDaisy shows a game, chosen with SELECT + Up / Down in place of
//! slot.'s screen sizes (it draws its own canvas at the panel's shape):
//! ZOOM's steps (each one whole scale further out, more of the map), EXPAND
//! (the game's scale, the map rebuilt out to the screen's edges) or ORIGINAL
//! (the game's own picture, at slot.'s screen size). Kept per game, as the
//! screen size is; the companion's own pick is kept the same way.
//!
//! Based on PokeDaisy by Lidor Itzhari - https://github.com/lidor30/pokedaisy
use std::path::Path;

use slot_store::Platform;

pub const VIEW_FILE: &str = "Config/pokedaisy_view.ini";

/// PokeDaisy's largest canvas (its CANVAS_MAX_W x CANVAS_MAX_H).
const CANVAS_MAX: (u32, u32) = (640, 480);
/// PokeDaisy's ZOOM_STEPS_MAX: 1x and at most five steps out.
const LEVELS_MAX: u32 = 6;

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum View {
    /// ZOOM's step, 1 the first one out.
    Zoom(u8),
    #[default]
    Expand,
    Original,
}

impl View {
    /// The core's `pokedaisy_aspect` value.
    pub fn option(self) -> String {
        match self {
            View::Zoom(step) => format!("zoom{}", step.max(1)),
            View::Expand => "expand".into(),
            View::Original => "original".into(),
        }
    }

    pub fn parse(text: &str) -> Option<View> {
        let text = text.trim().to_ascii_lowercase();
        match text.as_str() {
            "expand" => Some(View::Expand),
            "original" => Some(View::Original),
            "zoom" => Some(View::Zoom(1)),
            _ => text.strip_prefix("zoom")?.parse::<u8>().ok().filter(|n| *n >= 1).map(View::Zoom),
        }
    }

    /// This view as the game can show it: a ZOOM step past the game's last
    /// is its last, and a game with none shows EXPAND.
    pub fn within(self, steps: u8) -> View {
        match self {
            View::Zoom(_) if steps == 0 => View::Expand,
            View::Zoom(step) => View::Zoom(step.clamp(1, steps)),
            other => other,
        }
    }

    /// SELECT + Up goes toward ORIGINAL, Down toward ZOOM's last step; the
    /// ends stay, as the screen sizes do. `steps`: this game's ZOOM steps.
    pub fn step(self, up: bool, steps: u8) -> View {
        match (self.within(steps), up) {
            (View::Original, true) => View::Original,
            (View::Expand, true) => View::Original,
            (View::Zoom(1), true) => View::Expand,
            (View::Zoom(step), true) => View::Zoom(step - 1),
            (View::Original, false) => View::Expand,
            (View::Expand, false) if steps > 0 => View::Zoom(1),
            (View::Zoom(step), false) if step < steps => View::Zoom(step + 1),
            (view, false) => view,
        }
    }
}

/// The game's own frame on PokeDaisy: the Game Boy's or the GBA's.
pub fn game_frame(platform: Platform) -> (u32, u32) {
    if platform == Platform::Gba { (240, 160) } else { (160, 144) }
}

/// ZOOM's steps for a game on this panel, as PokeDaisy counts them
/// (zoom_steps): each one whole game scale further out than EXPAND, while
/// the canvas still holds the game and fits the core's largest.
pub fn zoom_steps(panel: (u32, u32), game: (u32, u32)) -> u8 {
    let ((pw, ph), (fw, fh)) = (panel, game);
    if pw == 0 || ph == 0 || fw == 0 || fh == 0 { return 0; }
    let scale = (pw / fw).min(ph / fh);
    let mut levels = 0;
    for level in 0..LEVELS_MAX {
        let Some(s) = scale.checked_sub(level).filter(|s| *s >= 1) else { break };
        let (x, y) = (pw / s, ph / s);
        if x > CANVAS_MAX.0 || y > CANVAS_MAX.1 || x < fw || y < fh { break; }
        levels += 1;
    }
    levels.saturating_sub(1) as u8
}

pub fn view_for(root: &Path, stem: &str) -> View {
    slot_store::ini::value(root, VIEW_FILE, stem).as_deref().and_then(View::parse).unwrap_or_default()
}

pub fn write_view(root: &Path, stem: &str, view: View) -> std::io::Result<()> {
    slot_store::ini::write(root, VIEW_FILE, stem, &view.option())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn up_goes_toward_the_original_picture_through_every_zoom_step() {
        assert_eq!(View::default(), View::Expand);
        assert_eq!(View::Expand.step(true, 2), View::Original);
        assert_eq!(View::Original.step(true, 2), View::Original);
        assert_eq!(View::Original.step(false, 2), View::Expand);
        assert_eq!(View::Expand.step(false, 2), View::Zoom(1));
        assert_eq!(View::Zoom(1).step(false, 2), View::Zoom(2));
        assert_eq!(View::Zoom(2).step(false, 2), View::Zoom(2));
        assert_eq!(View::Zoom(2).step(true, 2), View::Zoom(1));
        assert_eq!(View::Zoom(1).step(true, 2), View::Expand);
        // A game with no ZOOM step stops at EXPAND.
        assert_eq!(View::Expand.step(false, 0), View::Expand);
        assert_eq!(View::Zoom(3).within(0), View::Expand);
        assert_eq!(View::Zoom(5).within(2), View::Zoom(2));
    }

    #[test]
    fn the_core_counts_the_same_zoom_steps() {
        // KONKR Pocket Advance (960x640): EXPAND at 4x, then 3x and 2x.
        assert_eq!(zoom_steps((960, 640), (240, 160)), 2);
        assert_eq!(zoom_steps((960, 640), (160, 144)), 2);
        // 1920x1080: the GB at 7x, out to the 640x480 canvas (3x is 640x360).
        assert_eq!(zoom_steps((1920, 1080), (160, 144)), 4);
        assert_eq!(zoom_steps((0, 0), (160, 144)), 0);
    }

    #[test]
    fn each_game_keeps_its_own_view_and_the_option_reads_back() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("Config")).unwrap();
        assert_eq!(view_for(d.path(), "Pokemon Emerald"), View::Expand);
        write_view(d.path(), "Pokemon Emerald", View::Zoom(2)).unwrap();
        assert_eq!(view_for(d.path(), "Pokemon Emerald"), View::Zoom(2));
        assert_eq!(view_for(d.path(), "Pokemon Crystal"), View::Expand);
        assert_eq!(View::parse("ORIGINAL"), Some(View::Original));
        assert_eq!(View::parse("zoom"), Some(View::Zoom(1)));
        assert_eq!(View::parse("zoom0"), None);
        for view in [View::Original, View::Expand, View::Zoom(1), View::Zoom(4)] {
            assert_eq!(View::parse(&view.option()), Some(view));
        }
    }
}
