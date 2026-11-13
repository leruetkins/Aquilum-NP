package md.vibecodium.mobile

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class DropboxErrorsTest {
    @Test
    fun deleteMissingPathCountsAsMissing() {
        // files/delete_v2 uses path_lookup/not_found for a target that is
        // already gone; reads use path/not_found. Both mean "missing".
        assertTrue(DropboxException(409, "path_lookup/not_found/", "files/delete_v2").isMissing())
        assertTrue(DropboxException(409, "path/not_found/", "files/get_metadata").isMissing())
    }

    @Test
    fun conflictsAndAuthErrorsAreNotMissing() {
        assertFalse(DropboxException(409, "path/conflict/file/", "files/upload").isMissing())
        assertFalse(DropboxException(401, "invalid_access_token/", "files/delete_v2").isMissing())
        assertFalse(DropboxException(409, "too_many_write_operations/", "files/delete_v2").isMissing())
    }
}
