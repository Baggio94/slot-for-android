//! The menus' look: a ground a shade under the housing, a rounded bar under
//! the selected row, and a small path line over each page's title.
use crate::density::Canvas;
use crate::plate::UndoFace;
use crate::power_menu::MENU_INK;
use crate::slot_chrome::theme;
use crate::text;

/// The selected row's bar: its height and corner radius.
pub const PILL_H: u32 = 44;
pub const PILL_R: u32 = 11;
/// The bar's left and right edges, from the screen's.
pub const PILL_LEFT: f32 = 28.0;
pub const PILL_RIGHT: f32 = 22.0;

const PATH_PX: f32 = 13.0;
const PATH_TRACKING: f32 = 2.0;
pub const PATH_H: u32 = 18;

/// The menus' ground: the housing, a shade darker.
pub fn menu_ground_rgb() -> [u8; 3] {
    theme().housing.map(|c| (c as f32 * 0.88).round() as u8)
}

pub fn menu_ground() -> [f32; 4] {
    let c = menu_ground_rgb();
    [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, 1.0]
}

/// The bar's colour: the theme's edge, settled into the ground.
pub fn pill_rgb() -> [u8; 3] {
    let (ground, edge) = (menu_ground_rgb(), theme().edge);
    [0, 1, 2].map(|i| (ground[i] as f32 + (edge[i] as f32 - ground[i] as f32) * 0.62).round() as u8)
}

/// The bar in three faces: its left end, a strip to stretch between, its
/// right end. One colour throughout, so the joins can't show.
pub fn pill_faces() -> [UndoFace; 3] {
    let colour = pill_rgb();
    let end = |right: bool| {
        let mut face = Canvas::new(PILL_R, PILL_H);
        let (tw, th) = (face.tw as f32, face.th as f32);
        let r = tw;
        for y in 0..face.th {
            for x in 0..face.tw {
                // The corner's centre is on the inner side of the end.
                let px = if right { x as f32 + 0.5 } else { tw - (x as f32 + 0.5) };
                let py = y as f32 + 0.5;
                let dy = if py < r { r - py } else if py > th - r { py - (th - r) } else { 0.0 };
                let cover = if dy > 0.0 {
                    (r - (px * px + dy * dy).sqrt() + 0.5).clamp(0.0, 1.0)
                } else { 1.0 };
                let at = ((y * face.tw + x) * 4) as usize;
                face.rgba[at..at + 3].copy_from_slice(&colour);
                face.rgba[at + 3] = (cover * 255.0).round() as u8;
            }
        }
        face.into_undo()
    };
    let mut middle = Canvas::new(4, PILL_H);
    for p in middle.rgba.chunks_exact_mut(4) {
        p[..3].copy_from_slice(&colour);
        p[3] = 255;
    }
    [end(false), middle.into_undo(), end(true)]
}

/// A page's path line ("SETTINGS > SYSTEMS"): small, widely spaced capitals.
pub fn menu_path_face(label: &str) -> UndoFace {
    let Some(font) = text::label_font() else {
        return UndoFace { rgba: Vec::new(), w: 0, h: 0 };
    };
    let upper = label.to_uppercase();
    let ink = text::line_width(font, &upper, PATH_PX, PATH_TRACKING).ceil() as u32;
    let mut face = Canvas::new(ink + 4, PATH_H);
    let layout = text::Layout { lines: vec![upper], px: PATH_PX, tracking: PATH_TRACKING };
    face.text(&layout, MENU_INK);
    face.into_undo()
}

/// The mark on a row that opens a page.
pub fn menu_opens_face() -> UndoFace {
    let Some(font) = text::label_font() else {
        return UndoFace { rgba: Vec::new(), w: 0, h: 0 };
    };
    let px = 26.0;
    let w = text::line_width(font, "\u{203a}", px, 0.0).ceil() as u32 + 4;
    let mut face = Canvas::new(w, crate::power_menu::MENU_H);
    let layout = text::Layout { lines: vec!["\u{203a}".to_owned()], px, tracking: 0.0 };
    face.text(&layout, MENU_INK);
    face.into_undo()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::density::Face as _;

    #[test]
    fn the_bar_is_round_at_its_ends_and_whole_between() {
        let [left, middle, right] = pill_faces();
        let (w, h) = left.texels();
        let alpha = |f: &UndoFace, x: u32, y: u32| f.rgba[((y * w + x) * 4 + 3) as usize];
        assert_eq!(alpha(&left, 0, 0), 0, "outside the corner");
        assert!(alpha(&left, w - 1, 0) >= 250, "the top edge meets the strip");
        assert_eq!(alpha(&left, 0, h / 2), 255, "the side is straight");
        assert_eq!(alpha(&right, w - 1, 0), 0);
        assert!(alpha(&right, 0, h - 1) >= 250);
        assert!(middle.rgba.chunks_exact(4).all(|p| p[3] == 255));
        assert_eq!(&left.rgba[((h / 2 * w) * 4) as usize..][..3], &middle.rgba[..3]);
    }

    #[test]
    fn the_ground_sits_under_the_housing_and_the_bar_over_it() {
        let luma = |c: [u8; 3]| c.iter().map(|&v| v as u32).sum::<u32>();
        assert!(luma(menu_ground_rgb()) < luma(theme().housing));
        assert!(luma(pill_rgb()) > luma(menu_ground_rgb()));
    }

    #[test]
    fn a_path_line_and_the_mark_have_ink() {
        for face in [menu_path_face("Settings \u{203a} Systems"), menu_opens_face()] {
            assert!(face.w > 0 && face.rgba.chunks_exact(4).any(|p| p[3] > 0));
        }
    }
}
