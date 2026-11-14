package fyi.slot.konkr

import org.junit.Assert.assertEquals
import org.junit.Test

class Pico8FavouritesTest {
    private val list = """
        |mossmoss-17          |mossmoss             |1794   |noelcody         |                     |moss moss
        |lootslime-1          |lootslime            |1794   |moleman          |                     |lootslime 1.01
        |loop-4               |loop                 |1794   |kreyk29          |                     |
        not a favourite
        |../etc/passwd        |x                    |1794   |x                |                     |x
        |mossmoss-17          |mossmoss             |1794   |noelcody         |                     |moss moss
    """.trimIndent()

    @Test fun favouritesAreReadOncePerCart() {
        val favs = Pico8Favourites.parse(list)
        assertEquals(listOf("mossmoss-17", "lootslime-1", "loop-4"), favs.map { it.id })
        assertEquals("loop", favs[2].title)
    }

    @Test fun fileNamesAreTitlesWithCapitals() {
        val favs = Pico8Favourites.parse(list)
        assertEquals("Moss Moss.p8.png", Pico8Favourites.fileName(favs[0]))
        assertEquals("Lootslime 1.01.p8.png", Pico8Favourites.fileName(favs[1]))
        assertEquals("A B.p8.png", Pico8Favourites.fileName(Pico8Favourites.Favourite("x-1", "a/b")))
        assertEquals("x-1.p8.png", Pico8Favourites.fileName(Pico8Favourites.Favourite("x-1", "../")))
    }
}
