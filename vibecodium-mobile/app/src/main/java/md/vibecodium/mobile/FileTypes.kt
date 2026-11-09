package md.vibecodium.mobile

/**
 * Language id for a text file the editor can open, or null for markdown notes
 * (which use the markdown editor) and for unsupported/binary files.
 */
fun codeLanguageFor(path: String): String? {
    val name = path.substringAfterLast('/').lowercase()
    if (isMarkdownName(name)) return null
    val extension = name.substringAfterLast('.', "")
    return when (extension) {
        "rs" -> "rust"
        "kt", "kts" -> "kotlin"
        "java" -> "java"
        "js", "mjs", "cjs", "jsx" -> "javascript"
        "ts", "tsx" -> "typescript"
        "py" -> "python"
        "json", "json5" -> "json"
        "toml" -> "toml"
        "yaml", "yml" -> "yaml"
        "sh", "bash", "zsh" -> "bash"
        "c", "h" -> "c"
        "cpp", "cc", "cxx", "hpp", "hh" -> "cpp"
        "cs" -> "csharp"
        "go" -> "go"
        "swift" -> "swift"
        "sql" -> "sql"
        "html", "htm" -> "html"
        "css", "scss", "less" -> "css"
        "xml" -> "xml"
        "lua" -> "lua"
        "rb" -> "ruby"
        "pl" -> "perl"
        "r" -> "r"
        "dockerfile" -> "dockerfile"
        "makefile" -> "makefile"
        else -> when (name) {
            "dockerfile" -> "dockerfile"
            "makefile" -> "makefile"
            else -> null
        }
    }
}

fun isCodePath(path: String): Boolean = codeLanguageFor(path) != null
