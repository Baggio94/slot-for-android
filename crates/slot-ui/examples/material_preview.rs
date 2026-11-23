//! Deterministic material and label registration previews from the real renderer.
//! cargo run -p slot-ui --example material_preview -- OUT_DIR [DENSITY] [LABEL_DIR]
use std::path::{Path, PathBuf};

use slot_store::{Cart, ShellChoice, ShellFinish};
use slot_ui::{density, CartFace, Face, HomeMould};

fn save(dir: &Path, name: &str, face: &CartFace) {
    let (w, h) = face.texels();
    let file = std::io::BufWriter::new(std::fs::File::create(dir.join(name)).unwrap());
    let mut png = png::Encoder::new(file, w, h);
    png.set_color(png::ColorType::Rgba);
    png.set_depth(png::BitDepth::Eight);
    png.write_header().unwrap().write_image_data(&face.rgba).unwrap();
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().expect("OUT_DIR"));
    density::set_scale(args.next().map_or(1.0, |v| v.parse().unwrap()));
    let labels = args.next().map(PathBuf::from);
    std::fs::create_dir_all(&dir).unwrap();
    use HomeMould::*;
    let moulds = [
        ("ngp", NeoGeoPocket), ("ws", WonderSwan), ("wsc", WonderSwanColor),
        ("gg", GameGear), ("sms", MasterSystem), ("lynx", Lynx),
        ("pokemini", PokemonMini), ("sms_jp", MasterSystemJp),
        ("sms_card", MasterSystemCard), ("gg_codemasters", GameGearCodemasters),
    ];
    use ShellFinish::*;
    let variants = [
        ("neutral", [128, 128, 128], Solid),
        ("black", [18, 18, 18], Solid), ("white", [240, 240, 240], Solid),
        ("red", [210, 25, 35], Solid), ("clear", [124, 150, 178], Clear),
        ("gold", [158, 133, 54], SolidGlitter),
        ("purple", [124, 79, 184], ClearGlitter),
    ];
    for (key, mould) in moulds {
        for (name, colour, finish) in variants {
            let cart = Cart {
                platform: mould.platform(), stem: String::new(), title: String::new(),
                code: String::new(), rom: PathBuf::from(format!("{key}.rom")), label: None,
                shell: Some(ShellChoice { outline: mould.outline(), colour, finish }), battery: false,
            };
            let (w, h) = slot_ui::label_size(&cart);
            let (w, h) = (density::px(w), density::px(h));
            let blank = vec![0; (w * h * 4) as usize];
            save(&dir, &format!("{key}_{name}.png"), &slot_ui::cart_face_with_material(&cart, Some(blank)));
            if let Some(path) = labels.as_ref().map(|d| d.join(format!("{key}.png"))).filter(|p| p.is_file()) {
                let mut labelled = cart.clone();
                labelled.label = Some(path);
                save(&dir, &format!("{key}_{name}_label.png"), &slot_ui::cart_face_with_material(&labelled, None));
            }
            if name == "clear" {
                save(&dir, &format!("{key}_front.png"), &slot_ui::cart_face_with(&cart, Some(vec![0; (w * h * 4) as usize])));
                save(&dir, &format!("{key}_board.png"), &slot_ui::cart_backing(&cart));
                let mut label = vec![255; (w * h * 4) as usize];
                for y in 0..h {
                    for x in 0..w {
                        let i = ((y * w + x) * 4) as usize;
                        let rgb = if (x / density::px(12) + y / density::px(12)) % 2 == 0 {
                            [240, 80, 140]
                        } else { [60, 210, 225] };
                        label[i..i + 3].copy_from_slice(&rgb);
                    }
                }
                save(&dir, &format!("{key}_label-check.png"), &slot_ui::cart_face_with_material(&cart, Some(label)));
            }
        }
        println!("{key}: material variants, PCB, separate front and label registration exported");
    }
}
