package md.vibecodium.mobile

import java.time.LocalDateTime
import java.time.format.TextStyle
import java.util.Locale

/**
 * Expands `{{TOKEN}}` date/time placeholders in the daily-note name format.
 *
 * This mirrors `aquilum-gpui/src/settings.rs::expand_daily_note_format`:
 * unknown placeholders and an unclosed `{{` are kept verbatim.
 */
fun expandDailyNoteFormat(format: String, now: LocalDateTime): String {
    val out = StringBuilder(format.length)
    var rest = format
    while (true) {
        val start = rest.indexOf("{{")
        if (start < 0) {
            out.append(rest)
            break
        }
        out.append(rest, 0, start)
        val after = rest.substring(start + 2)
        val end = after.indexOf("}}")
        if (end < 0) {
            out.append(rest, start, rest.length)
            break
        }
        val token = after.substring(0, end)
        val value = dailyNoteTokenValue(token, now)
        if (value == null) {
            out.append("{{").append(token).append("}}")
        } else {
            out.append(value)
        }
        rest = after.substring(end + 2)
    }
    return out.toString()
}

private fun dailyNoteTokenValue(token: String, now: LocalDateTime): String? = when (token) {
    "YYYY" -> String.format(Locale.ROOT, "%04d", now.year)
    "YY" -> String.format(Locale.ROOT, "%02d", Math.floorMod(now.year, 100))
    "MM" -> String.format(Locale.ROOT, "%02d", now.monthValue)
    "M" -> now.monthValue.toString()
    "MMMM" -> now.month.getDisplayName(TextStyle.FULL, Locale.ENGLISH)
    "MMM" -> now.month.getDisplayName(TextStyle.SHORT, Locale.ENGLISH)
    "DD" -> String.format(Locale.ROOT, "%02d", now.dayOfMonth)
    "D" -> now.dayOfMonth.toString()
    "HH" -> String.format(Locale.ROOT, "%02d", now.hour)
    "H" -> now.hour.toString()
    "mm" -> String.format(Locale.ROOT, "%02d", now.minute)
    "m" -> now.minute.toString()
    "ss" -> String.format(Locale.ROOT, "%02d", now.second)
    "s" -> now.second.toString()
    else -> null
}

/**
 * Resolves the daily-note file stem for [now], matching the desktop rules:
 * characters that are not allowed in file names become `-`, a trailing `.md`
 * is dropped, and an empty result falls back to the ISO date.
 */
fun dailyNoteFileStem(format: String, now: LocalDateTime): String {
    val expanded = expandDailyNoteFormat(format, now)
    val sanitized = buildString(expanded.length) {
        for (character in expanded) {
            append(
                when {
                    character in "<>:\"/\\|?*" -> '-'
                    character.isISOControl() -> '-'
                    else -> character
                },
            )
        }
    }
    val trimmed = sanitized.trim()
    val withoutExtension = if (trimmed.lowercase(Locale.ROOT).endsWith(".md")) {
        trimmed.dropLast(3)
    } else {
        trimmed
    }
    val stem = withoutExtension.trim().trimEnd('.').trim()
    if (stem.isEmpty()) {
        return String.format(
            Locale.ROOT,
            "%04d-%02d-%02d",
            now.year,
            now.monthValue,
            now.dayOfMonth,
        )
    }
    return stem
}

/**
 * Normalizes a vault-relative daily-note folder the same way the desktop does:
 * `.`/`..` and empty segments are skipped, so the note cannot leave the vault.
 */
fun normalizeDailyNoteFolder(folder: String): String =
    folder
        .split('/', '\\')
        .map { it.trim() }
        .filter { it.isNotEmpty() && it != "." && it != ".." }
        .joinToString("/")

/** Vault-relative path of today's note, e.g. `Daily/2026-11-12.md`. */
fun dailyNotePath(settings: DailyNoteSettings, now: LocalDateTime): String {
    val folder = normalizeDailyNoteFolder(settings.folder)
    val fileName = "${dailyNoteFileStem(settings.format, now)}.md"
    return if (folder.isEmpty()) fileName else "$folder/$fileName"
}
