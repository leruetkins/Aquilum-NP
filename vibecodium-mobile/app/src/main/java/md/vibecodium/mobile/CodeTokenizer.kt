package md.vibecodium.mobile

enum class CodeTokenKind {
    COMMENT,
    KEYWORD,
    STRING,
    NUMBER,
    FUNCTION,
    TYPE,
    PUNCTUATION,
}

data class CodeToken(val start: Int, val end: Int, val kind: CodeTokenKind)

/**
 * A small, dependency-free tokenizer for the common languages used in notes.
 * It is deliberately forgiving: unknown languages still get strings, comments
 * and numbers, and incorrect guesses never change the text.
 */
fun tokenizeCode(code: String, language: String): List<CodeToken> {
    val lang = normalizeLanguage(language)
    val tokens = mutableListOf<CodeToken>()
    var index = 0

    while (index < code.length) {
        val char = code[index]

        if (char == '\n') {
            index += 1
            continue
        }

        val lineComment = lineComment(lang)
        if (lineComment != null && code.startsWith(lineComment, index)) {
            val end = code.indexOf('\n', index).let { if (it < 0) code.length else it }
            tokens.add(CodeToken(index, end, CodeTokenKind.COMMENT))
            index = end
            continue
        }

        if (code.startsWith("/*", index)) {
            val end = code.indexOf("*/", index + 2).let { if (it < 0) code.length else it + 2 }
            tokens.add(CodeToken(index, end, CodeTokenKind.COMMENT))
            index = end
            continue
        }

        if (code.startsWith("<!--", index)) {
            val end = code.indexOf("-->", index + 4).let { if (it < 0) code.length else it + 3 }
            tokens.add(CodeToken(index, end, CodeTokenKind.COMMENT))
            index = end
            continue
        }

        val supportsBacktick = lang in setOf("js", "javascript", "ts", "typescript", "go")
        if (char == '"' || char == '\'' || (supportsBacktick && char == '`')) {
            val end = stringEnd(code, index, char)
            tokens.add(CodeToken(index, end, CodeTokenKind.STRING))
            index = end
            continue
        }

        if (char.isDigit() && (index == 0 || !isWordPart(code[index - 1]))) {
            var end = index + 1
            while (end < code.length && (code[end].isLetterOrDigit() || code[end] == '.' || code[end] == '_')) {
                end += 1
            }
            tokens.add(CodeToken(index, end, CodeTokenKind.NUMBER))
            index = end
            continue
        }

        if (isIdentifierStart(char)) {
            var end = index + 1
            while (end < code.length && isWordPart(code[end])) {
                end += 1
            }
            val word = code.substring(index, end)
            val kind = when {
                word in keywords(lang) -> CodeTokenKind.KEYWORD
                nextNonSpace(code, end) == '(' -> CodeTokenKind.FUNCTION
                word.firstOrNull()?.isUpperCase() == true -> CodeTokenKind.TYPE
                else -> null
            }
            if (kind != null) {
                tokens.add(CodeToken(index, end, kind))
            }
            index = end
            continue
        }

        if (char in "{}[]()<>,;:.+-*/%=!&|?@#") {
            tokens.add(CodeToken(index, index + 1, CodeTokenKind.PUNCTUATION))
        }
        index += 1
    }

    return tokens
}

private fun normalizeLanguage(language: String): String = when (val value = language.trim().lowercase().substringBefore(' ').removePrefix("language-")) {
    "rs" -> "rust"
    "kt" -> "kotlin"
    "py" -> "python"
    "js" -> "javascript"
    "ts" -> "typescript"
    "sh", "shell", "zsh" -> "bash"
    "yml" -> "yaml"
    "c++" -> "cpp"
    "cs" -> "csharp"
    else -> value
}

private fun lineComment(language: String): String? = when (language) {
    "python", "py", "bash", "sh", "shell", "zsh", "yaml", "yml", "toml", "ruby", "rb",
    "perl", "pl", "r", "makefile", "dockerfile", -> "#"
    "sql", "lua", "haskell" -> "--"
    "rust", "kotlin", "java", "c", "cpp", "csharp", "go", "swift", "javascript", "typescript",
    "js", "ts", "json5", -> "//"
    else -> null
}

private fun stringEnd(code: String, start: Int, quote: Char): Int {
    if (quote == '"' && code.startsWith("\"\"\"", start)) {
        val end = code.indexOf("\"\"\"", start + 3)
        return if (end < 0) code.length else end + 3
    }
    var index = start + 1
    while (index < code.length) {
        val char = code[index]
        if (char == '\\') {
            index += 2
            continue
        }
        if (char == quote) {
            return index + 1
        }
        if (char == '\n' && quote != '`') {
            return index
        }
        index += 1
    }
    return code.length
}

private fun nextNonSpace(code: String, from: Int): Char? {
    var index = from
    while (index < code.length && code[index].isWhitespace()) {
        index += 1
    }
    return code.getOrNull(index)
}

private fun isIdentifierStart(char: Char): Boolean =
    char.isLetter() || char == '_' || char == '$'

private fun isWordPart(char: Char): Boolean =
    char.isLetterOrDigit() || char == '_' || char == '$'

private val RUST = setOf(
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
    "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod",
    "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super",
    "trait", "true", "type", "unsafe", "use", "where", "while", "yield",
)

private val KOTLIN = setOf(
    "as", "break", "class", "continue", "do", "else", "false", "for", "fun", "if", "in",
    "interface", "is", "null", "object", "package", "return", "super", "this", "throw",
    "true", "try", "typealias", "typeof", "val", "var", "when", "while", "by", "catch",
    "constructor", "delegate", "dynamic", "field", "file", "finally", "get", "import",
    "init", "param", "property", "receiver", "set", "setparam", "where", "actual",
    "abstract", "annotation", "companion", "const", "crossinline", "data", "enum",
    "expect", "external", "final", "infix", "inline", "inner", "internal", "lateinit",
    "noinline", "open", "operator", "out", "override", "private", "protected", "public",
    "reified", "sealed", "suspend", "tailrec", "vararg",
)

private val JAVASCRIPT = setOf(
    "break", "case", "catch", "class", "const", "continue", "debugger", "default",
    "delete", "do", "else", "export", "extends", "false", "finally", "for", "function",
    "if", "import", "in", "instanceof", "let", "new", "null", "return", "static",
    "super", "switch", "this", "throw", "true", "try", "typeof", "var", "void", "while",
    "with", "yield", "async", "await", "interface", "type", "implements", "enum",
    "readonly", "public", "private", "protected",
)

private val PYTHON = setOf(
    "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del",
    "elif", "else", "except", "False", "finally", "for", "from", "global", "if", "import",
    "in", "is", "lambda", "None", "nonlocal", "not", "or", "pass", "raise", "return",
    "True", "try", "while", "with", "yield",
)

private val C_LIKE = setOf(
    "auto", "break", "case", "char", "const", "continue", "default", "do", "double",
    "else", "enum", "extern", "float", "for", "goto", "if", "inline", "int", "long",
    "register", "return", "short", "signed", "sizeof", "static", "struct", "switch",
    "typedef", "union", "unsigned", "void", "volatile", "while", "class", "namespace",
    "new", "delete", "public", "private", "protected", "template", "typename", "using",
    "virtual", "override", "true", "false", "nullptr", "bool", "var", "function", "let",
    "const", "interface", "extends", "implements", "package", "import",
)

private val BASH = setOf(
    "if", "then", "else", "elif", "fi", "for", "while", "do", "done", "case", "esac",
    "function", "in", "select", "until", "time", "coproc", "return", "local", "export",
    "readonly", "shift", "source", "alias", "unalias",
)

private val JSON = setOf("true", "false", "null")

private fun keywords(language: String): Set<String> = when (language) {
    "rust" -> RUST
    "kotlin" -> KOTLIN
    "javascript", "typescript", "js", "ts" -> JAVASCRIPT
    "python", "py" -> PYTHON
    "json", "json5" -> JSON
    "bash", "sh", "shell", "zsh" -> BASH
    "c", "cpp", "csharp", "java", "go", "swift" -> C_LIKE
    else -> emptySet()
}
