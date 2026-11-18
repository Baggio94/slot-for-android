package fyi.slot.konkr

import org.junit.Assert.assertEquals
import org.junit.Test

/** Where the slot. folder writes a file: each folder on the way made once. */
class SlotFolderPathTest {
    @Test
    fun aNewFolderGetsItsSubfoldersOneByOne() {
        val made = mutableListOf<Pair<String, String>>()
        val known = HashMap<String, String>()
        val make = { parent: String, name: String -> made += parent to name; "$parent/$name" }

        assertEquals("root", folderFor("slot.json", "root", known, make))
        assertEquals("root/Saves/mGBA", folderFor("Saves/mGBA/Golden Sun.srm", "root", known, make))
        assertEquals(listOf("root" to "Saves", "root/Saves" to "mGBA"), made)

        // Known folders are not looked up again; only the new one is made.
        assertEquals("root/Saves/gpSP", folderFor("Saves/gpSP/Golden Sun.srm", "root", known, make))
        assertEquals("root/Saves/gpSP", folderFor("Saves/gpSP/Metroid.srm", "root", known, make))
        assertEquals(3, made.size)
        assertEquals("root/Saves" to "gpSP", made.last())

        // Deep paths, as Config/ and CustomCheats/ have.
        assertEquals("root/CustomCheats/GBA", folderFor("CustomCheats/GBA/Golden Sun.cht", "root", known, make))
        assertEquals(5, made.size)
    }
}
