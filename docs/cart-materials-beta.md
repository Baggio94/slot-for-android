# 3.0.0-materials2 — cartridge materials

Base: `beta/3.0-rc2`, `dc51f28c776baa23c2e924b1505cd5ceee334cfd`.
Android version code: 571 (materials1 was 570). This branch builds a signed test APK; it does not publish a stable release.

## Rendering

The existing native renderer is shared by Cart Studio, the carousel and the
separate animated cartridge layers. `home_material.rs` calibrates the 3.0
fronts before their usual colour overlay, cached once per mould and density.
It softens contrast and removes the baked black/white base colour of the
photographic special moulds. The source PNGs and PCB art are unchanged.

The Pokémon mini clear front and cap are less opaque, with denser reliefs
and a lighter frost. The real board remains a separate layer; opaque labels
remain opaque. Rounded/curved sticker cuts restore bare plastic including
its alpha, so clipped ink cannot show through a clear shell. Codemasters'
Game Gear scan uses the existing back-shell SVG outline to remove internal
alpha cracks, and its remaining donor-label strip is neutralised.

Glitter keeps its deterministic positions, has softer highlights and retains
edge coverage. Explicit plastic regions protect the exposed WS/Lynx contacts,
My Card contact panel and mini screw holes from tinting/glitter.
GB/GBC/GBA and the older home moulds retain their existing material path.
Slot bars, their themes and mechanisms are unchanged by this beta.

## materials2

Siham compared both renders and kept the best of each:

- NGP, Game Gear, Master System, WonderSwan/WSC and Lynx keep the rc2
  masters (sharper reliefs) and rc2 clear plastic. Only the Japanese Master
  System, My Card and Game Gear Codemasters are recalibrated
  (`home_material::calibrated`).
- The Pokémon mini keeps the more translucent clear body and the rc2 frosted cap.
- Glitter has the same strength as GB/GBC/GBA on every mould; contacts,
  the My Card contact panel and the mini's screws stay free of it.
- WonderSwan guide tabs and the My Card window rim (white plastic in their
  photos) take the cart colour; the Japanese Master System's lower body is
  one flat plastic with the frame.
- The disk cache of cart faces redraws the six 3.0 systems once.

## materials3

- Master System (`assets/home/sms_neutral.png`): the photo grain under the ribs
  is gone; the body is one flat plastic, the ribs and the side and bottom rims
  keep their relief without speckles.
- Pokemon mini: rc2's frosted clear plastic again (only the recalibrated special
  moulds keep the more translucent one).
- The disk cache key moves to `materials-3`, so the 3.0 faces are drawn again.

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
