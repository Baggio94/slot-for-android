use slot_store::Core;
use slot_ui::{ease, Millis, Refusal, CHIP_TIP};

pub const SLIDE_MS: Millis = 160;
pub const LIFT_MS: Millis = 260;
pub const OPEN_MS: Millis = SLIDE_MS + LIFT_MS;
pub const CLOSE_MS: Millis = 320;
pub const HOP_MS: Millis = 180;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Press {
    Left,
    Right,
    Keep,
    Back,
    /// A touch on one of the board's chips, by its place (0 is the first).
    To(usize),
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Outcome {
    Nothing,
    Refused,
    Write(Core),
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct Chip {
    /// Where the chip is between the board's places: 0 the first, 1 the
    /// second, 2 the third (PokeDaisy's), fractions while it hops.
    pub across: f32,
    /// The place it hops from and to (equal once seated), and how far
    /// along it is, eased.
    pub from: usize,
    pub to: usize,
    pub along: f32,
    pub lift: f32,
    pub tip: f32,
    pub seated: Option<Core>,
    pub shake: f32,
}

#[derive(Copy, Clone)]
struct Hop {
    from: usize,
    started: Millis,
}

#[derive(Copy, Clone)]
struct Close {
    started: Millis,
    from: f32,
}

#[derive(Copy, Clone)]
pub struct CorePicker {
    seat: Core,
    /// The board's cores in their places' order: two, or three on a
    /// Pokemon game's board (PokeDaisy first).
    cores: [Core; 3],
    count: usize,
    opened: Millis,
    started: Option<Millis>,
    hop: Option<Hop>,
    close: Option<Close>,
    refusal: Option<Refusal>,
    /// The one core this cart runs on (Splore: PICO-8); the other socket
    /// is greyed and the chip refuses to go there.
    only: Option<Core>,
}

impl CorePicker {
    pub fn open(seat: Core, now: Millis) -> Self {
        Self::open_with(seat, Core::Gpsp, now)
    }

    /// The right-hand socket holds the cart's other core: gpSP on a GBA
    /// board, Gambatte on a GB/GBC board. mGBA sits on the left.
    pub fn open_with(seat: Core, right: Core, now: Millis) -> Self {
        Self::open_between(seat, Core::Mgba, right, now)
    }

    /// A board's two sockets: the system's first core on the left (FCEUmm,
    /// Snes9x, Genesis Plus GX on the home consoles), its other one right.
    pub fn open_between(seat: Core, left: Core, right: Core, now: Millis) -> Self {
        Self::open_places([left, right, right], 2, seat, now)
    }

    /// A Pokemon game's board: PokeDaisy's chip, then the system's two.
    pub fn open_three(seat: Core, cores: [Core; 3], now: Millis) -> Self {
        Self::open_places(cores, 3, seat, now)
    }

    fn open_places(cores: [Core; 3], count: usize, seat: Core, now: Millis) -> Self {
        CorePicker {
            seat,
            cores,
            count,
            opened: now,
            started: None,
            hop: None,
            close: None,
            refusal: None,
            only: None,
        }
    }

    /// The chip stays on `core`: the other socket is greyed.
    pub fn only(mut self, core: Core) -> Self {
        self.only = Some(core);
        self
    }

    /// The chip can't move (only): the board shows no Swap.
    pub fn fixed(&self) -> bool {
        self.only.is_some()
    }

    /// Whether the chip may go to `core`.
    pub fn allows(&self, core: Core) -> bool {
        self.only.is_none_or(|only| only == core)
    }

    pub fn start(&mut self, now: Millis) {
        self.started.get_or_insert(now);
    }

    pub fn waiting(&self) -> bool {
        self.started.is_none()
    }

    pub fn waited(&self, now: Millis) -> Millis {
        now.saturating_sub(self.opened)
    }

    pub fn seat(&self) -> Core {
        self.seat
    }

    pub fn left(&self) -> Core {
        self.cores[0]
    }

    pub fn right(&self) -> Core {
        self.cores[self.count - 1]
    }

    /// The board's cores, in their places' order.
    pub fn cores(&self) -> &[Core] {
        &self.cores[..self.count]
    }

    /// The place of a core on this board.
    pub fn place(&self, core: Core) -> usize {
        self.cores().iter().position(|c| *c == core).unwrap_or(0)
    }

    pub fn closing(&self) -> bool {
        self.close.is_some()
    }

    pub fn openness(&self, now: Millis) -> f32 {
        match (self.close, self.started) {
            (Some(Close { started, from }), _) => {
                (from - now.saturating_sub(started) as f32 / CLOSE_MS as f32).max(0.0)
            }
            (None, Some(started)) => (now.saturating_sub(started) as f32 / OPEN_MS as f32).min(1.0),
            (None, None) => 0.0,
        }
    }

    pub fn finished(&self, now: Millis) -> bool {
        self.close.is_some() && self.openness(now) <= 0.0
    }

    pub fn press(&mut self, press: Press, now: Millis) -> Outcome {
        if self.close.is_some() {
            return Outcome::Nothing;
        }
        let target = match press {
            Press::Keep => {
                self.begin_close(now);
                return Outcome::Write(self.seat);
            }
            Press::Back => {
                self.begin_close(now);
                return Outcome::Nothing;
            }
            // One place over; the last one shakes, as the core in place.
            Press::Left => match self.place(self.seat).checked_sub(1) {
                Some(at) => self.cores[at],
                None => self.seat,
            },
            Press::Right => self.cores[(self.place(self.seat) + 1).min(self.count - 1)],
            Press::To(at) => self.cores[at.min(self.count - 1)],
        };
        if self.waiting() {
            return Outcome::Nothing;
        }
        if !self.allows(target) {
            self.refusal = Some(Refusal::started(now));
            return Outcome::Refused;
        }
        if let Some(progress) = self.hop_progress(now) {
            if target == self.seat {
                return Outcome::Nothing;
            }
            let done = ((1.0 - progress) * HOP_MS as f32) as Millis;
            self.hop = Some(Hop {
                from: self.place(self.seat),
                started: now.saturating_sub(done),
            });
            self.seat = target;
            self.refusal = None;
            return Outcome::Nothing;
        }
        if target == self.seat {
            self.refusal = Some(Refusal::started(now));
            return Outcome::Refused;
        }
        self.hop = Some(Hop {
            from: self.place(self.seat),
            started: now,
        });
        self.seat = target;
        self.refusal = None;
        Outcome::Nothing
    }

    pub fn chip(&self, now: Millis) -> Chip {
        let shake = self.refusal.map_or(0.0, |r| r.offset(now));
        match (self.hop, self.hop_progress(now)) {
            (Some(hop), Some(q)) => {
                let to = self.place(self.seat);
                let rightward = to > hop.from;
                let lean = if rightward { 1.0 } else { -1.0 };
                let arc = (std::f32::consts::PI * q).sin();
                Chip {
                    across: hop.from as f32 + (to as f32 - hop.from as f32) * ease(q),
                    from: hop.from,
                    to,
                    along: ease(q),
                    lift: arc,
                    tip: lean * CHIP_TIP * arc,
                    seated: None,
                    shake,
                }
            }
            _ => Chip {
                across: self.place(self.seat) as f32,
                from: self.place(self.seat),
                to: self.place(self.seat),
                along: 1.0,
                lift: 0.0,
                tip: 0.0,
                seated: Some(self.seat),
                shake,
            },
        }
    }

    fn begin_close(&mut self, now: Millis) {
        self.close = Some(Close {
            started: now,
            from: self.openness(now),
        });
    }

    fn hop_progress(&self, now: Millis) -> Option<f32> {
        let hop = self.hop?;
        let q = now.saturating_sub(hop.started) as f32 / HOP_MS as f32;
        (q < 1.0).then_some(q)
    }
}
