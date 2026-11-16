package md.vibecodium.mobile.ui

import android.graphics.BitmapFactory
import android.util.LruCache
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import md.vibecodium.mobile.ThemeEntry
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder

/** Catalog screenshot for one theme, loaded lazily and cached in memory. */
@Composable
fun ThemeThumbnail(entry: ThemeEntry, modifier: Modifier = Modifier) {
    val url = remember(entry.repo, entry.screenshot) { ThemePreviewLoader.url(entry) }
    var image by remember(url) { mutableStateOf<ImageBitmap?>(null) }
    var unavailable by remember(url) { mutableStateOf(false) }

    LaunchedEffect(url) {
        if (url == null) {
            unavailable = true
            return@LaunchedEffect
        }
        val loaded = ThemePreviewLoader.load(url)
        image = loaded
        unavailable = loaded == null
    }

    Box(
        modifier
            .clip(RoundedCornerShape(8.dp))
            .background(MaterialTheme.colorScheme.surfaceVariant),
        contentAlignment = Alignment.Center,
    ) {
        val bitmap = image
        if (bitmap != null) {
            Image(
                bitmap = bitmap,
                contentDescription = "${entry.name} preview",
                contentScale = ContentScale.Crop,
                modifier = Modifier.fillMaxSize(),
            )
        } else {
            Text(
                if (unavailable) "No preview" else "…",
                fontSize = 9.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

private object ThemePreviewLoader {
    private val cache = LruCache<String, ImageBitmap>(32)

    fun url(entry: ThemeEntry): String? {
        if (entry.repo.isBlank() || entry.screenshot.isBlank()) return null
        return "https://raw.githubusercontent.com/${entry.repo}/HEAD/${entry.screenshot}"
    }

    suspend fun load(url: String): ImageBitmap? = withContext(Dispatchers.IO) {
        cache.get(url)?.let { return@withContext it }
        val bytes = fetch(url) ?: fetch(proxy(url)) ?: return@withContext null
        val bitmap = decode(bytes) ?: return@withContext null
        val image = bitmap.asImageBitmap()
        cache.put(url, image)
        image
    }

    private fun fetch(url: String): ByteArray? {
        val connection = runCatching { URL(url).openConnection() as HttpURLConnection }.getOrNull()
            ?: return null
        return try {
            connection.connectTimeout = 12_000
            connection.readTimeout = 20_000
            connection.instanceFollowRedirects = true
            connection.setRequestProperty("User-Agent", "Vibecodium/0.1")
            connection.setRequestProperty("Accept-Encoding", "identity")
            if (connection.responseCode !in 200..299) return null
            connection.inputStream.use { it.readBytes() }
        } catch (_: Throwable) {
            null
        } finally {
            connection.disconnect()
        }
    }

    private fun proxy(url: String): String =
        "https://releases.obsidian.md/proxy?url=" + URLEncoder.encode(url, "UTF-8")

    private fun decode(bytes: ByteArray): android.graphics.Bitmap? {
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
        if (bounds.outWidth <= 0 || bounds.outHeight <= 0) return null
        var sample = 1
        while (bounds.outWidth / sample > 480 || bounds.outHeight / sample > 320) {
            sample *= 2
        }
        val options = BitmapFactory.Options().apply { inSampleSize = sample }
        return BitmapFactory.decodeByteArray(bytes, 0, bytes.size, options)
    }
}
