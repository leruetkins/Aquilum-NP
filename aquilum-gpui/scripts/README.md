# 🌐 Aquilum GPUI Internationalization (i18n) Guide

Aquilum GPUI supports **20 UI languages** with zero runtime loading overhead (all locale dictionaries are embedded into the binary via `include_str!` and parsed on startup).

---

## 📁 Directory Structure

```text
aquilum-gpui/
├── locales/                 # JSON dictionaries for each supported language
│   ├── en.json              # Baseline reference file (Source of Truth)
│   ├── ru.json              # Russian
│   ├── de.json              # German
│   ├── fr.json              # French
│   ├── es.json              # Spanish
│   ├── zh-CN.json           # Simplified Chinese
│   └── ... (20 files total)
├── scripts/                 # Automation & validation scripts
│   ├── README.md            # This documentation file
│   ├── check_i18n.py        # Key parity validator and missing key detector
│   ├── translate_all.py     # Incremental auto-translator via Google Translate API
│   └── translate_overrides.json # Curated translations that win over the machine service
└── src/
    └── i18n.rs              # Rust localization manager + parity unit tests
```

---

## 🛠️ Scripts & Tools

### 1. `scripts/check_i18n.py` — Parity Validator

Compares all locale JSON files against the baseline `locales/en.json`.

#### What it checks:
* **Missing keys**: Keys defined in `en.json` that are absent in any translation.
* **Extra keys**: Obsolete keys remaining in translations after being deleted from `en.json`.
* **Empty strings**: Untranslated empty values (`""`).

#### Commands:
```powershell
# Standard validation check (outputs [OK] or missing/extra keys report)
python scripts/check_i18n.py

# Automatically copy missing keys from en.json as fallback values,
# and replace empty ("") translations with the English text:
python scripts/check_i18n.py --fix
```

#### Exit Codes:
* `0` — All 20 files have 100% complete key parity.
* `1` — Missing keys, empty strings, or JSON syntax errors were found.

---

### 2. `scripts/translate_all.py` — Automated Incremental Translator

Translates missing or newly added strings into 18 target languages. `ru.json` is authored manually and is not machine-translated.

#### Key Features:
* **Incremental**: Automatically skips strings that are already translated. Only translates strings matching the English baseline.
* **Resumable & Safe**: Flushes progress to disk **every 10 translated keys** and at the end of each file. It can be safely interrupted anytime (`Ctrl+C`) without losing translated work.
* **Token Protection**:
  * Format placeholders (`{0}`, `{1}`, `{:?}`) are tokenized and protected from alteration.
  * Keyboard shortcuts (`Ctrl + O`, `Alt + Z`), system fonts, and technical terms (`JSON`, `BM25F`, `Consolas`) are preserved verbatim.
* **Curated Overrides**: `scripts/translate_overrides.json` maps
  `{ "<locale>": { "<english text>": "<translation>" } }`. An override is applied
  before any network request, so established UI terminology stays consistent.
  The service often returns short capitalized phrases (for example `Word Wrap`)
  unchanged; add such terms to the overrides file once.
* **Honest Logging**: When the service returns the source unchanged, the run prints
  `[Same] ...`, and the per-file summary reports processed vs. actually translated counts.
* **Empty Responses**: An empty service response is retried; if it keeps failing,
  the source text is kept instead of saving `""`.
* **Live Progress**: Prints real-time log entries showing the file, index, source text, and translated result.

#### Command:
```powershell
python scripts/translate_all.py
```

---

## 🔄 Workflow: Adding New UI Strings

1. **Add keys to `locales/en.json`**:
   ```json
   {
     "settings": {
       "editor": {
         "my_new_feature": "Enable Super Fast Mode",
         "my_new_feature_desc": "Accelerates file rendering"
       }
     }
   }
   ```

2. **Sync & Auto-Translate**:
   ```powershell
   # Option A: Auto-populate and auto-translate the 18 machine-translated languages
   python scripts/check_i18n.py --fix
   python scripts/translate_all.py

   # Option B: Translate manually in specific files and verify parity
   python scripts/check_i18n.py
   ```

3. **Use in Rust code**:
   ```rust
   self.t("settings.editor.my_new_feature")
   self.t_fmt("sidebar.file_deleted", &[&file_name])
   ```

---

## 🧪 Automated CI / CD Unit Tests

A comprehensive parity test is integrated into `src/i18n.rs` and runs with `cargo test`:
```powershell
cargo test --bin vibecodium test_all_locales_have_full_key_parity_with_en
```
If any language file is missing even a single key, the test immediately fails with the exact list of missing keys.
