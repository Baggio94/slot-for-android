package fyi.slot.konkr

import org.junit.Assert.assertEquals
import org.junit.Test

class ArtworkPriorityTest {
    private data class Cart(val uri: String, val platform: String, val title: String)
    @Test fun focusedCartAndNeighboursFirstWithoutDroppingGames() {
        val carts = listOf(Cart("a", Systems.GG, "A"), Cart("gb", Systems.GB, "GB"),
            Cart("b", Systems.GG, "B"), Cart("c", Systems.GG, "C"), Cart("d", Systems.GG, "D"))
        val sorted = ArtworkPriority.order(carts, "c", { it.uri }, { it.platform }, { it.title })
        assertEquals(listOf("c", "b", "d", "a", "gb"), sorted.map { it.uri })
        assertEquals(carts.toSet(), sorted.toSet())
    }
    @Test fun firstNonemptySystemWinsWithoutFocus() {
        val carts = listOf(Cart("gg", Systems.GG, "Game"), Cart("gb", Systems.GB, "Game"))
        assertEquals(listOf("gb", "gg"), ArtworkPriority.order(carts, null,
            { it.uri }, { it.platform }, { it.title }).map { it.uri })
    }
}
