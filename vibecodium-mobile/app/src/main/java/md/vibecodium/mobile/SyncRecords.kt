package md.vibecodium.mobile

import android.content.ContentValues
import android.content.Context
import android.database.sqlite.SQLiteDatabase
import android.database.sqlite.SQLiteOpenHelper

/** The last agreed state of one path. */
data class SyncRecord(
    val path: String,
    val localHash: String,
    val localSize: Long,
    val localModified: Long,
    val remoteRev: String,
    val remoteHash: String,
    val updatedAt: Long,
)

data class SyncState(
    val lastSync: Long,
    val remoteRoot: String,
    val cursor: String = "",
    val scope: String = "",
)

/** Storage contract; the Android implementation is [SyncDatabase]. */
interface SyncRecords {
    fun get(vault: String, path: String): SyncRecord?
    fun all(vault: String): Map<String, SyncRecord>
    fun put(vault: String, record: SyncRecord)
    fun remove(vault: String, path: String)
    fun state(vault: String): SyncState?
    fun saveState(vault: String, state: SyncState)
}

class SyncDatabase(context: Context) :
    SQLiteOpenHelper(context, "sync.db", null, SCHEMA_VERSION),
    SyncRecords {

    override fun onCreate(db: SQLiteDatabase) {
        db.execSQL(
            """
            CREATE TABLE records (
                vault TEXT NOT NULL,
                path TEXT NOT NULL,
                local_hash TEXT NOT NULL,
                local_size INTEGER NOT NULL,
                local_modified INTEGER NOT NULL,
                remote_rev TEXT NOT NULL,
                remote_hash TEXT NOT NULL,
                updated_at INTEGER NOT NULL,
                PRIMARY KEY (vault, path)
            )
            """.trimIndent(),
        )
        db.execSQL(
            """
            CREATE TABLE state (
                vault TEXT PRIMARY KEY,
                last_sync INTEGER NOT NULL,
                remote_root TEXT NOT NULL,
                cursor TEXT NOT NULL DEFAULT '',
                scope TEXT NOT NULL DEFAULT ''
            )
            """.trimIndent(),
        )
    }

    override fun onUpgrade(db: SQLiteDatabase, oldVersion: Int, newVersion: Int) {
        // Keep the records: they are the only local baseline for change
        // detection. Version 1 only lacked the remote cursor and scope.
        if (oldVersion < 2) {
            db.execSQL("ALTER TABLE state ADD COLUMN cursor TEXT NOT NULL DEFAULT ''")
            db.execSQL("ALTER TABLE state ADD COLUMN scope TEXT NOT NULL DEFAULT ''")
        }
    }

    override fun get(vault: String, path: String): SyncRecord? {
        readableDatabase.query(
            "records",
            arrayOf("path", "local_hash", "local_size", "local_modified", "remote_rev", "remote_hash", "updated_at"),
            "vault = ? AND path = ?",
            arrayOf(vault, path),
            null,
            null,
            null,
        ).use { cursor ->
            if (!cursor.moveToFirst()) return null
            return SyncRecord(
                path = cursor.getString(0),
                localHash = cursor.getString(1),
                localSize = cursor.getLong(2),
                localModified = cursor.getLong(3),
                remoteRev = cursor.getString(4),
                remoteHash = cursor.getString(5),
                updatedAt = cursor.getLong(6),
            )
        }
    }

    override fun all(vault: String): Map<String, SyncRecord> {
        val result = LinkedHashMap<String, SyncRecord>()
        readableDatabase.query(
            "records",
            arrayOf("path", "local_hash", "local_size", "local_modified", "remote_rev", "remote_hash", "updated_at"),
            "vault = ?",
            arrayOf(vault),
            null,
            null,
            null,
        ).use { cursor ->
            while (cursor.moveToNext()) {
                val record = SyncRecord(
                    path = cursor.getString(0),
                    localHash = cursor.getString(1),
                    localSize = cursor.getLong(2),
                    localModified = cursor.getLong(3),
                    remoteRev = cursor.getString(4),
                    remoteHash = cursor.getString(5),
                    updatedAt = cursor.getLong(6),
                )
                result[record.path] = record
            }
        }
        return result
    }

    override fun put(vault: String, record: SyncRecord) {
        val values = ContentValues().apply {
            put("vault", vault)
            put("path", record.path)
            put("local_hash", record.localHash)
            put("local_size", record.localSize)
            put("local_modified", record.localModified)
            put("remote_rev", record.remoteRev)
            put("remote_hash", record.remoteHash)
            put("updated_at", record.updatedAt)
        }
        writableDatabase.insertWithOnConflict(
            "records",
            null,
            values,
            SQLiteDatabase.CONFLICT_REPLACE,
        )
    }

    override fun remove(vault: String, path: String) {
        writableDatabase.delete("records", "vault = ? AND path = ?", arrayOf(vault, path))
    }

    override fun state(vault: String): SyncState? {
        readableDatabase.query(
            "state",
            arrayOf("last_sync", "remote_root", "cursor", "scope"),
            "vault = ?",
            arrayOf(vault),
            null,
            null,
            null,
        ).use { cursor ->
            if (!cursor.moveToFirst()) return null
            return SyncState(
                lastSync = cursor.getLong(0),
                remoteRoot = cursor.getString(1),
                cursor = cursor.getString(2).orEmpty(),
                scope = cursor.getString(3).orEmpty(),
            )
        }
    }

    override fun saveState(vault: String, state: SyncState) {
        val values = ContentValues().apply {
            put("vault", vault)
            put("last_sync", state.lastSync)
            put("remote_root", state.remoteRoot)
            put("cursor", state.cursor)
            put("scope", state.scope)
        }
        writableDatabase.insertWithOnConflict(
            "state",
            null,
            values,
            SQLiteDatabase.CONFLICT_REPLACE,
        )
    }

    private companion object {
        const val SCHEMA_VERSION = 2
    }
}
