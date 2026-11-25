//! The cartridge shelf selector. One native page, no new settings or ROM I/O.
use super::*;

type Face = (slot_gfx::TexId, u32, u32);
const ROWS: usize = 3;
const MARGIN: f32 = 22.0;
const TOP: f32 = 70.0;
const BOTTOM: f32 = 414.0;
const ANIM_MS: f32 = 180.0;

#[derive(Default)]
pub(super) struct GridState {
    art: Vec<Option<Face>>,
    labels: Vec<Face>,
    heading: Option<Face>,
    marks: Option<[Face; 2]>,
    opened: Option<Instant>,
    closing: Option<(usize, Instant)>,
    first_row: usize,
    touch: Option<(f32, f32, i64)>,
}

impl GridState {
    pub(super) fn reset(&mut self) {
        self.opened = None;
        self.closing = None;
        self.first_row = 0;
        self.touch = None;
    }
}

fn columns(width: u32) -> usize {
    match width {
        0..=510 => 3,
        511..=675 => 4,
        676..=779 => 5,
        _ => 6,
    }
}

fn shelf_order(nonempty: impl Fn(usize) -> bool, show_recents: bool) -> Vec<usize> {
    let mut order = Vec::with_capacity(Platform::ALL.len() + 3);
    if show_recents && nonempty(RECENTS_RACK) { order.push(RECENTS_RACK); }
    if nonempty(FAVORITES_RACK) { order.push(FAVORITES_RACK); }
    for (rack, platform) in Platform::ALL.iter().enumerate() {
        if nonempty(rack) { order.push(rack); }
        if *platform == Platform::Gba && nonempty(VIDEO_RACK) { order.push(VIDEO_RACK); }
    }
    order
}

fn first_row(current: usize, selected: usize, count: usize, cols: usize) -> usize {
    let max = count.div_ceil(cols).saturating_sub(ROWS);
    let row = selected / cols;
    current.min(max).min(row).max(row.saturating_sub(ROWS - 1)).min(max)
}

fn move_cell(index: usize, count: usize, cols: usize, key: i32) -> usize {
    if count == 0 { return 0; }
    let i = index.min(count - 1);
    match key {
        19 => i.checked_sub(cols).unwrap_or(i),
        20 if (i / cols + 1) * cols < count => (i + cols).min(count - 1),
        21 => i.saturating_sub(1),
        22 => (i + 1).min(count - 1),
        _ => i,
    }
}

fn cell(width: u32, cols: usize, index: usize, first: usize) -> [f32; 4] {
    let pitch_x = (width as f32 - 2.0 * MARGIN) / cols as f32;
    let pitch_y = (BOTTOM - TOP) / ROWS as f32;
    [MARGIN + (index % cols) as f32 * pitch_x + 4.0,
        TOP + (index / cols - first) as f32 * pitch_y + 4.0,
        pitch_x - 8.0, pitch_y - 8.0]
}

fn fit(face: Face, cx: f32, cy: f32, max_w: f32, max_h: f32, alpha: f32, out: &mut Vec<Draw>) {
    let (tex, w, h) = face;
    let scale = (max_w / w.max(1) as f32).min(max_h / h.max(1) as f32);
    let (w, h) = (w as f32 * scale, h as f32 * scale);
    out.push(Draw::Tex { x: cx - w / 2.0, y: cy - h / 2.0, w, h, tex, alpha });
}

fn rect(out: &mut Vec<Draw>, [x,y,w,h]: [f32;4], colour: [f32;4]) {
    out.push(Draw::Rect { x,y,w,h,colour });
}

fn asset(platform: Platform) -> &'static [u8] {
    match platform {
        Platform::Gba => include_bytes!("../../../../assets/gba_neutral.png"),
        Platform::Gb => include_bytes!("../../../../assets/gb_neutral.png"),
        Platform::Gbc => include_bytes!("../../../../assets/gbc_neutral.png"),
        Platform::Nes => include_bytes!("../../../../assets/home/nes_neutral.png"),
        Platform::Snes => include_bytes!("../../../../assets/home/snes_neutral.png"),
        Platform::Md => include_bytes!("../../../../assets/home/md_neutral.png"),
        Platform::Sms => include_bytes!("../../../../assets/home/sms_neutral.png"),
        Platform::Gg => include_bytes!("../../../../assets/home/gg_neutral.png"),
        Platform::Ngp => include_bytes!("../../../../assets/home/ngp_neutral.png"),
        Platform::Ws => include_bytes!("../../../../assets/home/ws_neutral.png"),
        Platform::Lynx => include_bytes!("../../../../assets/home/lynx_neutral.png"),
        Platform::Pokemini => include_bytes!("../../../../assets/home/pokemini_neutral.png"),
    }
}

// Upload at a bounded density, preserving the real silhouette and alpha.
fn thumbnail(gpu: &mut Compositor, platform: Platform) -> Option<Face> {
    let (rgba, sw, sh) = slot_ui::decode_png(asset(platform))?;
    if sw == 0 || sh == 0 { return None; }
    let scale = (256.0 / sw.max(sh) as f32).min(1.0);
    let w = (sw as f32 * scale).round().max(1.0) as u32;
    let h = (sh as f32 * scale).round().max(1.0) as u32;
    let mut pixels = Vec::with_capacity((w*h*4) as usize);
    for y in 0..h {
        for x in 0..w {
            let sx = ((x as u64 * sw as u64) / w as u64) as u32;
            let sy = ((y as u64 * sh as u64) / h as u64) as u32;
            let at = ((sy * sw + sx) * 4) as usize;
            pixels.extend_from_slice(&rgba[at..at+4]);
        }
    }
    let tex = gpu.create_texture(w,h,&pixels);
    Some((tex,w,h))
}

impl Engine {
    fn shelf_grid_order(&self) -> Vec<usize> {
        shelf_order(|rack| self.shelves.get(rack).is_some_and(|s| !s.carts.is_empty()),
            self.settings.show_recents)
    }

    pub(super) fn open_shelves(&mut self) {
        let order = self.shelf_grid_order();
        let Some(&rack) = order.iter().find(|&&r| r == self.active).or_else(|| order.first()) else { return; };
        self.shelves[self.active].release_hold();
        self.touch = None;
        self.studio_when_settled = false;
        self.menu_held = None;
        self.shelves_grid.reset();
        self.shelves_grid.opened = Some(Instant::now());
        self.shelves_grid.first_row = first_row(0, order.iter().position(|&r| r == rack).unwrap_or(0), order.len(), columns(out_w()));
        self.overlay = ShelfOverlay::Shelves { rack };
    }

    fn close_shelves(&mut self, rack: usize, select: bool) {
        let order = self.shelf_grid_order();
        self.shelves_grid.touch = None;
        self.shelves_grid.closing = Some((rack, Instant::now()));
        self.menu_held = None;
        self.overlay = ShelfOverlay::None;
        if select && order.contains(&rack) && rack != self.active {
            self.shelves[self.active].release_hold();
            self.active = rack;
            self.progress = 0.0;
            self.letter_shown_at = None;
            self.platform_shown_at = Some(Instant::now());
            self.last_cart_navigation = Instant::now();
        }
    }

    pub(super) fn shelves_key(&mut self, key: i32) {
        let ShelfOverlay::Shelves { rack } = self.overlay else { return; };
        let order = self.shelf_grid_order();
        let Some(&fallback) = order.first() else {
            self.shelves_grid.reset();
            self.overlay = ShelfOverlay::None;
            return;
        };
        let current = order.iter().position(|&r| r == rack).unwrap_or(0);
        match key {
            96 => self.close_shelves(order[current], true),
            97 | 108 => self.close_shelves(rack, false),
            19..=22 | 102 | 103 => {
                let cols = columns(out_w());
                let key = match key { 102 => 21, 103 => 22, _ => key };
                let next = move_cell(current, order.len(), cols, key);
                self.shelves_grid.first_row = first_row(self.shelves_grid.first_row, next, order.len(), cols);
                self.overlay = ShelfOverlay::Shelves { rack: order.get(next).copied().unwrap_or(fallback) };
            }
            _ => {}
        }
    }

    pub(super) fn shelves_touch(&mut self, action: u8, x: f32, y: f32, ms: i64) {
        // Android: DOWN=0, MOVE=1, UP=2, CANCEL=3.
        if action == 0 {
            self.shelves_grid.touch = Some((x,y,ms));
            return;
        }
        if action == 1 { return; }
        let Some((sx,sy,at)) = self.shelves_grid.touch.take() else { return; };
        if action != 2 { return; }
        let dy = y - sy;
        if dy.abs() > 34.0 && dy.abs() > (x-sx).abs() {
            self.shelves_key(if dy < 0.0 {20} else {19});
            return;
        }
        if (x-sx).hypot(dy) > 18.0 || ms - at > 600 { return; }
        let order = self.shelf_grid_order();
        let cols = columns(out_w());
        let first = self.shelves_grid.first_row.min(order.len().div_ceil(cols).saturating_sub(ROWS));
        for (index,&rack) in order.iter().enumerate().skip(first*cols).take(ROWS*cols) {
            let [left,top,w,h] = cell(out_w(), cols, index, first);
            if x >= left && x <= left+w && y >= top && y <= top+h {
                self.close_shelves(rack, true);
                return;
            }
        }
    }

    pub(super) fn prepare_shelves_grid(&mut self) {
        if self.game.is_some() || self.inserted || self.progress > 0.0 { return; }
        let open = matches!(self.overlay, ShelfOverlay::Shelves { .. });
        // Warm at most one neutral thumbnail per idle frame, never during boot,
        // scrolling or a game. Opening directly also works before warming.
        if !open && (!matches!(self.overlay, ShelfOverlay::None)
            || self.last_cart_navigation.elapsed() < Duration::from_millis(1800)
            || self.shelves[self.active].is_scrolling()) { return; }
        if self.shelf_grid_order().is_empty() { return; }
        if self.shelves_grid.art.is_empty() {
            self.shelves_grid.art.resize(Platform::ALL.len(), None);
        }
        if self.shelves_grid.heading.is_none() {
            let face = menu_style::menu_path_face("SHELVES");
            self.shelves_grid.heading = Some((self.gpu.create_face(&face),face.w,face.h));
            let names = Platform::ALL.iter().map(|p|p.name()).chain(["RECENTS","FAVORITES","GBA Video"]);
            self.shelves_grid.labels = names.map(|name| {
                let face = quick_value_face(name, true);
                (self.gpu.create_face(&face),face.w,face.h)
            }).collect();
            let clock = slot_ui::analog_clock_face(24, 10, 10, [250,250,250]);
            let star = slot_ui::star_face(2,[246,195,44]);
            self.shelves_grid.marks = Some([
                (self.gpu.create_face(&clock),clock.w,clock.h),
                (self.gpu.create_face(&star),star.w,star.h),
            ]);
        }
        if let Some(index) = self.shelves_grid.art.iter().position(Option::is_none) {
            self.shelves_grid.art[index] = thumbnail(&mut self.gpu, Platform::ALL[index]);
        }
    }

    pub(super) fn draw_shelves_overlay(&self, out: &mut Vec<Draw>) -> bool {
        let (rack,alpha) = if let ShelfOverlay::Shelves { rack } = self.overlay {
            let t = self.shelves_grid.opened.map_or(1.0, |at| at.elapsed().as_secs_f32()*1000.0 / ANIM_MS);
            (rack,slot_ui::ease(t.clamp(0.0,1.0)))
        } else if matches!(self.overlay, ShelfOverlay::None) && !self.inserted {
            let Some((rack,at)) = self.shelves_grid.closing else { return false; };
            let t = at.elapsed().as_secs_f32()*1000.0 / ANIM_MS;
            if t >= 1.0 { return false; }
            (rack,1.0-slot_ui::ease(t.clamp(0.0,1.0)))
        } else { return false; };
        let width = out_w();
        let order = self.shelf_grid_order();
        if order.is_empty() { return true; }
        let cols = columns(width);
        let selected = order.iter().position(|&r|r==rack).unwrap_or(0);
        let first = first_row(self.shelves_grid.first_row,selected,order.len(),cols);
        let ink = menu_style::ink_map();
        // The current wallpaper remains underneath, with the theme's veil.
        // Redraw the ground over the carousel so no slot or old carts show.
        let screen = [0.0,0.0,width as f32,OUT_H as f32];
        let mut ground = menu_style::shelf_ground();
        ground[3] = alpha;
        rect(out,screen,ground);
        if let Some((tex,tint,_)) = self.active_wallpaper() {
            out.push(Draw::Tex { x:0.0,y:0.0,w:width as f32,h:OUT_H as f32,tex,alpha });
            if tint > 0.0 { rect(out,screen,[0.0,0.0,0.0,tint*alpha]); }
            let mut veil = menu_style::scrim(0.82);
            veil[3] *= alpha;
            rect(out,screen,veil);
        }
        let tone = if ink.is_some() {0.0} else {1.0};
        if let Some(face) = self.shelves_grid.heading {
            out.push(Draw::Ink(ink));
            fit(face,width as f32/2.0,52.0,180.0,20.0,alpha,out);
            out.push(Draw::Ink(None));
        }
        for (index,&id) in order.iter().enumerate().skip(first*cols).take(ROWS*cols) {
            let [x,y,w,h] = cell(width,cols,index,first);
            let chosen = index == selected;
            rect(out,[x,y,w,h],[tone,tone,tone,alpha*if chosen {0.18} else {0.055}]);
            if chosen {
                let line = [tone,tone,tone,0.87*alpha];
                rect(out,[x,y,w,1.5],line); rect(out,[x,y+h-1.5,w,1.5],line);
                rect(out,[x,y,1.5,h],line); rect(out,[x+w-1.5,y,1.5,h],line);
            }
            let cx = x+w/2.0;
            let cy = y+h*0.40;
            let zoom = (if chosen {1.08} else {1.0}) * (0.94+0.06*alpha);
            let icon = |platform:Platform| Platform::ALL.iter().position(|&p|p==platform)
                .and_then(|i| self.shelves_grid.art.get(i).copied().flatten());
            if id == RECENTS_RACK || id == FAVORITES_RACK {
                for (p,dx,dy) in [(Platform::Gb,-0.19,1.0),(Platform::Gbc,0.19,1.0),(Platform::Gba,0.0,-3.0)] {
                    if let Some(face) = icon(p) { fit(face,cx+dx*w,cy+dy,w*0.50*zoom,h*0.61*zoom,alpha,out); }
                }
                if let Some(marks)=self.shelves_grid.marks {
                    let mark=marks[usize::from(id==FAVORITES_RACK)];
                    rect(out,[cx-13.0,cy-13.0,26.0,26.0],[0.13,0.13,0.14,0.90*alpha]);
                    fit(mark,cx,cy,22.0,22.0,alpha,out);
                }
            } else {
                let platform = if id==VIDEO_RACK { Some(Platform::Gba) } else { Platform::ALL.get(id).copied() };
                if let Some(face) = platform.and_then(icon) {
                    fit(face,cx,cy,w*0.76*zoom,h*0.63*zoom,alpha,out);
                }
            }
            if let Some(&face) = self.shelves_grid.labels.get(id) {
                out.push(Draw::Ink(ink));
                fit(face,cx,y+h-13.0,w-12.0,17.0,alpha*if chosen {1.0} else {0.82},out);
                out.push(Draw::Ink(None));
            }
        }
        if order.len()>ROWS*cols {
            let total = order.len().div_ceil(cols);
            let track = BOTTOM-TOP-8.0;
            let thumb = track*ROWS as f32/total as f32;
            rect(out,[width as f32-10.0,TOP+4.0,2.0,track],[tone,tone,tone,0.13*alpha]);
            rect(out,[width as f32-10.0,TOP+4.0+track*first as f32/total as f32,2.0,thumb],[tone,tone,tone,0.7*alpha]);
        }
        let from = out.len();
        out.push(Draw::Ink(ink));
        for (tex,w,x) in centred_hints(&self.menu_hints,LEGEND_GAP) {
            out.push(Draw::Tex { x,y:437.0,w:w as f32,h:HINT_H as f32,tex,alpha });
        }
        if ink.is_some() { self.theme_hints(from,out); }
        out.push(Draw::Ink(None));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shelves_only_nonempty_and_specials_first() {
        let gba=Platform::ALL.iter().position(|p|*p==Platform::Gba).unwrap();
        let gb=Platform::ALL.iter().position(|p|*p==Platform::Gb).unwrap();
        let present=|r| [gba,gb,RECENTS_RACK,FAVORITES_RACK,VIDEO_RACK].contains(&r);
        let order=shelf_order(present,true);
        assert_eq!(&order[..2], &[RECENTS_RACK,FAVORITES_RACK]);
        assert_eq!(order[order.iter().position(|&r|r==gba).unwrap()+1],VIDEO_RACK);
        assert!(!shelf_order(present,false).contains(&RECENTS_RACK));
        assert!(shelf_order(|_|false,true).is_empty());
    }
    #[test]
    fn layouts_and_navigation() {
        assert_eq!([columns(480),columns(549),columns(640),columns(720),columns(853)],[3,4,4,5,6]);
        assert_eq!(move_cell(7,10,3,20),9);
        assert_eq!(move_cell(9,10,3,20),9);
        assert_eq!(move_cell(2,10,3,19),2);
        assert_eq!(first_row(0,14,15,3),2);
        assert_eq!(first_row(2,0,15,3),0);
        assert_eq!(first_row(2,1,2,3),0);
    }
}
