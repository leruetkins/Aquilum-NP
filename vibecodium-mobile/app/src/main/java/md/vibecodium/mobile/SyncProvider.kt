package md.vibecodium.mobile

import android.content.Context
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.io.IOException
import java.net.HttpURLConnection
import java.net.InetAddress
import java.net.ServerSocket
import java.net.URI
import java.net.URL
import java.net.URLDecoder
import java.net.URLEncoder
import java.security.MessageDigest
import java.security.SecureRandom
import java.util.Base64
import java.util.Locale

data class RemoteEntry(
    val path: String,
    val rev: String,
    val contentHash: String,
    val size: Long,
)

data class RemoteDownload(
    val bytes: ByteArray,
    val rev: String,
    val contentHash: String,
)

class DropboxException(val status: Int, val summary: String, val endpoint: String = "") :
    IOException("Dropbox $status${if (endpoint.isNotBlank()) " ($endpoint)" else ""}: $summary")

data class RemoteDelta(
    val changed: List<RemoteEntry>,
    val removed: List<String>,
    val cursor: String,
)

interface SyncProvider {
    suspend fun accountName(): String
    suspend fun ensureFolder(path: String)
    suspend fun listFolder(root: String): List<RemoteEntry>
    /** Cursor representing the current state of [root], used before a full list. */
    suspend fun latestCursor(root: String): String
    /** Changes since [cursor]; null means the cursor is stale and a full list is required. */
    suspend fun delta(root: String, cursor: String): RemoteDelta?
    /** Current metadata for [path]; used to repair an upload response without rev. */
    suspend fun metadata(path: String): RemoteEntry?
    suspend fun download(path: String): RemoteDownload
    suspend fun upload(path: String, bytes: ByteArray, rev: String?): RemoteEntry
    suspend fun delete(path: String)
}

/** OAuth refresh token and access token for the selected Dropbox app. */
class DropboxTokenStore(context: Context) {
    private val prefs = context.getSharedPreferences("vibecodium", Context.MODE_PRIVATE)

    var clientId: String
        get() = prefs.getString(KEY_CLIENT_ID, DEFAULT_CLIENT_ID)?.ifBlank { DEFAULT_CLIENT_ID }
            ?: DEFAULT_CLIENT_ID
        set(value) {
            prefs.edit().putString(KEY_CLIENT_ID, value.trim().ifBlank { DEFAULT_CLIENT_ID }).apply()
        }

    var accessToken: String
        get() = prefs.getString(KEY_ACCESS, "").orEmpty()
        private set(value) = prefs.edit().putString(KEY_ACCESS, value).apply()

    var refreshToken: String
        get() = prefs.getString(KEY_REFRESH, "").orEmpty()
        private set(value) = prefs.edit().putString(KEY_REFRESH, value).apply()

    var accessExpires: Long
        get() = prefs.getLong(KEY_EXPIRES, 0L)
        private set(value) = prefs.edit().putLong(KEY_EXPIRES, value).apply()

    var accountName: String
        get() = prefs.getString(KEY_ACCOUNT, "").orEmpty()
        private set(value) = prefs.edit().putString(KEY_ACCOUNT, value).apply()

    val connected: Boolean get() = refreshToken.isNotBlank()

    fun save(tokens: DropboxAuth.Tokens, account: String? = null) {
        accessToken = tokens.access
        refreshToken = tokens.refresh
        accessExpires = tokens.expiresAt
        if (account != null) accountName = account
    }

    fun saveAccount(name: String) {
        accountName = name
    }

    fun clear() {
        prefs.edit()
            .remove(KEY_ACCESS)
            .remove(KEY_REFRESH)
            .remove(KEY_EXPIRES)
            .remove(KEY_ACCOUNT)
            .apply()
    }

    private companion object {
        const val DEFAULT_CLIENT_ID = "hqch0u08opkzvpn"
        const val KEY_CLIENT_ID = "dropbox_client_id"
        const val KEY_ACCESS = "dropbox_access"
        const val KEY_REFRESH = "dropbox_refresh"
        const val KEY_EXPIRES = "dropbox_expires"
        const val KEY_ACCOUNT = "dropbox_account"
    }
}

object DropboxAuth {
    private const val AUTHORIZE = "https://www.dropbox.com/oauth2/authorize"
    private const val TOKEN = "https://api.dropboxapi.com/oauth2/token"
    private const val REDIRECT = "http://127.0.0.1:47219/"

    data class Pkce(val verifier: String, val challenge: String, val state: String)
    data class Tokens(val access: String, val refresh: String, val expiresAt: Long)

    fun newPkce(): Pkce {
        val random = ByteArray(48).also(SecureRandom()::nextBytes)
        val verifier = Base64.getUrlEncoder().withoutPadding().encodeToString(random)
        val challenge = Base64.getUrlEncoder().withoutPadding().encodeToString(
            MessageDigest.getInstance("SHA-256").digest(verifier.toByteArray(Charsets.US_ASCII)),
        )
        val state = Base64.getUrlEncoder().withoutPadding()
            .encodeToString(ByteArray(16).also(SecureRandom()::nextBytes))
        return Pkce(verifier, challenge, state)
    }

    fun authUrl(clientId: String, pkce: Pkce): String {
        val query = listOf(
            "client_id" to clientId,
            "response_type" to "code",
            "redirect_uri" to REDIRECT,
            "code_challenge" to pkce.challenge,
            "code_challenge_method" to "S256",
            "state" to pkce.state,
            "token_access_type" to "offline",
        ).joinToString("&") { (key, value) ->
            "${encode(key)}=${encode(value)}"
        }
        return "$AUTHORIZE?$query"
    }

    suspend fun exchange(clientId: String, code: String, verifier: String): Tokens =
        request(
            listOf(
                "grant_type" to "authorization_code",
                "code" to code,
                "code_verifier" to verifier,
                "redirect_uri" to REDIRECT,
                "client_id" to clientId,
            ),
        )

    suspend fun refresh(clientId: String, refreshToken: String): Tokens =
        request(
            listOf(
                "grant_type" to "refresh_token",
                "refresh_token" to refreshToken,
                "client_id" to clientId,
            ),
            previousRefresh = refreshToken,
        )

    private suspend fun request(
        form: List<Pair<String, String>>,
        previousRefresh: String = "",
    ): Tokens = withContext(Dispatchers.IO) {
        val body = form.joinToString("&") { (key, value) -> "${encode(key)}=${encode(value)}" }
            .toByteArray(Charsets.UTF_8)
        var retries = 0
        while (true) {
            val connection = (URL(TOKEN).openConnection() as HttpURLConnection).apply {
                requestMethod = "POST"
                connectTimeout = 30_000
                readTimeout = 60_000
                doOutput = true
                setFixedLengthStreamingMode(body.size)
                setRequestProperty("Content-Type", "application/x-www-form-urlencoded")
            }
            connection.outputStream.use { it.write(body) }
            val status = connection.responseCode
            val text = (if (status in 200..299) connection.inputStream else connection.errorStream)
                ?.bufferedReader()?.use { it.readText() }.orEmpty()
            if ((status == 429 || status in 500..599) && retries < 2) {
                retries += 1
                delay(1_000L * retries)
                continue
            }
            if (status !in 200..299) {
                throw DropboxException(status, summarize(text), "oauth2/token")
            }
            val json = JSONObject(text)
            val access = json.optString("access_token")
            if (access.isBlank()) {
                throw DropboxException(status, "Dropbox returned no access token", "oauth2/token")
            }
            val refresh = json.optString("refresh_token").ifBlank { previousRefresh }
            val expires = System.currentTimeMillis() + json.optLong("expires_in", 14_400L) * 1000L
            return@withContext Tokens(access, refresh, expires)
        }
        error("unreachable")
    }

    private fun encode(value: String) = URLEncoder.encode(value, "UTF-8")
}

/**
 * One-shot loopback server for the OAuth redirect. The Dropbox app already
 * registers `http://127.0.0.1:47219/` (the same URI the desktop app uses).
 */
class LoopbackServer {
    private var socket: ServerSocket? = null

    fun start() {
        socket = ServerSocket(47219, 1, InetAddress.getByName("127.0.0.1")).apply {
            reuseAddress = true
        }
    }

    suspend fun awaitCode(expectedState: String, timeoutMs: Long): String = withContext(Dispatchers.IO) {
        val server = socket ?: error("Loopback server is not started")
        server.soTimeout = timeoutMs.toInt()
        try {
            server.accept().use { client ->
                val reader = client.getInputStream().bufferedReader()
                val requestLine = reader.readLine() ?: error("Empty OAuth request")
                val target = requestLine.split(' ').getOrNull(1).orEmpty()
                val query = target.substringAfter('?', "")
                val params = parseQuery(query)
                val failure = params["error_description"] ?: params["error"]
                val state = params["state"]
                val code = params["code"]
                val ok = failure == null && state == expectedState && code != null
                val detail = when {
                    failure != null -> failure
                    state != expectedState -> "state mismatch"
                    code == null -> "no authorization code"
                    else -> "You can close this window and return to vibecodium."
                }.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")
                val body = """
                    <!doctype html><html><head><meta charset="utf-8"><title>vibecodium</title></head>
                    <body style="font-family:sans-serif;background:#1e1e1e;color:#dadada;text-align:center;padding-top:20vh">
                    <h2>${if (ok) "Dropbox sign-in received" else "Dropbox sign-in failed"}</h2><p>$detail</p>
                    </body></html>
                """.trimIndent().toByteArray(Charsets.UTF_8)
                val response = buildString {
                    append("HTTP/1.1 200 OK\r\n")
                    append("Content-Type: text/html; charset=utf-8\r\n")
                    append("Content-Length: ${body.size}\r\n")
                    append("Connection: close\r\n\r\n")
                }.toByteArray(Charsets.UTF_8)
                client.getOutputStream().use { output ->
                    output.write(response)
                    output.write(body)
                    output.flush()
                }
                if (!ok) {
                    if (failure != null) error("Dropbox sign-in failed: $failure")
                    if (state != expectedState) error("Dropbox sign-in state mismatch")
                    error("Dropbox returned no authorization code")
                }
                code ?: error("Dropbox returned no authorization code")
            }
        } finally {
            close()
        }
    }

    fun close() {
        runCatching { socket?.close() }
        socket = null
    }

    private fun parseQuery(query: String): Map<String, String> =
        query.split('&')
            .filter { it.isNotBlank() }
            .mapNotNull { pair ->
                val index = pair.indexOf('=')
                if (index <= 0) null
                else URLDecoder.decode(pair.substring(0, index), "UTF-8") to
                    URLDecoder.decode(pair.substring(index + 1), "UTF-8")
            }
            .toMap()
}

class DropboxProvider(private val tokens: DropboxTokenStore) : SyncProvider {
    override suspend fun accountName(): String =
        tokens.accountName.ifBlank { "Dropbox" }

    override suspend fun ensureFolder(path: String) {
        val normalized = path.trim().trimEnd('/')
        if (normalized.isEmpty()) return
        try {
            rpc(
                "files/create_folder_v2",
                JSONObject().put("path", normalized),
            )
        } catch (error: DropboxException) {
            if (!error.summary.contains("conflict", ignoreCase = true)) throw error
        }
    }

    override suspend fun listFolder(root: String): List<RemoteEntry> = withContext(Dispatchers.IO) {
        val normalizedRoot = root.trimEnd('/').ifEmpty { "" }
        var response = try {
            rpc(
                "files/list_folder",
                JSONObject()
                    .put("path", normalizedRoot)
                    .put("recursive", true),
            )
        } catch (error: DropboxException) {
            if (error.summary.contains("path/not_found")) return@withContext emptyList()
            throw error
        }

        val entries = mutableListOf<RemoteEntry>()
        while (true) {
            val array = response.optJSONArray("entries") ?: JSONArray()
            for (index in 0 until array.length()) {
                val entry = array.optJSONObject(index) ?: continue
                if (entry.optString(".tag") != "file") continue
                val relative = relativePath(entry.optString("path_display"), normalizedRoot) ?: continue
                entries.add(remoteEntry(relative, entry))
            }
            if (!response.optBoolean("has_more")) break
            response = rpc(
                "files/list_folder/continue",
                JSONObject().put("cursor", response.optString("cursor")),
            )
        }
        entries.sortedBy { it.path.lowercase() }
    }

    override suspend fun latestCursor(root: String): String = withContext(Dispatchers.IO) {
        val normalizedRoot = root.trimEnd('/').ifEmpty { "" }
        val argument = JSONObject()
            .put("path", normalizedRoot)
            .put("recursive", true)
        suspend fun latest(): String =
            rpc("files/list_folder/get_latest_cursor", argument).optString("cursor")
        try {
            latest()
        } catch (error: DropboxException) {
            if (!error.summary.contains("path/not_found")) throw error
            ensureFolder(normalizedRoot)
            latest()
        }
    }

    override suspend fun delta(root: String, cursor: String): RemoteDelta? = withContext(Dispatchers.IO) {
        val normalizedRoot = root.trimEnd('/').ifEmpty { "" }
        var response = try {
            rpc("files/list_folder/continue", JSONObject().put("cursor", cursor))
        } catch (error: DropboxException) {
            // A reset or a deleted folder invalidates the cursor; the caller
            // falls back to a full listing.
            if (error.status == 409 || error.status == 410) return@withContext null
            throw error
        }
        val changed = linkedMapOf<String, RemoteEntry>()
        val removed = linkedSetOf<String>()
        var next = response.optString("cursor", cursor)
        while (true) {
            val array = response.optJSONArray("entries") ?: JSONArray()
            for (index in 0 until array.length()) {
                val entry = array.optJSONObject(index) ?: continue
                when (entry.optString(".tag")) {
                    "file" -> {
                        val relative = relativePath(entry.optString("path_display"), normalizedRoot)
                        if (relative != null) changed[relative] = remoteEntry(relative, entry)
                    }

                    "deleted" -> {
                        val display = entry.optString("path_display")
                        val relative = relativePath(display, normalizedRoot)
                        if (relative != null) {
                            removed.add(relative)
                        } else if (display.trimEnd('/').equals(normalizedRoot, ignoreCase = true)) {
                            // The remote folder itself disappeared. Force a
                            // full listing; the engine aborts if records exist.
                            return@withContext null
                        }
                    }
                }
            }
            next = response.optString("cursor", next)
            if (!response.optBoolean("has_more")) break
            response = rpc("files/list_folder/continue", JSONObject().put("cursor", next))
        }
        RemoteDelta(changed.values.toList(), removed.toList(), next)
    }

    private fun remoteEntry(relative: String, entry: JSONObject) = RemoteEntry(
        path = relative,
        rev = entry.optString("rev"),
        contentHash = entry.optString("content_hash"),
        size = entry.optLong("size"),
    )

    private fun relativePath(display: String, root: String): String? {
        if (display.isBlank()) return null
        val rootParts = root.trim('/').split('/').filter { it.isNotEmpty() }
        val parts = display.trimStart('/').split('/')
        if (parts.size <= rootParts.size) return null
        val underRoot = rootParts.indices.all { index ->
            parts[index].equals(rootParts[index], ignoreCase = true)
        }
        if (!underRoot) return null
        val relative = parts.drop(rootParts.size).joinToString("/")
        if (relative.isEmpty() || relative.split('/').any { it.isEmpty() || it == ".." }) return null
        return relative
    }

    override suspend fun download(path: String): RemoteDownload = withContext(Dispatchers.IO) {
        val response = content("files/download", JSONObject().put("path", path), null)
        val result = response.result
        RemoteDownload(
            bytes = response.bytes,
            rev = result?.optString("rev").orEmpty(),
            contentHash = result?.optString("content_hash").orEmpty(),
        )
    }

    override suspend fun metadata(path: String): RemoteEntry? = withContext(Dispatchers.IO) {
        val value = try {
            rpc("files/get_metadata", JSONObject().put("path", path))
        } catch (error: DropboxException) {
            if (error.summary.contains("path/not_found")) return@withContext null
            throw error
        }
        RemoteEntry(
            path = path,
            rev = value.optString("rev"),
            contentHash = value.optString("content_hash"),
            size = value.optLong("size"),
        )
    }

    override suspend fun upload(path: String, bytes: ByteArray, rev: String?): RemoteEntry =
        withContext(Dispatchers.IO) {
            ensureParents(path)
            val mode = if (rev.isNullOrBlank()) {
                "add"
            } else {
                JSONObject().put(".tag", "update").put("update", rev)
            }
            val arg = JSONObject()
                .put("path", path)
                .put("mode", mode)
                .put("autorename", false)
                .put("strict_conflict", true)
                .put("mute", true)
            val response = content("files/upload", arg, bytes)
            val result = response.result
            RemoteEntry(
                path = path,
                rev = result?.optString("rev").orEmpty(),
                contentHash = result?.optString("content_hash").orEmpty(),
                size = result?.optLong("size") ?: bytes.size.toLong(),
            )
        }

    override suspend fun delete(path: String) {
        try {
            rpc("files/delete_v2", JSONObject().put("path", path))
        } catch (error: DropboxException) {
            if (!error.summary.contains("path/not_found")) throw error
        }
    }

    private suspend fun ensureParents(path: String) {
        val parts = path.trim('/').split('/').dropLast(1)
        var current = ""
        for (part in parts) {
            current += "/$part"
            ensureFolder(current)
        }
    }

    private suspend fun rpc(endpoint: String, body: JSONObject?): JSONObject =
        withContext(Dispatchers.IO) {
            // No-argument RPCs expect `null`, not an empty body.
            val payload = (body?.toString() ?: "null").toByteArray(Charsets.UTF_8)
            var refreshed = false
            var retries = 0
            while (true) {
                val connection = (URL("https://api.dropboxapi.com/2/$endpoint").openConnection() as HttpURLConnection).apply {
                    requestMethod = "POST"
                    connectTimeout = 30_000
                    readTimeout = 60_000
                    doOutput = true
                    setFixedLengthStreamingMode(payload.size)
                    setRequestProperty("Authorization", "Bearer ${accessToken()}")
                    setRequestProperty("Content-Type", "application/json")
                }
                connection.outputStream.use { it.write(payload) }
                val status = connection.responseCode
                val text = (if (status in 200..299) connection.inputStream else connection.errorStream)
                    ?.bufferedReader()?.use { it.readText() }.orEmpty()
                if (status == 401 && !refreshed) {
                    refreshed = true
                    forceRefresh()
                    continue
                }
                if ((status == 429 || status in 500..599) && retries < 2) {
                    retries += 1
                    delay(1_000L * retries)
                    continue
                }
                if (status !in 200..299) throw DropboxException(status, summarize(text), endpoint)
                return@withContext if (text.isBlank()) JSONObject() else JSONObject(text)
            }
            error("unreachable")
        }

    private suspend fun content(endpoint: String, arg: JSONObject, body: ByteArray?): ContentResponse =
        withContext(Dispatchers.IO) {
            var refreshed = false
            var retries = 0
            while (true) {
                val connection = (URL("https://content.dropboxapi.com/2/$endpoint").openConnection() as HttpURLConnection).apply {
                    requestMethod = "POST"
                    connectTimeout = 30_000
                    readTimeout = 180_000
                    doOutput = true
                    setFixedLengthStreamingMode(body?.size ?: 0)
                    setRequestProperty("Authorization", "Bearer ${accessToken()}")
                    // HTTP headers are ASCII; Dropbox requires non-ASCII path
                    // components to be escaped in this JSON header.
                    setRequestProperty("Dropbox-API-Arg", asciiJson(arg))
                    setRequestProperty("Content-Type", "application/octet-stream")
                }
                connection.outputStream.use { output ->
                    if (body != null) output.write(body)
                }
                val status = connection.responseCode
                if (status == 401 && !refreshed) {
                    refreshed = true
                    forceRefresh()
                    continue
                }
                if ((status == 429 || status in 500..599) && retries < 2) {
                    retries += 1
                    delay(1_000L * retries)
                    continue
                }
                if (status !in 200..299) {
                    val text = connection.errorStream?.bufferedReader()?.use { it.readText() }.orEmpty()
                    throw DropboxException(status, summarize(text), endpoint)
                }
                val bytes = connection.inputStream.use { it.readBytes() }
                val result = connection.getHeaderField("Dropbox-API-Result")
                    ?.let { runCatching { JSONObject(it) }.getOrNull() }
                return@withContext ContentResponse(bytes, result)
            }
            error("unreachable")
        }

    private fun asciiJson(value: JSONObject): String {
        val text = value.toString()
        val output = StringBuilder(text.length)
        for (unit in text) {
            if (unit.code < 0x7f) {
                output.append(unit)
            } else {
                output.append("\\u").append(String.format(Locale.US, "%04x", unit.code))
            }
        }
        return output.toString()
    }

    private suspend fun accessToken(): String {
        val access = tokens.accessToken
        if (access.isNotBlank() && tokens.accessExpires > System.currentTimeMillis() + 60_000) {
            return access
        }
        return forceRefresh()
    }

    private suspend fun forceRefresh(): String {
        val refresh = tokens.refreshToken
        if (refresh.isBlank()) throw DropboxException(401, "Dropbox is not connected")
        val updated = DropboxAuth.refresh(tokens.clientId, refresh)
        tokens.save(updated)
        return updated.access
    }

    private data class ContentResponse(val bytes: ByteArray, val result: JSONObject?)
}

internal fun summarizeDropboxError(text: String): String = summarize(text)

private fun summarize(text: String): String {
    if (text.isBlank()) return "empty response"
    return runCatching {
        val json = JSONObject(text)
        json.optString("error_summary").ifBlank { json.optString("error_description") }
    }.getOrDefault("").ifBlank { text.take(300) }
}
