# Dropbox sync

Vibecodium can keep a workspace in sync through the user's own Dropbox
account. The implementation is a focused port of the Aquilum Dropbox engine
(`Aquilum/aquilum-app/core/src/sync`) into the GPUI application model.

## What it does

* The remote folder is the workspace folder name relative to the Dropbox
  app's root. Opening `D:\Notes\Work` writes to `/Work`; with an **App folder**
  app that is `Dropbox/Apps/<your app>/Work`, visible only to this app.
* Notes, attachments and any other visible file are synced. Hidden service
  folders (`.vault`, `.obsidian`, `.git`, `.DS_Store`, `._*`, `node_modules`,
  `target`) are excluded.
* `.trash` is optional and can be enabled per workspace.
* Files larger than 10 MiB stay on the device until the user presses **Send**
  in Settings → Sync. A large file that already exists in Dropbox is
  downloaded and keeps syncing.
* A local `records` SQLite database stores the last agreed base for every
  path: SHA-256 of the local file plus Dropbox `rev` and `content_hash`.
* Remote changes arrive through Dropbox `list_folder/continue` cursors. A
  missing/expired cursor falls back to a full listing.
* Both sides changed: Markdown/UTF-8 files up to 1 MiB are merged line by
  line. Overlapping local lines are preserved in a `(конфликт ...)` copy.
  Other files use a two-sided conflict copy (the remote version is preserved
  locally and uploaded on the next pass).
* Deletions are guarded: more than half of at least 10 files requires an
  explicit **Allow deletions** confirmation. A full listing that suddenly
  becomes empty stops the pass instead of deleting local files.
* Dropbox writes use `strict_conflict`, update by `rev`, and delete by
  `parent_rev`; the sync worker retries transient failures and refreshes the
  OAuth token once on 401.

## Setup

1. Open **Settings → Sync**.
2. Press **Connect Dropbox** and finish the browser sign-in.
3. Open the workspace you want to sync.
4. Enable **Sync this workspace** and choose an interval.

The OAuth client is a public PKCE client with loopback redirect
`http://127.0.0.1:47219/`. The built-in client id is
`hqch0u08opkzvpn`; it must be registered as an **App folder** app with that
exact redirect URI and the four `files.*` scopes listed below. To use a
different Dropbox app, follow the next section.

## Using your own Dropbox app

The app key is public and is compiled into the binary. To replace
`hqch0u08opkzvpn`, create a separate Dropbox app and build with
`VIBECODIUM_DROPBOX_CLIENT_ID=<app key>` (or edit `CLIENT.id` directly).

### 1. Create the app

1. Open <https://www.dropbox.com/developers/apps> → **Create app**.
2. Choose **Scoped access**.
3. Choose **App folder** — the least-privilege option. Dropbox creates one
   dedicated folder for this app, and the app cannot read or write anything
   outside it. The vault folder will be a subfolder of that app folder, e.g.
   `Dropbox/Apps/<your app>/<vault name>`.
   **Full Dropbox** also works, but grants access to the whole Dropbox; only
   choose it if you explicitly need top-level folders like `/Notes`.

   The sync code needs no changes for App folder mode: Dropbox API paths are
   relative to the app folder, so `/<vault name>` is simply a subfolder inside
   `Apps/<your app>/`. The app type is chosen at creation and cannot be
   converted later; switching from Full Dropbox to App folder means creating a
   new Dropbox app and reconnecting. Existing Full-Dropbox remote folders are
   not visible to the new App-folder app, so the first pass uploads a fresh
   baseline.
4. Open the app → **Permissions** and enable:
   * `files.metadata.read`
   * `files.metadata.write`
   * `files.content.read`
   * `files.content.write`
5. Press **Submit**.
6. Open **OAuth 2** → **Redirect URIs** and add exactly:

   ```
   http://127.0.0.1:47219/
   ```

   The trailing slash matters. If the console rejects loopback `127.0.0.1`,
   use `http://localhost:47219/` and set
   `VIBECODIUM_DROPBOX_REDIRECT_ADDRESS=localhost:47219` in the build
   environment below.
7. Copy the **App key**. The app secret is not needed for the PKCE public
   client; leave `VIBECODIUM_DROPBOX_CLIENT_SECRET` unset.

### 2. Point the build at the new app

`VIBECODIUM_DROPBOX_CLIENT_ID` and `VIBECODIUM_DROPBOX_REDIRECT_ADDRESS` are
read at **compile time**, so rebuild after changing them.

Windows PowerShell:

```powershell
$env:VIBECODIUM_DROPBOX_CLIENT_ID="your_app_key"
$env:VIBECODIUM_DROPBOX_REDIRECT_ADDRESS="127.0.0.1:47219"
cargo build --release
```

Linux/macOS:

```bash
VIBECODIUM_DROPBOX_CLIENT_ID=your_app_key \
VIBECODIUM_DROPBOX_REDIRECT_ADDRESS=127.0.0.1:47219 \
cargo build --release
```

Or edit the constants directly:

* `src/sync/dropbox/mod.rs` — `CLIENT.id` (and `CLIENT.secret` only for a
  confidential app);
* `src/sync/oauth.rs` — `ADDRESS`, used both for `TcpListener::bind` and for
  the redirect URI (`http://{ADDRESS}/`).

If you change the port, register the matching URI in the Dropbox console too.

### 3. Reconnect

Old tokens in `<app data>/sync/dropbox.json` belong to the old app key and
cannot be refreshed by the new app. Press **Disconnect** in
**Settings → Sync** and then **Connect Dropbox** again. Connecting also clears
the per-app sync state, so the first pass is a full baseline scan.

The token file is under the app data directory:

* Windows: `%APPDATA%\Vibecodium\sync\dropbox.json`
* macOS: `~/Library/Application Support/Vibecodium/sync/dropbox.json`
* Linux: `~/.local/share/Vibecodium/sync/dropbox.json`

## Files

```
<app data>/sync/
├── dropbox.json          OAuth tokens
├── <cache key>.sqlite3    per-workspace records/state
├── device                8-hex device id for conflict-copy names
└── staging/              temporary downloads
```

The cache key is SHA-256 of the canonical workspace path. Reconnecting a
Dropbox account resets the sync state (`start_over` removes the `sync`
folder), which produces a clean first pass.

## Architecture

```
SyncService (background thread)
└── worker::wait / pass
    └── cycle::run
        ├── measure::survey       local scan + large-file list
        ├── view::observe         cursor delta or full listing
        ├── decide::settle        Push/Pull/Delete/Conflict/Match
        ├── transfer::execute     4 parallel fetches, serial apply
        │   ├── apply::fetch      download/upload/delete
        │   └── apply::apply      write/trash/merge/records
        └── records.commit        cursor + scope
```

Writes go through the same per-path lock as the editor's `save_manager`, so a
sync write and a user save cannot interleave. The existing fs watcher then
reloads clean open documents, reports external changes on dirty ones, closes
tabs for remotely deleted notes, and refreshes the tree for new folders.

## Current limitations

* No end-to-end encryption: Dropbox sees file names and contents.
* Each changed file is uploaded whole; there is no block-level dedup.
* Downloaded content is checked by length, not by Dropbox content hash.
* Path keys are case-sensitive; a case-only rename on a case-insensitive
  filesystem may be treated as two paths.
* Two local workspaces with the same folder name map to the same Dropbox
  folder; there is no separate remote vault id.
* The `records` database has no schema version/migration yet.
* OAuth uses a fixed loopback port; if it is occupied, connection fails.

## Tests

`cargo test sync::` runs the unit tests for the merge, hashes, SQLite store,
scope, OAuth PKCE and the fake-Dropbox integration tests (push, pull, edits,
line merge, deletion guard, empty-cloud guard). The full app test suite also
covers the settings tab and i18n parity.
