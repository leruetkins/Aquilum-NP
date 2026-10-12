"""
Full localization translator for Aquilum GPUI.
Translates all untranslated keys from en.json into the target 18 language JSON files.
Preserves placeholders ({0}, {1}, etc.), keyboard shortcuts, special terms, and JSON structure.
"""

import json
import re
import time
import urllib.parse
import urllib.request
from pathlib import Path

# Mapping of file codes to Google Translate API language codes
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


def translate_text(text: str, target_lang: str) -> str:
    """Translates a single string with placeholder preservation."""
    if text in DO_NOT_TRANSLATE or text in SHORTCUT_KEYS:
        return text

    # Extract placeholders like {0}, {1} to prevent mangling
    placeholders = re.findall(r"\{\d+\}", text)
    # Temporary token replacements
    temp_text = text
    for i, ph in enumerate(placeholders):
        temp_text = temp_text.replace(ph, f"__PH_{i}__")

    url = (
        "https://translate.googleapis.com/translate_a/single?client=gtx&sl=en&tl="
        + urllib.parse.quote(target_lang)
        + "&dt=t&q="
        + urllib.parse.quote(temp_text)
    )
    req = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64)"})

    for attempt in range(4):
        try:
            with urllib.request.urlopen(req, timeout=10) as response:
                raw = response.read().decode("utf-8")
                data = json.loads(raw)
                res = "".join([part[0] for part in data[0] if part[0]])

                # Restore placeholders
                for i, ph in enumerate(placeholders):
                    res = re.sub(rf"__\s*PH_{i}\s*__", ph, res, flags=re.IGNORECASE)

                return res
        except Exception as e:
            if attempt == 3:
                print(f"    Failed to translate '{text}': {e}")
                return text
            time.sleep(1.0 + attempt)


def main():
    locales_dir = Path(__file__).resolve().parent.parent / "locales"
    with open(locales_dir / "en.json", "r", encoding="utf-8") as f:
        en_data = json.load(f)

    en_flat = flatten_keys(en_data)

    for code, tl_code in LOCALE_MAP.items():
        path = locales_dir / f"{code}.json"
        if not path.exists():
            print(f"Skipping {code}: file not found")
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

        print(f"\n[{code}] Found {len(keys_to_translate)} untranslated keys. Translating to {tl_code}...")
        if not keys_to_translate:
            print(f"[{code}] Already fully translated!")
            continue

        count = 0
        for k in keys_to_translate:
            original = en_flat[k]
            translated = translate_text(original, tl_code)
            target_flat[k] = translated
            count += 1
            if count % 25 == 0 or count == len(keys_to_translate):
                print(f"  [{code}] Progress: {count}/{len(keys_to_translate)}")
            time.sleep(0.04)

        # Reconstruct json
        rebuilt = unflatten_keys(target_flat)
        with open(path, "w", encoding="utf-8") as f:
            json.dump(rebuilt, f, ensure_ascii=False, indent=2)
            f.write("\n")
        print(f"[{code}] Done! Saved {len(target_flat)} keys to {path.name}")

    print("\nAll locales processed successfully!")


if __name__ == "__main__":
    main()
