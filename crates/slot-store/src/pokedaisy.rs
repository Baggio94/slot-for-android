//! Which carts PokeDaisy plays: the Pokemon games its companion reads, by
//! the header the library scan already keeps (the GBA game code, the Game
//! Boy title). A hack keeps its base game's header and comes along; a
//! version PokeDaisy doesn't know still plays, with its own "not
//! supported" notice instead of the companion.
//!
//! Based on PokeDaisy by Lidor Itzhari - https://github.com/lidor30/pokedaisy

use crate::Platform;

/// The GBA game codes' first three letters: Ruby, Sapphire, FireRed,
/// LeafGreen and Emerald in every language, and the two hacks' own
/// (PokeDaisy's pd_library.c).
const GBA_CODES: [&str; 7] = ["AXV", "AXP", "BPR", "BPG", "BPE", "MON", "PUV"];

/// The Game Boy spin-offs whose title starts like a main game's.
const GB_SPIN_OFFS: [&str; 4] = ["PIN", "PUZ", "CARD", "TCG"];

/// `code`: the GBA header's 4-letter game code. `gb_title`: the Game Boy
/// header's title, its first 11 bytes (Red is "POKEMON RED", Gold
/// "POKEMON_GLD", Crystal "PM_CRYSTAL").
pub fn plays(platform: Platform, code: &str, gb_title: &str) -> bool {
    match platform {
        Platform::Gba => {
            code.len() == 4 && GBA_CODES.iter().any(|base| code.starts_with(base))
        }
        Platform::Gb | Platform::Gbc => {
            let title = gb_title.trim().to_ascii_uppercase();
            (title.starts_with("POKEMON") || title.starts_with("PM_CRYSTAL"))
                && !GB_SPIN_OFFS.iter().any(|spin| title.contains(spin))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_main_games_and_their_hacks_play() {
        for code in ["BPRE", "BPRF", "BPGE", "BPEE", "BPEJ", "AXVE", "AXVD", "AXPE", "AXPI"] {
            assert!(plays(Platform::Gba, code, ""), "{code}");
        }
        for title in ["POKEMON RED", "POKEMON BLU", "POKEMON YEL", "POKEMON_GLD",
                      "POKEMON_SLV", "PM_CRYSTAL", "pokemon red"] {
            assert!(plays(Platform::Gb, "", title), "{title}");
            assert!(plays(Platform::Gbc, "", title), "{title}");
        }
    }

    #[test]
    fn other_games_do_not() {
        for code in ["AGBJ", "BPPE", "", "BPR", "AMKE"] {
            assert!(!plays(Platform::Gba, code, ""), "{code:?}");
        }
        for title in ["POKEMON PIN", "TETRIS", "", "POKEMON PUZ", "ZELDA"] {
            assert!(!plays(Platform::Gb, "", title), "{title:?}");
        }
        assert!(!plays(Platform::Nes, "BPRE", "POKEMON RED"));
        assert!(!plays(Platform::Gba, "", "POKEMON RED"),
            "a GBA cart is known by its code, never a Game Boy title");
    }
}
