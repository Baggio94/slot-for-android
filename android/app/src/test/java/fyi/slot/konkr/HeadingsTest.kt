package fyi.slot.konkr

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

class HeadingsTest {
    @Test fun everyWindowTitleHasItsPicture() {
        assertEquals("heading-ra.png", Headings.asset("RETROACHIEVEMENTS"))
        assertEquals("heading-save-conflict.png", Headings.asset("SAVE CONFLICT"))
        assertEquals("heading-slot-folder.png", Headings.asset("SLOT. FOLDER"))
        assertEquals("heading-states-already-in-this-folder.png",
            Headings.asset("STATES ALREADY IN THIS FOLDER"))
        // Each title the sources give a window is among the pictures
        // export_apps_legend draws.
        val root = generateSequence(File("").absoluteFile) { it.parentFile }
            .first { File(it, "crates/slot-ui/examples/export_apps_legend.rs").isFile() }
        val exported = File(root, "crates/slot-ui/examples/export_apps_legend.rs").readText()
        val sources = File(root, "android/app/src/main/java/fyi/slot/konkr").listFiles()!!
            .filter { it.name.endsWith(".kt") }.joinToString("\n") { it.readText() }
        val titles = Regex("""SlotModalDialog\(\s*(?:this|activity),\s*"([^"$]+)"""")
            .findAll(sources).map { it.groupValues[1] }.toSet() +
            listOf("APPS", "SAVES ALREADY IN THIS FOLDER", "STATES ALREADY IN THIS FOLDER")
        for (title in titles) {
            val asset = Headings.asset(title)
            assertTrue("$title: $asset is not exported", exported.contains("\"$asset\""))
        }
    }
}
