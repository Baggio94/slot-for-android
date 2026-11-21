package fyi.slot.konkr

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.ByteArrayInputStream
import java.util.zip.CRC32

class HomeCartLabelsTest {
    private fun rom(platform: String, name: String, region: String = "", maker: String = "") =
        HomeCartLabels.Rom("content://x", platform, name, "$name.rom", 1, 1, region, maker)

    @Test
    fun shellsFollowTheRegionAndPublisher() {
        assertEquals("fc", HomeCartLabels.shell(rom(Systems.NES, "Super Mario Bros. 3 (Japan)")))
        assertEquals("nes", HomeCartLabels.shell(rom(Systems.NES, "Super Mario Bros. (Japan, USA)")))
        assertEquals("snes_us", HomeCartLabels.shell(rom(Systems.SNES, "Super Mario World (USA)")))
        assertEquals("snes", HomeCartLabels.shell(rom(Systems.SNES, "Super Mario World (Japan)")))
        assertEquals("snes_us", HomeCartLabels.shell(rom(Systems.SNES, "smw", region = "U")))
        assertEquals("md_jp", HomeCartLabels.shell(rom(Systems.MD, "Sonic (Japan)")))
        assertEquals("md_ea", HomeCartLabels.shell(rom(Systems.MD, "Road Rash (USA, Europe)", maker = "(C)T-50 1991.SEP")))
    }

    @Test
    fun eaCartsFollowSlotUisTable() {
        HomeCartLabels.readShells(java.io.File("../../crates/slot-ui/assets/home_shells.tsv").readText())
        assertEquals("md_ea", HomeCartLabels.shell(rom(Systems.MD, "Road Rash (Europe)", maker = "(C)T-50 1991.SEP")))
        assertEquals("md_ea", HomeCartLabels.shell(rom(Systems.MD, "Zool (World)")))
        assertEquals("md_ea", HomeCartLabels.shell(rom(Systems.MD, "Where in Time Is Carmen Sandiego (USA)")))
        assertEquals("md", HomeCartLabels.shell(rom(Systems.MD, "Where in Time Is Carmen Sandiego (Europe)")))
        assertEquals("md", HomeCartLabels.shell(rom(Systems.MD, "Sonic The Hedgehog (USA, Europe)")))
        assertEquals("md_jp", HomeCartLabels.shell(rom(Systems.MD, "Road Rash (Japan)")))
        assertEquals("fifa98roadtoworldcup", HomeCartLabels.titleKey("FIFA 98 - Road to World Cup (Europe) (En,Fr)"))
        assertEquals("roadrash", HomeCartLabels.titleKey("Road Rash (Europe).md"))
        assertEquals("drmario", HomeCartLabels.titleKey("Dr. Mario (Japan, USA).zip"))
    }

    @Test
    fun publishersMegaDriveMouldsFollowSlotUisTable() {
        HomeCartLabels.readShells(java.io.File("../../crates/slot-ui/assets/home_shells.tsv").readText())
        assertEquals("md_accolade", HomeCartLabels.shell(rom(Systems.MD, "Bubsy II (USA, Europe)")))
        assertEquals("md_accolade", HomeCartLabels.shell(rom(Systems.MD, "Gods (Europe)")))
        assertEquals("md", HomeCartLabels.shell(rom(Systems.MD, "Gods (USA)")))
        assertEquals("md_codemasters", HomeCartLabels.shell(rom(Systems.MD, "Micro Machines 2 - Turbo Tournament (Europe) (J-Cart)")))
        assertEquals("md_vr", HomeCartLabels.shell(rom(Systems.MD, "Virtua Racing (USA)")))
        assertEquals("md_sk", HomeCartLabels.shell(rom(Systems.MD, "Sonic & Knuckles (World)")))
        assertEquals("md_jp", HomeCartLabels.shell(rom(Systems.MD, "Virtua Racing (Japan)")))
    }

    @Test
    fun labelsCutForAnotherShellAreCutAgain() {
        HomeCartLabels.readShells(java.io.File("../../crates/slot-ui/assets/home_shells.tsv").readText())
        val vr = rom(Systems.MD, "Virtua Racing (USA)")
        val sonic = rom(Systems.MD, "Sonic The Hedgehog (USA, Europe)")
        val ea = rom(Systems.MD, "Road Rash (USA)", maker = "(C)T-50 1991.SEP")
        val metroid = rom(Systems.NES, "Metroid (USA)")
        // Saved before slot. remembered the shell: cut for Sega's own cart.
        assertEquals(true, HomeCartLabels.recut(JSONObject().put(vr.uri, true), vr))
        assertEquals(false, HomeCartLabels.recut(JSONObject().put(sonic.uri, true), sonic))
        assertEquals(false, HomeCartLabels.recut(JSONObject().put(ea.uri, true), ea))
        assertEquals(false, HomeCartLabels.recut(JSONObject().put(metroid.uri, true), metroid))
        // Saved with its shell.
        assertEquals(false, HomeCartLabels.recut(JSONObject().put(vr.uri, "md_vr"), vr))
        assertEquals(true, HomeCartLabels.recut(JSONObject().put(vr.uri, "md"), vr))
        assertEquals(false, HomeCartLabels.recut(JSONObject(), vr))
    }

    @Test
    fun the30SystemsTakeEveryRegionsLabel() {
        assertEquals("jp", HomeCartLabels.labelRegions(rom(Systems.WS, "Gunpey (Japan)")).first())
        assertEquals("us", HomeCartLabels.labelRegions(rom(Systems.GG, "Aladdin (USA)")).first())
        assertEquals("eu", HomeCartLabels.labelRegions(rom(Systems.GG, "Aladdin (Europe)")).first())
        // A western cart still takes the Japanese label when it's the only one.
        assertTrue("jp" in HomeCartLabels.labelRegions(rom(Systems.NGP, "Metal Slug - 1st Mission (World)")))
        assertTrue(Systems.LYNX in HomeCartLabels.LABEL_PLATFORMS)
        assertEquals(listOf("NGP", "NGPC"), Systems.artKeys(Systems.NGP))
        assertEquals(listOf("WS", "WSC"), Systems.artKeys(Systems.WS))
        assertEquals(listOf("GG"), Systems.artKeys(Systems.GG))
    }

    @Test
    fun labelsComeFromTheShellsRegion() {
        assertEquals(listOf("jp"), HomeCartLabels.labelRegions(rom(Systems.NES, "Dig Dug (Japan)")))
        assertEquals("us", HomeCartLabels.labelRegions(rom(Systems.NES, "Metroid (USA)")).first())
        assertEquals("eu", HomeCartLabels.labelRegions(rom(Systems.NES, "Metroid (Europe)")).first())
        assertEquals(listOf("us"), HomeCartLabels.labelRegions(rom(Systems.SNES, "Super Metroid (USA)")))
        assertEquals("jp", HomeCartLabels.labelRegions(rom(Systems.SNES, "Super Metroid (Japan)")).first())
    }

    @Test
    fun theScanWinsThenTheRenderOfTheRightRegion() {
        val json = JSONObject("""{"response":{"jeu":{"medias":[
            {"type":"support-2D","region":"us","url":"https://a/2d-us"},
            {"type":"support-texture","region":"jp","url":"https://a/tex-jp"},
            {"type":"support-texture","region":"us","url":"https://a/tex-us"},
            {"type":"wheel","region":"us","url":"https://a/wheel"}]}}}""")
        val medias = HomeCartLabels.medias(json)
        assertEquals("https://a/tex-us", HomeCartLabels.pick(medias, rom(Systems.SNES, "Game (USA)"))?.url)
        val noScan = medias.filter { it.type != "support-texture" }
        assertEquals("https://a/2d-us", HomeCartLabels.pick(noScan, rom(Systems.SNES, "Game (USA)"))?.url)
        assertNull(HomeCartLabels.pick(noScan, rom(Systems.SNES, "Game (Europe)")))
    }

    @Test
    fun virtuaRacingAndSonicKnucklesLabelsFitTheirWell() {
        HomeCartLabels.readShells(java.io.File("../../crates/slot-ui/assets/home_shells.tsv").readText())
        val json = JSONObject("""{"response":{"jeu":{"medias":[
            {"type":"support-2D","region":"us","url":"https://a/2d-us"},
            {"type":"support-2D","region":"eu","url":"https://a/2d-eu"},
            {"type":"support-2D","region":"wor","url":"https://a/2d-wor"},
            {"type":"support-texture","region":"wor","url":"https://a/tex-wor"},
            {"type":"support-texture","region":"us","url":"https://a/tex-us"},
            {"type":"support-texture","region":"eu","url":"https://a/tex-eu"}]}}}""")
        val medias = HomeCartLabels.medias(json)
        assertEquals("https://a/2d-us", HomeCartLabels.pick(medias, rom(Systems.MD, "Virtua Racing (USA)"))?.url)
        assertEquals("https://a/2d-eu", HomeCartLabels.pick(medias, rom(Systems.MD, "Virtua Racing (Europe)"))?.url)
        assertEquals("https://a/tex-us", HomeCartLabels.pick(medias, rom(Systems.MD, "Sonic & Knuckles (World)"))?.url)
        assertEquals("https://a/tex-eu",
            HomeCartLabels.pick(medias, rom(Systems.MD, "Sonic & Knuckles + Sonic 3 (Europe) (Lock-On)"))?.url)
        val boxOnly = medias.filter { it.type != "support-texture" || it.region == "wor" }
        assertNull(HomeCartLabels.pick(boxOnly, rom(Systems.MD, "Sonic & Knuckles (World)")))
    }

    @Test
    fun newSystemLabelsFollowTheirActualMoulds() {
        HomeCartLabels.readShells(java.io.File("../../crates/slot-ui/assets/home_shells.tsv").readText())
        assertEquals("sms_card", HomeCartLabels.shell(rom(Systems.SMS, "Bomb Jack (Europe)")))
        assertEquals("sms_jp", HomeCartLabels.shell(rom(Systems.SMS, "Hokuto no Ken (Japan)")))
        assertEquals("sms_jp", HomeCartLabels.shell(rom(Systems.SMS, "Super Boy 4 (Korea)")))
        assertEquals("sms", HomeCartLabels.shell(rom(Systems.SMS, "Sonic The Hedgehog (Europe)")))
        assertEquals("gg_codemasters", HomeCartLabels.shell(rom(Systems.GG, "Micro Machines (Europe)")))
        assertEquals("gg", HomeCartLabels.shell(rom(Systems.GG, "Sonic The Hedgehog (USA)")))
        val sms = rom(Systems.SMS, "Sonic The Hedgehog (Europe)")
        assertTrue("jp" !in HomeCartLabels.labelRegions(sms))
        assertTrue("us" !in HomeCartLabels.labelRegions(rom(Systems.SMS, "Hokuto no Ken (Japan)")))
        val gg = rom(Systems.GG, "Micro Machines (Europe)")
        assertTrue(HomeCartLabels.recut(JSONObject().put(gg.uri, ""), gg))
        assertEquals(false, HomeCartLabels.recut(JSONObject().put(gg.uri, "gg_codemasters"), gg))
    }

    @Test
    fun newSystemRenderFallbacksAreAvailableWhenTheSetHasNoStickerScan() {
        val render = HomeCartLabels.Media("support-2D", "us", "https://a/cart")
        val sticker = HomeCartLabels.Media("support-texture", "us", "https://a/label")
        for (platform in listOf(Systems.SMS, Systems.GG, Systems.NGP, Systems.WS, Systems.LYNX, Systems.POKEMINI)) {
            val cart = rom(platform, "Example (USA)")
            assertEquals(render, HomeCartLabels.pick(listOf(render), cart))
            assertEquals(sticker, HomeCartLabels.pick(listOf(render, sticker), cart))
        }
    }

    @Test
    fun checksumsSkipDumpHeaders() {
        val body = ByteArray(40000) { (it * 7).toByte() }
        val ines = byteArrayOf(0x4e, 0x45, 0x53, 0x1a) + ByteArray(12) + body
        val (whole, bare) = HomeCartLabels.checksums(ByteArrayInputStream(ines))
        fun crc(b: ByteArray) = CRC32().apply { update(b) }.value.toString(16).uppercase().padStart(8, '0')
        assertEquals(crc(ines), whole)
        assertEquals(crc(body), bare)
        val plain = ByteArray(0x8000) { it.toByte() }
        assertNull(HomeCartLabels.checksums(ByteArrayInputStream(plain)).second)
        val copier = ByteArray(512) + plain
        assertEquals(crc(plain), HomeCartLabels.checksums(ByteArrayInputStream(copier)).second)
    }
}
