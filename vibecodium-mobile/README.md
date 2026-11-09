# vibecodium mobile

Android front end for vibecodium: a Kotlin/Compose phone shell over a
platform-independent Rust core. The layout is a direct port of the Obsidian
1.15.0 Android shell (`assets/public/app.css` + `app.js` inside the APK,
`sha256 2d92966f…bca5d`), not a generic Android template.

## Architecture

```
vibecodium-mobile/
  core/        Rust cdylib (vibecodium-core)
  app/         Android app (Kotlin + Jetpack Compose)
  scripts/     build-core.sh
```

### Rust core (`core/`)

- `vault.rs` and `trash.rs` are **included from the desktop repository by
  path** (`../../../aquilum-gpui/src/...`), so vault path validation and
  `.trash` handling cannot drift between the two front ends.
- `src/api.rs` is a small JSON API: `open_vault`, `create_vault`,
  `list_notes`, `read_note`, `write_note`, `create_note`, `delete_note`,
  `rename_note`, `search`, `outline`, `stats`, `markdown`, `analyze`.
  `analyze` is the path the editor uses: it builds the line model once and
  derives the outline and statistics from it.
- `src/markdown.rs` is a line-oriented block parser (headings, paragraphs,
  fenced code, quotes, lists/tasks, images, tables, rules) with 1-based source
  lines. The Compose preview and the source highlighter both consume it. Its
  JSON is compact: default fields are omitted, `raw` is not sent, and inline
  spans travel as one `kind:start:end;…` string per line.
- `src/jni_bridge.rs` exports `Java_md_vibecodium_mobile_CoreBridge_*`.
- `images` in `core/src/lib.rs` is a no-op stand-in for the desktop attachment
  subsystem; it keeps `vault.rs`/`trash.rs` reusable without the GPUI tree.
- Every JNI call returns `{"ok":true,"data":...}` or
  `{"ok":false,"error":"..."}`; the Kotlin side renders the error verbatim.

### Android app (`app/`)

- `CoreBridge.java` — static native declarations matching the exported symbols.
- `VaultRepository.kt` — one contract with two implementations:
  - `FileVaultRepository` uses the Rust vault (path validation, `.trash`);
  - `SafVaultRepository` queries `DocumentsContract` directly for a folder
    chosen through the Storage Access Framework. One child cursor per folder
    returns id, name, MIME type, size and modification time, so a large vault
    costs one binder query per directory instead of several per file. The
    resulting path→document map is cached and reused for opens, writes,
    renames, attachments and sync. The tree URI is persisted with
    `takePersistableUriPermission`.
- Opening a vault publishes the shell first and builds the tree snapshot on
  `Dispatchers.IO`; the first frame no longer waits for a recursive walk. Tabs
  persist their SAF document URI, so a saved note reopens with one
  `openInputStream` instead of re-resolving each path segment.
- Startup restores only the saved tabs. If there are none, the editor stays on
  **No tabs open**; it does not create or auto-open a note. An unreadable saved
  tab also does not overwrite the stored session with an empty list.
- `Persistence.kt` — locator, per-vault tabs/cursors and bookmarks.
- `VibecodiumController.kt` — state holder; file work on `Dispatchers.IO`,
  debounced auto-save (600 ms), history, tabs, search, rename, bookmarks.
- `ui/VibecodiumApp.kt` — the phone shell:
  - bottom navbar **52 dp** (`--navbar-height: var(--touch-size-l)`);
  - view header **44 dp** (`--view-header-height: var(--touch-size-m)` on a phone);
  - left/right drawers **84 %** of the width, clamped to **300–420 dp**
    (`--mobile-sidebar-width: 84vw`, `--mobile-sidebar-min-width: 300px`,
    `--mobile-sidebar-max-width: 450px`);
  - backdrop and full-screen tab switcher, as in `.workspace-drawer-backdrop`
    and `.mobile-tab-switcher`;
  - source editor with line numbers and markdown highlighting;
  - preview mode with headings, lists, quotes, fenced code, tables and local
    images (`![[pic.png]]` / `![](path)`), loaded through the repository so it
    works for both file and SAF vaults;
  - the Files drawer is a **collapsible folder tree** (empty folders included)
    over every vault file: notes open in the editor, images open in a
    full-screen viewer, other attachments show their type; expand/collapse is
    persisted per vault, and a refresh button and file counter sit in the
    drawer header.
  - above the tree there is an instant **file filter** (name/path, no content
    scan) and a one-button toolbar that collapses or expands every folder in
    one tap; the tree itself is prebuilt when the file list changes, so
    opening the drawer does not rebuild it.
- `ui/Theme.kt` — Obsidian 1.15.0 **mobile dark** palette copied from
  `app.css` (background `#000000`, surface `#1E1E1E`, accent `#8A5CF5`,
  text `#DADADA`/`#B3B3B3`), with every Material 3 role set and a neutral
  `surfaceTint`, so `Surface(tonalElevation)` cannot paint a purple cast on
  the navbar, header, drawers or menus.
- Markdown: `classify`, `inline`, `syntax_spans`, `grammar`, `quote`, `list` and
  `code_scan` are **included from `aquilum-gpui/src/markdown` by path**, so the
  Android preview and source view use the same block classification, inline
  spans, escapes, links, tags, callouts and code-fence state as the desktop app.
  `core/src/markdown.rs` flattens them into one JSON line model.
- Source live preview: on idle lines block markers (`# `, `> `, `- `, code
  fences) and inline markers (`**`, `*`, backticks, `~~`, `==`, link syntax) are
  replaced with spaces or a rendered glyph of the **same char count**, so caret
  offsets stay 1:1; the line with the caret keeps its raw markers for editing.
  The preview consumes the same model.
- Very large notes (over `120_000` chars or `4_000` lines) open in source mode
  without the live line model, so a 600 KB note does not block the first frame.
  Switching to **Preview** builds the model on demand and shows
  “Preparing preview…”.
- Code files: `.rs`, `.kt`, `.py`, `.js`, `.ts`, `.json`, `.toml`, `.yaml`,
  `.sh`, C/C++ and more open in the editor with line numbers and syntax
  highlighting. Preview shows a read-only syntax view with line numbers, Copy
  and horizontal scrolling.
- `CodeTokenizer.kt` supplies token-level highlighting for code blocks and code
  files; markdown block/inline structure comes from the shared Rust modules.

### Dropbox sync

- `SyncProvider.kt` — OAuth 2 PKCE over a loopback redirect
  (`http://127.0.0.1:47219/`, the same URI the desktop app registers),
  token refresh, and the Dropbox `files/list_folder`, `download`, `upload`,
  `delete_v2` and `create_folder_v2` endpoints.
- `SyncEngine.kt` — provider-neutral pass over `VaultRepository`, so it works
  for both a plain directory and a SAF tree:
  - new local files upload, new remote files download;
  - one changed side pushes/pulls;
  - both changed → local stays at the original path and the remote version is
    kept as `name (conflict date).ext` locally and on Dropbox;
  - deletions propagate only while the other side is unchanged;
  - more than half of 10+ files deleting aborts the pass;
  - `Dropbox content_hash` is computed locally to avoid false conflicts;
  - service paths (`.vault`, `.git`, `.trash`, `.obsidian`, `node_modules`,
    `target`, hidden files) never sync.
- `SyncRecords.kt` — SQLite (`sync.db`) records the last agreed local hash,
  size/mtime, remote rev and content hash per path, plus the last pass time.
- The remote root is `/<vault name>` inside the Dropbox app folder.
- Sync is available from **More → Sync**: connect, sync now, disconnect,
  client-id override, and **Sync on open**. The drawer subtitle shows the last
  sync time or the current progress.

## Performance

- **SAF traversal**: one `DocumentsContract` child query per folder with the
  five metadata columns. `DocumentFile` 1.0.0 queries `name`, `isDirectory`,
  `length` and `lastModified` separately, which cost several binder calls per
  file and made large vaults feel like a full rescan.
- **Non-blocking open**: the Compose shell is published before the tree
  snapshot; `refreshLists` fills the drawer from IO. The app no longer waits
  for three sequential walks (`listNotes` + `listFiles` + `listFolders`).
- **Fast note open**: the snapshot keeps `path → document URI`, and tabs
  persist that URI. Opening a note is one `openInputStream`, not a
  segment-by-segment `findFile` with a name query per sibling. Writes, renames,
  deletes and downloads update the cached index in place.
- **One parse**: `nativeAnalyze` builds the markdown model once and derives
  the outline and stats from it, replacing three JNI calls and three parses.
- **Compact model**: the JSON dropped from 15.7 MB to 3.5 MB for a 20k-line
  note by omitting default fields, dropping duplicated `raw`/span text and
  encoding spans as `kind:start:end;…`.
- **Large notes**: source mode defers the line model above the threshold;
  preview builds it only when requested.
- **Overlays**: the folder tree is built once per file-list change and stored
  in `UiState`, so opening the Files drawer filters a ready tree instead of
  rebuilding it. The editor consumes an `EditorSlice` rather than the whole
  `UiState`. On the debug emulator the first frame after a tap was 18 ms for
  the More menu, 22–25 ms for the tab switcher and 40–64 ms for the left
  drawer. The remaining visible delay of the More menu is Material
  `DropdownMenu`'s ~120 ms enter animation, not I/O; drawers use no explicit
  animation.
- **Measured on the emulator** (debug x86_64 build, not a physical device):
  2,000-file local vault cold start `TotalTime` about 1.0–1.2 s; 2,003-file SAF
  vault cold start about 1.0 s with the drawer populated by about 2 s;
  SAF note open `readNote` 10 ms and first frame 35 ms; a new local note
  `readNote` 2 ms / first frame 22 ms; 20k-line (609 KB) preview
  `nativeAnalyze` 0.17 s and JSON parse 0.50 s, with no ANR. The earlier
  15.7 MB model took 4.4 s in native/JNI plus 0.8 s in `org.json`.

## Build

Prerequisites:

- JDK 17+ (`JAVA_HOME` set)
- Android SDK with platform 35 and NDK `28.2.13676358`
- Rust targets `aarch64-linux-android` and `x86_64-linux-android`
- Gradle 8.14+ (the project was built with the cached 8.14.4)

```bash
bash scripts/build-core.sh          # builds both .so files into app/src/main/jniLibs
gradle :app:assembleDebug           # or ./gradlew after generating a wrapper
```

APK: `app/build/outputs/apk/debug/app-debug.apk` (arm64-v8a + x86_64).

`local.properties` points at the local SDK. It is machine-specific; do not
commit it.

### Release signing

`app/build.gradle.kts` reads `keystore.properties` from the project root when it
exists:

```properties
storeFile=keystore/vibecodium-release.jks
storePassword=change-me
keyAlias=vibecodium
keyPassword=change-me
```

Both `keystore.properties` and `keystore/` are git-ignored. To create a local
key for testing:

```bash
mkdir -p keystore
keytool -genkeypair -v -keystore keystore/vibecodium-release.jks \
  -alias vibecodium -keyalg RSA -keysize 2048 -validity 10000 \
  -storepass vibecodium -keypass vibecodium \
  -dname "CN=vibecodium, OU=mobile, O=vibecodium, L=Local, ST=Local, C=US"
cp keystore.properties.example keystore.properties   # edit the passwords
gradle :app:assembleRelease
apksigner verify --print-certs app/build/outputs/apk/release/app-release.apk
```

Without `keystore.properties`, the release build stays unsigned; the debug
build is unaffected.

## Verified on the emulator

AVD `Medium_Phone_API_36.1` (x86_64, 1080×2400):

- **File vault**: demo vault creation, note open, JNI save, search, new note,
  tabs, drawer, outline, rename, bookmarks.
- **SAF vault**: picked `Download/vibesaf` through the system picker;
  `SAF.md` was listed and opened; editing wrote through `ContentResolver` to
  `/sdcard/Download/vibesaf/SAF2.md`; content search and rename worked.
- **Preview**: heading, paragraph, list, fenced code, table and `![[pic.png]]`
  rendered; the image bitmap was verified in the screenshot (red 240×140 PNG).
- **Source**: line numbers and markdown highlighting are visible.
- **Explorer**: a 24-note + 1-image demo vault showed **25 files**; the list
  scrolls, `attachment.png` opened in the full-screen viewer (red PNG verified
  in the screenshot), and the refresh button picked up a file added externally
  while the vault was open.
- **Folder tree**: added `Empty/`, `Projects/alpha.md`,
  `Projects/Sub/beta.md` and `Projects/picture.png`; the tree showed folders
  first, a nested `Sub` folder, an empty folder, and PNG badges. Collapsing
  `Projects` hid its children, the collapsed state survived a force-stop and
  relaunch, and expanding restored them.
- **Dark theme/code**: source and preview screenshots were checked by pixel:
  background `#000000`, header/navbar/menu `#1E1E1E`, code block `#111111`,
  and the Obsidian syntax colors (keyword/string/number/comment/function) in
  both modes; there is no purple tint in the header, navbar or menu.
- **Code editing**: `CodeEditingTest` covers Rust/Python/JSON tokenization,
  Tab/Shift+Tab and list/indent continuation on Enter.
- **Shared markdown model / live preview**: `rich.md` (headings, bold/italic,
  inline code, links, tags, tasks, nested lists, callout, code fence, table,
  image) rendered in preview; OCR of the source showed `Heading 2` without
  `##` while the caret line kept `# Heading 1`, and fences were hidden while
  code lines remained. `main.rs` opened in Source and CodeView with rust
  syntax colors and line numbers.
- **Scale**: a 2,000-note / 40-folder vault (local and SAF) opened without
  blocking the first frame; the SAF drawer reported **2,003 files**. A
  nested SAF note opened in **10 ms** (`readNote`) and its first frame in
  **35 ms**. A 20k-line / 609 KB note opened without ANR; its deferred preview
  took 0.17 s native + 0.50 s JSON parsing. A 20k-line source view no longer
  creates 20k composables for the gutter (one text block instead).
- **Large SAF preview**: `SAF2.md` (heading, image, bullets, fenced code,
  table) rendered from the compact span format; source view still hides
  idle markers and keeps the caret line raw.
- **File filter / folder toolbar**: typing `SafHuge` in the Files drawer
  reduced a 2,003-file trees to the matching file and its ancestors; the
  toolbar button collapsed the tree to root folders/files and changed to
  **Expand all**, which restored the full tree.
- **Overlay latency**: measured first-frame times on the same 2,003-file SAF
  vault: More menu 18 ms, tab switcher 22–25 ms, left drawer 40–64 ms.
- **Startup without tabs**: cleared the saved tab list and launched twice; the
  editor showed **No tabs open** and the SAF vault still had 2,003 files before
  and after, so no note was created. Opening `n1.md` and relaunching restored
  that tab with its content.
- **Persistence**: tabs and active index restored after force-stop/relaunch;
  bookmarks persisted per vault.
- **Sync unit tests**: `gradle :app:testDebugUnitTest` — 7 sync-engine tests
  (upload, download, local/remote change, conflict copy, deletion, bulk-delete
  guard) and 3 OAuth tests (PKCE URL, loopback code/state, wrong-state reject).
- **Sync on device**: the Sync screen opened; **Connect Dropbox** launched
  Chrome with a PKCE URL; the loopback server received a code and state through
  `adb forward`; the token exchange reached Dropbox and returned the expected
  `Dropbox 400: code doesn't exist or has expired` for the synthetic code.
  A real successful Dropbox login was **not** performed: it needs the user's
  Dropbox account and consent.
- **Release**: `app-release.apk` assembled with the local keystore; v2 signature
  verified with `apksigner`.
- No `FATAL EXCEPTION`/`UnsatisfiedLink` in logcat.

Screenshots: `screenshots/editor.png`, `drawer.png`, `tabs.png`,
`saf-preview.png`, `sync.png`.

## Current limitations

- The source editor numbers logical lines; a wrapped long line does not get an
  extra gutter row.
- The source live preview hides markers by replacing their chars with spaces or
  glyphs; it is not a custom painted editor, so task checkboxes, table editing
  and similar Obsidian widgets remain approximate.
- Notes above 120k chars / 4k lines open without live marker hiding in source
  mode; **Preview** builds the model on demand, so the first preview of a very
  large note has a short “Preparing preview…” delay.
- The SAF snapshot is built in the background and is not a filesystem watcher;
  external changes appear after **Refresh**, re-opening the vault, or a sync
  pass. On vaults with tens of thousands of folders the drawer can fill a few
  seconds after the first frame, although the editor is already usable.
- Table preview cells are plain text (they do not carry inline markdown spans).
- Inline image syntax remains visible in the source editor; the preview renders
  the image.
- Syntax highlighting for code is token-based and covers common languages; it
  is not a semantic parser.
- External `http(s)` images are not downloaded (local vault images work).
- SAF rename is same-folder only; cross-folder moves are not implemented.
- Bookmarks are a flat path set; no nested bookmark groups.
- The Files drawer filter matches file names and paths only; the **Search**
  tab still performs the slower content search by reading notes.
- The explorer shows all files, but only images get an inline viewer; PDFs and
  other attachments show a "preview is not supported" message.
- There is no filesystem watcher: external changes appear after **Refresh**,
  after re-opening the vault, or after a sync pass.
- Sync conflict handling creates a conflict copy; the desktop's line-by-line
  Markdown merge is not ported yet.
- OAuth refresh/access tokens are stored in the app-private
  `SharedPreferences` (not encrypted); a production build should use the
  Android Keystore or `EncryptedSharedPreferences`.
- Sync is manual or on-open; there is no periodic background pass yet.
- The real Dropbox API flow was exercised only up to the token exchange with a
  synthetic code; a successful login/data pass requires the user's account.
- The Gradle wrapper is not generated: `services.gradle.org` redirects to a
  GitHub CDN that returned 504 during the wrapper URL check. Use the installed
  Gradle 8.14.4, or run `gradle wrapper` on a machine with access.
- The APK is debug/release-signed locally; production distribution still needs
  its own key management.

## Next steps

1. Persist the last editor mode and scroll offset per tab.
2. Inline image rendering inside the source view, not only preview.
3. Markdown table editing and task checkboxes in preview.
4. Bookmark groups and a search scope selector.
5. Port the desktop line-by-line Markdown merge for sync conflicts.
6. Encrypted token storage and a periodic WorkManager sync.
