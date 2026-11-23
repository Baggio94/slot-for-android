# Cartridge materials and label registration

Technical reference for the cartridge materials introduced in
slot. for Android 3.0.

## Rendering

Cart Studio, the carousel and animated cartridge layers share the
native renderer. The cartridge mould determines its geometry, label
window, plastic material and PCB presentation.

`home_material.rs` calibrates selected cartridge fronts before the
usual colour overlay. The source PNGs and PCB artwork remain separate.

The Japanese Master System cartridge, Sega My Card and Game Gear
Codemasters moulds use additional material calibration. Other moulds
retain their existing material treatment.

## Material behavior

- Solid and clear finishes preserve the mould's relief and geometry.
- Transparent plastics keep the underlying PCB visible.
- Opaque labels remain opaque, including on transparent shells.
- Rounded label corners preserve the underlying plastic transparency.
- Glitter uses deterministic placement and consistent intensity.
- Exposed contacts, the My Card contact area and Pokémon mini screws
  are protected from plastic tinting and glitter.
- Pokémon mini retains its frosted clear-plastic appearance.
- Master System plastic regions retain their relief without the
  unwanted photographic grain.
- WonderSwan guide tabs and the My Card window rim follow the
  selected cartridge colour.
- The cartridge material treatment does not change slot bars,
  themes or cartridge insertion mechanisms.

## Label registration

Logical pixels (panel-density scaling is applied by the renderer):

| Mould | Canvas | Label x,y,w,h |
|---|---|---|
| NGP / NGPC | 312 × 280 | 31,66,252,138 |
| WS / WSC | 312 × 188 | 10,26,293,108 |
| Game Gear | 312 × 316 | 28,58,256,185 |
| Game Gear Codemasters | 312 × 367 | 41,17,231,295 |
| Master System | 312 × 200 | 8,4,296,42 |
| Master System Japan | 312 × 360 | 29,2,254,209 |
| Sega My Card | 312 × 500 | 18,21,279,290 |
| Lynx | 312 × 326 | 5,69,302,211 |
| Pokémon mini | 312 × 300 | 19,107,274,149 |

`HomeCartLabels.kt` uses the native mould table for special SMS/GG labels,
and supplies these windows for support-2D fallback. The own art set remains
the first source and sticker scans remain preferred. Western and Japanese
SMS labels are selected for their respective shapes.
For Game Gear Codemasters, the full cart render is preferred exclusively:
the current Micro Machines sticker scan in the own set is horizontal and
cannot fit the tall well. If absent from the own set, the existing ScreenScraper
fallback is used; an unavailable fitting image keeps the handwritten label.
Cached automatic labels cut for a different mould can be fetched again. Custom choices are protected
by the existing byte comparison before replacing the displayed label. Old
negative results for 3.0 labels are retried once with the new mould rules.

## Verification

`new_cart_material` checks colour response, canvas/label registration at
1×, 4/3× and 3×, transparent mini body/cap, opaque stickers, clipped corners,
fixed contacts, deterministic glitter and the repaired Codemasters outline.
An additional regression check prevents an opaque unprinted strip beside
clear labels when the origin and width round differently at fractional density.
The existing cartridge/density/upstream asset tests remain in the CI suite.
Android JVM tests cover special-mould selection, regional label compatibility,
cached-label recutting and support-2D fallback for all new platforms.

Reproduce the real native renderer's blank material and registration previews:

```sh
cargo run --locked -p slot-ui --example material_preview -- /tmp/material-preview 1.3333333
```

An optional third argument names a directory of labels (`ngp.png`, `ws.png`,
`wsc.png`, `gg.png`, `sms.png`, `lynx.png`, `pokemini.png`, `sms_jp.png`,
`sms_card.png`, `gg_codemasters.png`). These pass through the same native label
loader as the application, producing additional labelled material previews.

Device review still required: check white, black, saturated colours, both
glitter finishes and clear shells in Cart Studio, then return to the shelf,
open/select the core where supported, insert/eject and reopen the app. Check
light/dark/OLED backgrounds. The automated checks do not validate every
individual scan in the art database or replace a physical handheld review.
