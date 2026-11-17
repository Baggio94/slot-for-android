package fyi.slot.konkr

import org.junit.Assert.*
import org.junit.Test
import java.util.Base64

/** Independent Python zlib/zstandard fixtures modeled on RetroArch rzip_stream.c,
 * not built by RetroArchCompression.encode(), so decoder interoperability is tested.
 */
class RetroArchCompressionTest {
    private val raw = Base64.getDecoder().decode(
        "UkFTVEFURQFNRU0gBAAAAFRFU1QAAAAARU5EIAAAAAA="
    )
    private val fixture = Base64.getDecoder().decode(
        "I1JaSVB2ASMAAAIAIAAAAAAAAAAkAAAAeJwLcgwOcQxxZfR19VVgYWBgCHENDgFSDK5+LgogGgBsJAVQ"
    )
    // Python zstandard.ZstdCompressor(level=3, write_checksum=True), with
    // b'#RZIPv\x02#' + struct.pack('<IQ', 131072, len(raw)), then each
    // independent frame prefixed with struct.pack('<I', len(frame)).
    private val zstdFixture = Base64.getDecoder().decode(
        "I1JaSVB2AiMAAAIAIAAAAAAAAAAtAAAAKLUv/SQgAQEAUkFTVEFURQFNRU0gBAAAAFRFU1QAAAAARU5EIAAAAAABfWOk"
    )
    // The same payload with write_content_size=False and write_checksum=False.
    private val zstdWithoutContentSize = Base64.getDecoder().decode(
        "I1JaSVB2AiMAAAIAIAAAAAAAAAApAAAAKLUv/QAAAQEAUkFTVEFURQFNRU0gBAAAAFRFU1QAAAAARU5EIAAAAAA="
    )
    // Valid RASTATE: 300,000 MEM bytes, byte[i] = (i * 31 + 7) & 255.
    // Two full 128 KiB chunks followed by a partial chunk, all with checksums.
    private val zstdMultiChunk = Base64.getDecoder().decode(
        "I1JaSVB2AiMAAAIA+JMEAAAAAAArAQAAKLUv/aQAAAIA3QgABBFSQVNUQVRFAU1FTSDgkwQAByZFZIOiweD/Hj1ce5q52PcWNVRz" +
        "krHQ7w4tTGuKqcjnBiVEY4KhwN/+HTxbepm41/YVNFNykbDP7g0sS2qJqMfmBSRDYoGgv979HDtaeZi31vUUM1JxkK/O7QwrSmmI" +
        "p8blBCNCYYCfvt38GzpZeJe21fQTMlFwj67N7AsqSWiHpsXkAyJBYH+evdz7GjlYd5a11PMSMVBvjq3M6wopSGeGpcTjAiFAX36d" +
        "vNv6GThXdpW00/IRME9ujazL6gkoR2aFpMPiASA/Xn2cu9r5GDdWdZSz0vEQL05tjKvK6QgnRmWEo8LhAB8+XXybutn4FzZVdJOy" +
        "0fAPLk1si6rJ6AEAEO3+A/maAiiGlKobAQAAKLUv/aQAAAIAXQgABBAXNlV0k7LR8A8uTWyLqsnoByZFZIOiweD/Hj1ce5q52PcW" +
        "NVRzkrHQ7w4tTGuKqcjnBiVEY4KhwN/+HTxbepm41/YVNFNykbDP7g0sS2qJqMfmBSRDYoGgv979HDtaeZi31vUUM1JxkK/O7Qwr" +
        "SmmIp8blBCNCYYCfvt38GzpZeJe21fQTMlFwj67N7AsqSWiHpsXkAyJBYH+evdz7GjlYd5a11PMSMVBvjq3M6wopSGeGpcTjAiFA" +
        "X36dvNv6GThXdpW00/IRME9ujazL6gkoR2aFpMPiASA/Xn2cu9r5GDdWdZSz0vEQL05tjKvK6QgnRmWEo8LhAB8+XXybutn4AQAA" +
        "/f4D+ZoCgontUCEBAAAotS/9ZPiSnQgAhBAXNlV0k7LR8A8uTWyLqsnoByZFZIOiweD/Hj1ce5q52PcWNVRzkrHQ7w4tTGuKqcjn" +
        "BiVEY4KhwN/+HTxbepm41/YVNFNykbDP7g0sS2qJqMfmBSRDYoGgv979HDtaeZi31vUUM1JxkK/O7QwrSmmIp8blBCNCYYCfvt38" +
        "GzpZeJe21fQTMlFwj67N7AsqSWiHpsXkAyJBYH+evdz7GjlYd5a11PMSMVBvjq3M6wopSGeGpcTjAiFAX36dvNv6GThXdpW00/IR" +
        "ME9ujazL6gkoR2aFpMPiASA/Xn2cu9r5GDdWdZSz0vEQL05tjKvK6QgnRmWEo8LhAB8+XXybutn4RU5EIAAAAAABAADtkgF9TQH4" +
        "2Vq4"
    )

    @Test fun independentRzipV1FixtureDecodes() {
        assertTrue(RetroArchCompression.isRzip(fixture))
        assertArrayEquals(raw, RetroArchCompression.decode(fixture))
    }

    @Test fun uncompressedStatesAreUnchanged() {
        assertSame(raw, RetroArchCompression.decode(raw))
    }

    @Test fun independentRzipV2FixturesDecodeAndCanBeExportedAsV1() {
        for (fixture in listOf(zstdFixture, zstdWithoutContentSize)) {
            val original = fixture.copyOf()
            assertTrue(RetroArchCompression.isRzip(fixture))
            val decoded = RetroArchCompression.decode(fixture)
            assertArrayEquals(raw, decoded)
            RetroArchCompression.requireSupportedContainer(fixture)
            val exported = RetroArchCompression.encode(decoded)
            assertEquals(1, exported[6].toInt())
            assertArrayEquals(raw, RetroArchCompression.decode(exported))
            assertArrayEquals(original, fixture)
        }
    }

    @Test fun independentZstdFramesDecodeAcrossRzipChunkBoundaries() {
        val header = raw.copyOfRange(0, 16)
        for (i in 0..3) header[12 + i] = (300_000 ushr (8 * i)).toByte()
        val expected = header + ByteArray(300_000) { (it * 31 + 7).toByte() } +
            raw.copyOfRange(24, 32)
        assertArrayEquals(expected, RetroArchCompression.decode(zstdMultiChunk))
        RetroArchCompression.requireSupportedContainer(zstdMultiChunk)
    }

    @Test fun corruptOrTruncatedZstdNeverPassesOverwriteValidation() {
        val corruptChecksum = zstdFixture.copyOf().apply {
            this[lastIndex] = (this[lastIndex].toInt() xor 1).toByte()
        }
        for (bad in listOf(
            corruptChecksum,
            zstdFixture.copyOf(zstdFixture.size - 1),
            zstdMultiChunk.copyOf(zstdMultiChunk.size - 1),
            zstdFixture + byteArrayOf(1),
            fixture.copyOf().apply { this[6] = 2 },
            zstdFixture.copyOf().apply { this[6] = 1 }
        )) {
            val original = bad.copyOf()
            assertThrows(Exception::class.java) { RetroArchCompression.requireSupportedContainer(bad) }
            assertArrayEquals(original, bad)
        }
    }

    @Test fun zstdMustMatchTheBoundedRzipSize() {
        // Both too-short and too-long output, and invalid/oversized container
        // sizes, must fail before any external file is eligible for overwrite.
        for ((offset, value) in listOf(
            12 to 31L, 12 to 33L, 12 to 0L, 12 to (65L * 1024 * 1024 + 1),
            16 to 1L, 8 to 0L, 8 to (64L * 1024 * 1024 + 1),
            20 to 0L, 20 to 0xffff_ffffL
        )) {
            val bad = zstdFixture.copyOf()
            for (i in 0..3) bad[offset + i] = (value ushr (8 * i)).toByte()
            assertThrows(IllegalArgumentException::class.java) {
                RetroArchCompression.requireSupportedContainer(bad)
            }
        }
    }

    @Test fun validZstdStillRequiresACompleteRastateContainer() {
        // A valid zstd frame containing RASTATE with MEM but no END block.
        val incomplete = Base64.getDecoder().decode(
            "I1JaSVB2AiMAAAIAGAAAAAAAAAAlAAAAKLUv/SQYwQAAUkFTVEFURQFNRU0gBAAAAFRFU1QAAAAAHtS7YQ=="
        )
        assertArrayEquals(raw.copyOf(24), RetroArchCompression.decode(incomplete))
        assertThrows(IllegalStateException::class.java) {
            RetroArchCompression.requireSupportedContainer(incomplete)
        }
    }

    @Test fun encoderMatchesHeaderLayoutAndRoundTripsMultipleChunks() {
        for (n in listOf(28, 65536, 131072, 131073, 300000)) {
            val state = ByteArray(n) { i -> (i * 31 + 7).toByte() }
            val rzip = RetroArchCompression.encode(state)
            assertArrayEquals(byteArrayOf(35, 82, 90, 73, 80, 118, 1, 35),
                rzip.copyOfRange(0,8))
            assertArrayEquals(state, RetroArchCompression.decode(rzip))
            // Declared RetroArch chunk size is 128 KiB.
            assertEquals(0, rzip[8].toInt() and 255)
            assertEquals(0, rzip[9].toInt() and 255)
            assertEquals(2, rzip[10].toInt() and 255)
            assertEquals(0, rzip[11].toInt() and 255)
        }
    }

    @Test fun existingRastateMustBeCompleteBeforeOverwrite() {
        RetroArchCompression.requireSupportedContainer(raw)
        RetroArchCompression.requireSupportedContainer(fixture)
        RetroArchCompression.requireSupportedContainer(RetroArchCompression.encode(raw))
        assertThrows(IllegalArgumentException::class.java) {
            RetroArchCompression.requireSupportedContainer("unknown legacy state".toByteArray())
        }
        assertThrows(IllegalArgumentException::class.java) {
            RetroArchCompression.requireSupportedContainer(byteArrayOf(82,65,83,84,65,84,69,1))
        }
        val futureVersion = fixture.copyOf()
        futureVersion[6] = 3
        assertThrows(IllegalArgumentException::class.java) {
            RetroArchCompression.requireSupportedContainer(futureVersion)
        }
        val truncated = raw.copyOfRange(0, raw.size - 8)
        assertThrows(IllegalStateException::class.java) {
            RetroArchCompression.requireSupportedContainer(truncated)
        }
    }

    @Test fun optionalAchievementBlockIsCompatible() {
        val state = raw.copyOfRange(0, raw.size - 8) +
            byteArrayOf(65,67,72,86,76,0,0,0) +
            ByteArray(76) + ByteArray(4) +
            raw.copyOfRange(raw.size - 8, raw.size)
        RetroArchCompression.requireSupportedContainer(state)
        val zipped = RetroArchCompression.encode(state)
        assertArrayEquals(state, RetroArchCompression.decode(zipped))
        RetroArchCompression.requireSupportedContainer(zipped)
        assertThrows(IllegalArgumentException::class.java) {
            RetroArchCompression.requireSupportedContainer(state + byteArrayOf(1))
        }
    }

    @Test fun corruptTruncatedAndFutureVersionAreNeverAccepted() {
        assertThrows(IllegalArgumentException::class.java) {
            RetroArchCompression.decode(fixture.copyOfRange(0, 22))
        }
        val futureVersion = fixture.copyOf()
        futureVersion[6] = 3
        assertThrows(IllegalArgumentException::class.java) {
            RetroArchCompression.decode(futureVersion)
        }
        val badSize = fixture.copyOf()
        badSize[8] = 0
        badSize[9] = 0
        badSize[10] = 0
        badSize[11] = 0
        assertThrows(IllegalArgumentException::class.java) {
            RetroArchCompression.decode(badSize)
        }
    }
}
