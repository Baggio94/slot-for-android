//! Build-time-only export of EXACT Slot core-picker keycap faces for the
//! Android Apps drawer. Reuse the upstream drawing/font code, not an Android
//! imitation with different fonts or Unicode arrow glyphs.
//!
//! Faces are drawn at 3x, so each PNG is three times Slot's logical size and
//! stays sharp on 3x panels; Android divides by `SlotUi.LEGEND_DENSITY`.
use slot_ui::density::{self, Face};
use slot_ui::{arrows_hint_face, arrows_label_x, hint_face, hint_label_x, quick_value_face, UndoFace};
use std::{fs::{self, File}, io::BufWriter, path::PathBuf};

fn write_png(out: &PathBuf, face: &UndoFace) -> Result<(), Box<dyn std::error::Error>> {
    let file = BufWriter::new(File::create(out)?);
    let (w, h) = face.texels();
    let mut encoder = png::Encoder::new(file, w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&face.rgba)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let destination = std::env::args().nth(1)
        .ok_or("Usage: cargo run -p slot-ui --example export_apps_legend -- <output-dir>")?;
    let destination = PathBuf::from(destination);
    fs::create_dir_all(&destination)?;
    density::with_scale(3.0, || export(&destination))
}

fn export(destination: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let key = |key: &str, label: &str| (hint_face(key, label), Some(hint_label_x(key)));
    let arrows = |label: &str| (arrows_hint_face(label), Some(arrows_label_x()));
    let heading = |text: &str| (quick_value_face(text, true), None);
    // Where each legend's word starts, in texels: Android keeps the keycap
    // as drawn and inks the word alone in Light and Original.
    let mut splits = String::new();
    for (filename, (face, split)) in [
        ("move.png", arrows("Move")),
        ("open.png", key("A", "Open")),
        ("back.png", key("B", "Back")),
        // Shared authentic keycaps for Android dialogs / Studio / RA panel.
        ("select.png", key("A", "Select")),
        ("refresh.png", key("START", "Refresh")),
        ("sign-out.png", key("X", "Sign Out")),
        // Library > RomM's forms: the sign-in's two modes on the shoulders
        // (the controller's own L1 / R1 art), a filter's value on Left/Right.
        ("l1-pairing-code.png", key("L1", "Pairing Code")),
        ("r1-account.png", key("R1", "Account")),
        ("change.png", arrows("Change")),
        // Pixel-identical to Rust's Library/Settings heading textures.
        ("heading-apps.png", heading("APPS")),
        ("heading-ra.png", heading("RETROACHIEVEMENTS")),
        ("heading-save-conflict.png", heading("SAVE CONFLICT")),
        ("heading-save-state.png", heading("SAVE STATE")),
        ("heading-sign-out.png", heading("SIGN OUT")),
        ("heading-link-consent.png", heading("LINK — SHARE CARTRIDGE")),
        ("heading-direct-join.png", heading("MULTIPLAYER — DIRECT JOIN")),
    ] {
        let expected_height = if filename.starts_with("heading-") { 40 } else { 24 };
        assert!(face.w > 20 && face.h == expected_height,
            "Slot legend / native heading generation failed");
        let (w, h) = face.texels();
        assert_eq!((w, h), (density::px(face.w), density::px(face.h)));
        write_png(&destination.join(filename), &face)?;
        if let Some(split) = split {
            splits.push_str(&format!("{filename} {}\n", density::px(split)));
        }
        println!("{}: {}×{} texels for {}×{} Slot pixels", filename, w, h, face.w, face.h);
    }
    fs::write(destination.join("legend-splits.txt"), splits)?;
    Ok(())
}
