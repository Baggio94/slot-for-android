//! Game menu > Cheats, as in RetroArch: the game's cheat files (Game Genie,
//! Action Replay...), each cheat turned on or off with A. What is on is kept
//! for the game and applied again at its next start (crate::cheats).
use super::*;

/// The longest cheat name shown whole; longer ones end in "…".
const NAME_CHARS: usize = 44;

/// One row of the Cheats page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CheatRow {
    Cheat(usize),
    /// The game has no cheat file.
    Empty,
}

pub(super) fn short_name(desc: &str) -> String {
    if desc.chars().count() <= NAME_CHARS { return desc.to_owned(); }
    let cut: String = desc.chars().take(NAME_CHARS - 1).collect();
    format!("{}…", cut.trim_end())
}

impl Engine {
    /// Find the game's cheats (by checksum, else by name) and open the page.
    pub(super) fn open_cheats(&mut self) {
        let Some(game) = self.game.as_ref() else { return };
        if !game.takes_cheats() {
            set_message("Cheats are off during Multiplayer.".into());
            return;
        }
        let platform = self.current_cart_platform();
        let started = Instant::now();
        let checksums = std::fs::read(game.rom_path())
            .map(|rom| crate::cheats::rom_checksums(platform, &rom)).unwrap_or_default();
        self.cheat_sources = crate::cheats::lookup(platform, &checksums, game.rom_stem());
        // Start on the file holding a cheat already on, if any.
        let on = game.cheats();
        self.cheat_source = self.cheat_sources.iter()
            .position(|source| source.cheats.iter().any(|c| on.iter().any(|o| o.code == c.code)))
            .unwrap_or(0);
        crate::game::log_launch_timing(&format!("Cheats: {} files found in {}ms",
            self.cheat_sources.len(), started.elapsed().as_millis()));
        self.overlay = ShelfOverlay::Cheats { row: 0 };
    }

    pub(super) fn cheat_rows(&self) -> Vec<CheatRow> {
        let Some(source) = self.cheat_sources.get(self.cheat_source) else {
            return vec![CheatRow::Empty];
        };
        (0..source.cheats.len()).map(CheatRow::Cheat).collect()
    }

    pub(super) fn cheat_is_on(&self, index: usize) -> bool {
        let (Some(game), Some(source)) = (self.game.as_ref(), self.cheat_sources.get(self.cheat_source))
            else { return false };
        source.cheats.get(index)
            .is_some_and(|cheat| game.cheats().iter().any(|on| on.code == cheat.code))
    }

    fn set_game_cheats(&mut self, cheats: Vec<crate::cheats::Cheat>) {
        let Some(game) = self.game.as_mut() else { return };
        if let Err(error) = game.set_cheats(cheats) {
            set_error_message("Couldn't change the cheats.", &error);
        }
    }

    pub(super) fn cheats_menu_key(&mut self, code: i32) {
        let ShelfOverlay::Cheats { row } = self.overlay else { return };
        let rows = self.cheat_rows();
        match code {
            19 | 20 => self.overlay = ShelfOverlay::Cheats { row: move_menu_row(row, code, rows.len(), self.menu_repeat_dispatch) },
            // L1 / R1: the game's other cheat file (Codes, Game Genie...),
            // from its first cheat.
            102 | 103 if self.cheat_sources.len() > 1 => {
                let count = self.cheat_sources.len();
                self.cheat_source = if code == 103 { (self.cheat_source + 1) % count }
                    else { (self.cheat_source + count - 1) % count };
                self.overlay = ShelfOverlay::Cheats { row: 0 };
            }
            96 => if let Some(CheatRow::Cheat(index)) = rows.get(row).copied() {
                let Some(cheat) = self.cheat_sources.get(self.cheat_source)
                    .and_then(|source| source.cheats.get(index)).cloned() else { return };
                let mut on: Vec<crate::cheats::Cheat> =
                    self.game.as_ref().map(|game| game.cheats().to_vec()).unwrap_or_default();
                match on.iter().position(|c| c.code == cheat.code) {
                    Some(at) => { on.remove(at); }
                    None => on.push(cheat),
                }
                self.set_game_cheats(on);
            },
            // Y: every cheat of the game off, whichever file it came from.
            100 => if self.game.as_ref().is_some_and(|game| !game.cheats().is_empty()) {
                self.set_game_cheats(Vec::new());
                set_message("All cheats off".into());
            },
            97 => {
                let linked = self.game.as_ref().is_some_and(GameSession::linked);
                let relaunch = self.game.as_ref().is_some_and(GameSession::requires_relaunch);
                self.overlay = ShelfOverlay::GameMenu {
                    row: pause_rows(linked, relaunch).iter()
                        .position(|item| *item == PauseRow::Cheats).unwrap_or(0),
                };
                self.cheat_sources = Vec::new();
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::short_name;

    #[test]
    fn long_names_end_in_an_ellipsis() {
        assert_eq!(short_name("Infinite Lives"), "Infinite Lives");
        let long = "Have All Items In The Inventory Except The Ones You Need";
        let short = short_name(long);
        assert!(short.ends_with('…'));
        assert!(short.chars().count() <= 44);
    }
}
