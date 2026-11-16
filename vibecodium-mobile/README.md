# vibecodium mobile

Android client: a Kotlin/Compose shell over a platform-independent Rust core.

<p align="center">
  <img src="vibecodium-mobile.jpg" alt="Vibecodium Mobile" width="320">
</p>

## Architecture

- **`core/`**: Rust cdylib with the vault, trash, JSON API (open, read, write, search, outline, stats), markdown parser, JNI bridge, and native `SurfaceView` renderers for the editor and file tree.
- **`app/`**: Kotlin/Compose with the header, drawers, tabs, settings, preview, state, and saving.
- **`scripts/`**: `build-core.sh`.

## Features

- Vaults: plain folder or SAF folder.
- Editor: native, with line numbers, markdown and code highlighting, selection, and inertial scrolling.
- Preview: headings, lists, quotes, code, tables, local images.
- Files: folder tree, quick filter, bookmarks, recents, search.
- Dropbox sync (OAuth PKCE, conflict copies, delta listing).
- Settings: appearance, editor, files, sync, advanced.
- Obsidian community themes: catalog download, install/update/remove, Dark/Light/System, accent and token-level CSS theming.

## Obsidian themes

Settings → **Appearance** supports the Obsidian community catalog. Themes are
stored app-privately (`files/themes/<Name>/manifest.json`, `theme.css`, plus a
small catalog metadata file), so they also work for SAF vaults that hide
`.obsidian`. Installation uses GitHub Releases, falls back to
`raw.githubusercontent.com/<repo>/HEAD/`, and retries network failures through
`releases.obsidian.md/proxy`; legacy `obsidian.css` themes are supported.

On install/update the client parses `theme.css` once and writes
`vibecodium-theme.json`, a token artifact covering phone/tablet × light/dark.
Normal startup reads that small JSON and does not run the CSS parser; the
parser runs again only when the CSS stamp (`length:lastModified`) or schema
changes. Accent overrides are applied on top of the parsed tokens, so changing
the accent never re-parses the theme.

**Light theme** and **Dark theme** are independent slots. The base color
scheme (Dark / Light / Adapt to system) selects which slot is active, so two
different themes can be assigned and switched without re-opening the catalog.

`ThemeCss` resolves root variables (`:root`, `body`, `.theme-dark`,
`.theme-light`, `.is-mobile`), `var()`, `calc()`, `color-mix()`,
`light-dark()` and Obsidian's semantic aliases. The resulting palette feeds
both Compose and the native renderer through
`CoreBridge.nativeThemeSetPalette`. Rules that target Obsidian's DOM are
skipped and counted in the Appearance **Compatibility** note; this is
token-level theming, not a full CSS engine. Theme font families apply to
Compose text; the native editor keeps its bundled fonts.

## Build

Requires JDK 17+, Android SDK 35, NDK `28.2.13676358`, and Rust targets `aarch64-linux-android` and `x86_64-linux-android`.

```bash
bash scripts/build-core.sh
gradle :app:assembleDebug
```

Release builds are signed via `keystore.properties` (git-ignored).

## Limitations

- Text scale doesn't apply to the native editor.
- No soft wrapping or full TalkBack support.
- No filesystem watcher; refresh is manual.
- Dropbox tokens are unencrypted; no background sync.
- No line-by-line conflict merge.

## Next steps

Benchmarks, inline images in the editor, table editing and checkboxes, bookmark groups, line-by-line merge, encrypted token storage, and periodic sync.