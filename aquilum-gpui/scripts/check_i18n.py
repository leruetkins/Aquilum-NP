"""
================================================================================
                    Aquilum GPUI - i18n Parity Checker
================================================================================

PURPOSE:
    Validates completeness and key parity of all localization files
    (locales/*.json) against the baseline reference locales/en.json.

FEATURES:
    1. Detects missing keys across all 20 supported languages.
    2. Identifies obsolete / extra keys present in translations but removed from en.json.
    3. Finds empty string translations ("").
    4. Supports automatic backfilling (--fix) with English values for missing
       and empty entries.

USAGE:
    1. Standard check (validation):
       python scripts/check_i18n.py

    2. Auto-fill missing and empty keys in all locales with English values:
       python scripts/check_i18n.py --fix

    3. Specify custom locales folder:
       python scripts/check_i18n.py --locales-dir path/to/locales

EXIT CODES:
    0 - All 20 files are 100% in sync (full key parity).
    1 - Missing keys or JSON syntax errors detected.
================================================================================
"""

import argparse
import json
import os
import sys
from pathlib import Path


def flatten_keys(d, prefix=""):
    """Recursively extracts flattened dot-notation keys and their values."""
    items = {}
    for k, v in d.items():
        full_key = f"{prefix}.{k}" if prefix else k
        if isinstance(v, dict):
            items.update(flatten_keys(v, full_key))
        else:
            items[full_key] = v
    return items


def unflatten_keys(flat_dict):
    """Reconstructs nested dictionary from dot-notation keys."""
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


def main():
    parser = argparse.ArgumentParser(description="Check i18n locale parity against en.json")
    parser.add_argument(
        "--locales-dir",
        default=str(Path(__file__).resolve().parent.parent / "locales"),
        help="Path to locales directory containing JSON files",
    )
    parser.add_argument(
        "--fix",
        action="store_true",
        help="Automatically fill missing keys with English fallback values",
    )
    args = parser.parse_args()

    locales_dir = Path(args.locales_dir)
    en_path = locales_dir / "en.json"

    if not en_path.exists():
        print(f"[ERROR] en.json not found at {en_path}")
        sys.exit(1)

    with open(en_path, "r", encoding="utf-8") as f:
        en_data = json.load(f)

    en_flat = flatten_keys(en_data)
    en_keys = set(en_flat.keys())

    all_files = sorted(locales_dir.glob("*.json"))
    other_files = [p for p in all_files if p.name != "en.json"]

    total_missing = 0
    total_extra = 0
    total_empty = 0

    print(f"Checking {len(other_files)} locale files against en.json ({len(en_keys)} keys)...\n")

    for loc_file in other_files:
        with open(loc_file, "r", encoding="utf-8") as f:
            try:
                data = json.load(f)
            except Exception as e:
                print(f"[{loc_file.name}] JSON Parse Error: {e}")
                total_missing += 1
                continue

        flat = flatten_keys(data)
        loc_keys = set(flat.keys())

        missing = sorted(list(en_keys - loc_keys))
        extra = sorted(list(loc_keys - en_keys))
        empty = [k for k, v in flat.items() if isinstance(v, str) and not v.strip()]

        total_missing += len(missing)
        total_extra += len(extra)
        total_empty += len(empty)

        if not missing and not extra and not empty:
            print(f"  [OK]  {loc_file.name:<12} (100% in sync, {len(loc_keys)} keys)")
        else:
            print(f"  [WARN] {loc_file.name:<12}:")
            if missing:
                print(f"         - Missing {len(missing)} keys:")
                for m in missing[:10]:
                    print(f"             * {m}")
                if len(missing) > 10:
                    print(f"             ... and {len(missing) - 10} more")
            if extra:
                print(f"         - Extra {len(extra)} keys not in en.json:")
                for e in extra[:5]:
                    print(f"             * {e}")
            if empty:
                print(f"         - Empty {len(empty)} keys:")
                for em in empty[:5]:
                    print(f"             * {em}")

        if args.fix and (missing or empty):
            fixed_empty = 0
            for m in missing:
                flat[m] = en_flat[m]
            for em in empty:
                if em in en_flat:
                    flat[em] = en_flat[em]
                    fixed_empty += 1
            rebuilt = unflatten_keys(flat)
            with open(loc_file, "w", encoding="utf-8") as out_f:
                json.dump(rebuilt, out_f, ensure_ascii=False, indent=2)
                out_f.write("\n")
            if missing:
                print(f"         => Auto-filled {len(missing)} missing keys with English values.")
            if fixed_empty:
                print(f"         => Auto-filled {fixed_empty} empty keys with English values.")
            total_missing -= len(missing)
            total_empty -= fixed_empty

    print("\n" + "=" * 50)
    if total_missing == 0 and total_extra == 0 and total_empty == 0:
        print("[SUCCESS] All localization files are 100% complete and in sync!")
        sys.exit(0)
    else:
        print(f"[SUMMARY] Found: {total_missing} missing, {total_extra} extra, {total_empty} empty strings.")
        if not args.fix and total_missing > 0:
            print("Tip: Run with --fix to automatically populate missing keys from en.json.")
        sys.exit(1 if total_missing > 0 else 0)


if __name__ == "__main__":
    main()
