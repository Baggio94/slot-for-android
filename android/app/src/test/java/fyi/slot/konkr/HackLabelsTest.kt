package fyi.slot.konkr

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** The cases of tools/home-art/test_import_hacks.py, on HACKS.names.json's titles. */
class HackLabelsTest {
    private val titles = mapOf(
        "radical red" to "Pokémon Radical Red",
        "sors 2" to "Pokémon Sors 2",
        "sors" to "Pokémon Sors",
        "too many types 2" to "Pokémon Too Many Types 2",
        "firered rocket edition" to "Pokémon FireRed Rocket Edition",
        "firered and leafgreen plus" to "Pokémon FireRed & LeafGreen+",
        "rowe" to "Pokémon R.O.W.E.",
        "lets go pikachu" to "Pokémon Let's Go Pikachu (GBA)",
        "mariomon" to "Super Mariomon",
        "monomyth" to "Pokémon MonoMyth",
        "lost artifacts" to "Pokémon Hyper Emerald Lost Artifacts",
        "super dark worship" to "Pokémon Super Dark Worship",
        "picross" to "Pokémon Picross (English translation)",
        "balloon fight" to "Balloon Kid (Balloon Fight GB, English translation)",
        "card gb2" to "Pokémon Trading Card Game 2 (English translation)",
        "mega man 5 dx" to "Mega Man World 5 DX",
        "peach" to "Pokémon Peach",
    )

    @Test fun keysAreTheirOwnWords() {
        for (key in titles.keys) assertEquals(key, key.split(' '), HackLabels.words(key))
    }

    @Test fun fileNamesFindTheirHack() {
        val cases = mapOf(
            "Pokemon - Radical Red (v4.1).gba" to "radical red",
            "Pokémon Sors 2.gba" to "sors 2",
            "Pokemon Sors.gba" to "sors",
            "Too Many Types 2 v1.0 (Pokemon).gba" to "too many types 2",
            "Pokemon Fire Red Rocket Edition.gba" to "firered rocket edition",
            "Pokemon FireRed & LeafGreen+.gba" to "firered and leafgreen plus",
            "Pokemon R.O.W.E. v1.2.gba" to "rowe",
            "Pokemon Let's Go Pikachu.gba" to "lets go pikachu",
            "Super Mariomon.gba" to "mariomon",
            "Pokemon MonoMyth v1.1.gba" to "monomyth",
            "Pokemon - Hyper Emerald Lost Artifacts.gba" to "lost artifacts",
            "Pokemon Super Dark Worship.gba" to "super dark worship",
            "Pokemon Picross (Japan) [T-En].gbc" to "picross",
            "Balloon Fight GB (Japan) [T-En].gb" to "balloon fight",
            "Pokemon Card GB2 (Japan) [T-En].gbc" to "card gb2",
            "Mega Man World 5 DX.gbc" to "mega man 5 dx",
            "Pokemon - FireRed Version (USA).gba" to null,
            "Peach Adventure.gba" to null,
        )
        for ((name, key) in cases) assertEquals(name, key, HackLabels.keyOf(name, titles))
    }

    @Test fun aHackGoesOnItsOwnFamily() {
        assertTrue(HackLabels.fits("GB", "GBC"))
        assertTrue(HackLabels.fits("GBC", "GB"))
        assertTrue(HackLabels.fits("GBA", "GBA"))
        assertFalse(HackLabels.fits("GBA", "GBC"))
        assertFalse(HackLabels.fits("GB", "GBA"))
    }
}
