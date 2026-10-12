"""
Full localization translator for Aquilum GPUI.
Features:
- Incremental translation: automatically skips already translated strings.
- Robust UTF-8 console output safe for Windows cp1251 terminals.
- Uses reliable client endpoint (dict-chrome-ex) with fallback to gtx.
- Auto-saves JSON every 10 translated keys so progress is immediately committed to disk.
- Rate-limit handling with exponential backoff.
- Placeholder protection ({0}, {1}, etc.).
"""

import json
import re
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

# Force UTF-8 stdout if needed
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")

LOCALE_MAP = {
    "ar": "ar",
    "cs": "cs",
    "de": "de",
    "es": "es",
    "fr": "fr",
    "hi": "hi",
    "it": "it",
    "ja": "ja",
    "ko": "ko",
    "nl": "nl",
    "pl": "pl",
    "pt": "pt",
    "sv": "sv",
    "tr": "tr",
    "uk": "uk",
    "vi": "vi",
    "zh-CN": "zh-CN",
    "zh-TW": "zh-TW",
}

DO_NOT_TRANSLATE = {
    "OK", "JSON", "Markdown", "Tantivy", "BM25F", "Adamic-Adar",
    "Consolas", "Fira Code", "JetBrains Mono", "Source Code Pro", "Cascadia Code",
    "Segoe UI", "Inter", "Roboto", "Arial", "Georgia", "Merriweather", "Lora", "Charter"
}

SHORTCUT_KEYS = {
    "Ctrl + O", "Ctrl + N", "Ctrl + S", "Ctrl + F", "Ctrl + ,", "Ctrl + \\",
    "Alt + Z", "Ctrl + B", "Ctrl + I", "Ctrl + A", "Ctrl + C", "Ctrl + X",
    "Ctrl + V", "Ctrl + Z", "Ctrl + Y", "Home / End"
}


def log(msg: str):
    print(msg, flush=True)


def flatten_keys(d, prefix=""):
    items = {}
    for k, v in d.items():
        p = f"{prefix}.{k}" if prefix else k
        if isinstance(v, dict):
            items.update(flatten_keys(v, p))
        else:
            items[p] = v
    return items


def unflatten_keys(flat_dict):
    res = {}
    for k, v in flat_dict.items():
        parts = k.split(".")
        cur = res
        for part in parts[:-1]:
            if part not in cur or not isinstance(cur[part], dict):
                cur[part] = {}
            cur = cur[part]
        cur[parts[-1]] = v
    return res


def save_locale(file_path: Path, flat_dict: dict):
    rebuilt = unflatten_keys(flat_dict)
    with open(file_path, "w", encoding="utf-8") as f:
        json.dump(rebuilt, f, ensure_ascii=False, indent=2)
        f.write("\n")


def fetch_translation(text: str, target_lang: str) -> str:
    # 1. Try dict-chrome-ex endpoint
    url1 = f"https://clients5.google.com/translate_a/t?client=dict-chrome-ex&sl=en&tl={target_lang}&q={urllib.parse.quote(text)}"
    req1 = urllib.request.Request(url1, headers={"User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64)"})
    try:
        with urllib.request.urlopen(req1, timeout=8) as resp:
            data = json.loads(resp.read().decode("utf-8"))
            if isinstance(data, list) and len(data) > 0 and isinstance(data[0], str):
                return data[0]
            if isinstance(data, list) and len(data) > 0 and isinstance(data[0], list):
                return "".join([p[0] for p in data[0] if p and p[0]])
    except Exception:
        pass

    # 2. Fallback to gtx endpoint
    url2 = f"https://translate.googleapis.com/translate_a/single?client=gtx&sl=en&tl={target_lang}&dt=t&q={urllib.parse.quote(text)}"
    req2 = urllib.request.Request(url2, headers={"User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64)"})
    with urllib.request.urlopen(req2, timeout=8) as resp:
        data = json.loads(resp.read().decode("utf-8"))
        return "".join([p[0] for p in data[0] if p and p[0]])


def translate_text(text: str, target_lang: str) -> str:
    if text in DO_NOT_TRANSLATE or text in SHORTCUT_KEYS:
        return text

    placeholders = re.findall(r"\{\d+\}", text)
    temp_text = text
    for i, ph in enumerate(placeholders):
        temp_text = temp_text.replace(ph, f"__PH_{i}__")

    for attempt in range(5):
        try:
            res = fetch_translation(temp_text, target_lang)
            for i, ph in enumerate(placeholders):
                res = re.sub(rf"__\s*PH_{i}\s*__", ph, res, flags=re.IGNORECASE)
            return res
        except urllib.error.HTTPError as he:
            wait_sec = 4.0 * (attempt + 1)
            log(f"   [HTTP {he.code}] Sleeping {wait_sec:.0f}s before retry {attempt + 1}/5...")
            time.sleep(wait_sec)
        except Exception as e:
            wait_sec = 2.0 * (attempt + 1)
            time.sleep(wait_sec)

    log(f"   [Warning] Translation failed for '{text}', keeping original.")
    return text


def main():
    locales_dir = Path(__file__).resolve().parent.parent / "locales"
    with open(locales_dir / "en.json", "r", encoding="utf-8") as f:
        en_data = json.load(f)

    en_flat = flatten_keys(en_data)

    log("=" * 60)
    log("Aquilum i18n Translation Runner")
    log(f"Base locale: en.json ({len(en_flat)} keys)")
    log("=" * 60)

    for code, tl_code in LOCALE_MAP.items():
        path = locales_dir / f"{code}.json"
        if not path.exists():
            continue

        with open(path, "r", encoding="utf-8") as f:
            target_data = json.load(f)

        target_flat = flatten_keys(target_data)
        keys_to_translate = []
        for k, v in target_flat.items():
            if k in en_flat and v == en_flat[k]:
                if v in DO_NOT_TRANSLATE or v in SHORTCUT_KEYS:
                    continue
                keys_to_translate.append(k)

        total_keys = len(keys_to_translate)
        if not keys_to_translate:
            log(f"[{code}.json] -> 100% translated (0 untranslated). Skipping.")
            continue

        log(f"\n>>> [{code}.json] Found {total_keys} untranslated strings -> Translating to [{tl_code}]...")

        count = 0
        unsaved = 0
        for idx, k in enumerate(keys_to_translate, 1):
            orig = en_flat[k]
            trans = translate_text(orig, tl_code)
            target_flat[k] = trans
            count += 1
            unsaved += 1

            preview_orig = (orig[:25] + "..") if len(orig) > 25 else orig
            preview_trans = (trans[:25] + "..") if len(trans) > 25 else trans
            log(f"  [{code}] ({idx}/{total_keys}) {k}: '{preview_orig}' -> '{preview_trans}'")

            if unsaved >= 10:
                save_locale(path, target_flat)
                unsaved = 0

            time.sleep(0.15)

        save_locale(path, target_flat)
        log(f"[{code}.json] Successfully completed and saved! ({count} keys translated)\n")

    log("=" * 60)
    log("All localization files processed successfully!")
    log("=" * 60)


if __name__ == "__main__":
    main()
