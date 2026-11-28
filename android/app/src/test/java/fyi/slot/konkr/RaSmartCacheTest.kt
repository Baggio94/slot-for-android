package fyi.slot.konkr

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Stable SAF metadata may reuse the last uncompressed ROM MD5. */
class RaSmartCacheTest {
    private val valid = RaSmartCache.Fingerprint(
        size = 16L * 1024 * 1024,
        modified = 123456789L,
        md5 = "abcdef0123456789abcdef0123456789"
    )

    @Test fun neoGeoSetsAreKnownByTheirName() {
        // md5("mslug"), as rcheevos hashes an Arcade game.
        assertEquals("b43c8b4ec999588c04dad79bb8bcc745", RaSmartCache.arcadeHash("mslug.zip"))
        assertEquals(RaSmartCache.arcadeHash("mslug.zip"), RaSmartCache.arcadeHash("roms/mslug.ZIP"))
        assertNull(RaSmartCache.arcadeHash("mslug.neo"))
    }

    @Test fun aGeolithNeoIsHashedAfterItsHeader() {
        val body = ByteArray(0x4000) { (it * 7).toByte() }
        val neo = ByteArray(4096 + body.size)
        "NEO".toByteArray().copyInto(neo); neo[3] = 1
        for (i in 4 until 4096) neo[i] = 0x55
        body.copyInto(neo, 4096)
        val expected = java.security.MessageDigest.getInstance("MD5").digest(body)
            .joinToString("") { "%02x".format(it.toInt() and 0xff) }
        assertEquals(expected, RaSmartCache.neoHash(java.io.ByteArrayInputStream(neo)))
        try {
            RaSmartCache.neoHash(java.io.ByteArrayInputStream(body))
            throw AssertionError("A file without the NEO magic was hashed")
        } catch (_: IllegalArgumentException) { }
    }

    @Test fun unchangedFileReusesMd5() {
        assertTrue(RaSmartCache.canReuse(valid, valid.size, valid.modified))
    }

    @Test fun changedRomMustBeRehashed() {
        assertFalse(RaSmartCache.canReuse(valid, valid.size + 1, valid.modified))
        assertFalse(RaSmartCache.canReuse(valid, valid.size, valid.modified + 1))
        assertFalse(RaSmartCache.canReuse(valid, 0, valid.modified))
        assertFalse(RaSmartCache.canReuse(valid, valid.size, -1))
        assertFalse(RaSmartCache.canReuse(null, valid.size, valid.modified))
    }

    @Test fun cachedMd5MustBeCanonicalHex() {
        assertFalse(RaSmartCache.canReuse(valid.copy(md5 = "bad"), valid.size, valid.modified))
        assertFalse(RaSmartCache.canReuse(valid.copy(md5 = "z".repeat(32)),
            valid.size, valid.modified))
        assertFalse(RaSmartCache.canReuse(valid.copy(md5 = valid.md5.uppercase()),
            valid.size, valid.modified))
    }
}
