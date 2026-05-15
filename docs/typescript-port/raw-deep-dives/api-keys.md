# Deep-Dive: API Keys Page

> Raw findings from Wave 1 agent. Source of truth for `20-features/feature-api-keys.md`.
> Generated 2026-05-15. Cite with file:line.

Path conventions used throughout this doc:
- Frontend: `/home/user/OpenBBPort/desktop/src/routes/api-keys.tsx`
- Rust handler: `/home/user/OpenBBPort/desktop/src-tauri/src/tauri_handlers/credentials.rs`
- FS/Env abstractions: `/home/user/OpenBBPort/desktop/src-tauri/src/tauri_handlers/helpers.rs`
- Wiring: `/home/user/OpenBBPort/desktop/src-tauri/src/main.rs`
- OpenBB Python model (for schema): `/home/user/OpenBBPort/openbb_platform/core/openbb_core/app/model/{user_settings.py,credentials.py}`

---

## 0. On-disk schema (`~/.openbb_platform/user_settings.json`)

Resolved location (Rust): `Path::new(home_dir).join(".openbb_platform").join("user_settings.json")` where `home_dir = env.var("HOME").or_else(env.var("USERPROFILE"))`. See `credentials.rs:9-15`, `:48-54`. No XDG fallback, no `dirs` crate — raw env-var lookup.

The Python side (`openbb_platform/core/openbb_core/app/model/user_settings.py:18-20`) defines:

```py
class UserSettings(Tagged):
    credentials: Credentials       # dict-shaped, keys are lower-cased provider names
    preferences: Preferences
    defaults: Defaults
```

`Credentials` is a dynamically-built Pydantic model whose fields are populated from the registered providers (see `credentials.py:38-80`). On disk the credentials block is a flat `{ "<provider_lowercase_key>": "<secret>" | null, ... }`. Keys are normalised to lower-case (`credentials.py:54-57`). Values serialise as plain strings via `OBBSecretStr`'s `PlainSerializer` (`credentials.py:30-35`).

Effective JSON shape the desktop UI cares about:

```json
{
  "credentials": {
    "polygon_api_key": "abc...",
    "fmp_api_key": null,
    "benzinga_api_key": "xyz..."
  },
  "preferences": { ... },
  "defaults":    { ... }
}
```

The desktop page never inspects `preferences` or `defaults`, but the Rust update path **preserves them** (read-modify-write semantics, see Section 4).

---

## 1. Mount-time load (read)

**Frontend trigger** — component mount.
- `api-keys.tsx:204-206` `useEffect(() => { loadData(); }, [])`. Empty deps array, no re-fetch on focus, no Tauri event listener. The list is **not reconciled** if the user edits `user_settings.json` externally via the "Open File" action.
- `loadData` body at `api-keys.tsx:176-201`.

**Invoke call** — `invoke<UserCredentialsResult>("get_user_credentials")` (`api-keys.tsx:182`).
- Payload: none.
- Return type (TS): `{ credentials?: Record<string, string | null | undefined> }` (`api-keys.tsx:14-16`).
- Return type (Rust): `Result<serde_json::Value, String>` — the **entire** parsed `user_settings.json`, not just the credentials block (`credentials.rs:6`, `:33`). Frontend just picks `.credentials`.

**Rust handler** — `get_user_credentials` (`credentials.rs:36-39`) → `get_user_credentials_impl` (`credentials.rs:3-34`).
- Resolves home dir from `HOME` / `USERPROFILE` env vars.
- If `user_settings.json` is missing → returns `{"credentials": {}}` (`credentials.rs:17-22`).
- Otherwise: `fs.read_to_string(...)`, `serde_json::from_str(...)`, return whole tree.
- File is read non-atomically with `std::fs::read_to_string` (see `helpers.rs:87-89`). No file lock taken on the read path (locking is only used in `toggle_theme_impl`, `helpers.rs:211`).

**State updates** (`api-keys.tsx:184-200`):
- `setLoading(true)` → on success, transform `credentials` object into `ApiKey[]` via `Object.entries(...).map(...)` where `null` becomes `""`. `required` is hard-coded `false` for every key (this field is essentially dead — there's no provider manifest).
- `setApiKeys(formattedKeys)`, `setError(null)` on success, `setLoading(false)` in `finally`.
- Error path: `setError("Failed to load API keys: " + err)` triggers the `useEffect` at `api-keys.tsx:466-472` which surfaces a Tauri dialog via `message(...)` (`@tauri-apps/plugin-dialog`).

**Display ordering** — Comes back in JS object-iteration order. `serde_json::Value::Object` defaults to alphabetical ordering unless `preserve_order` feature is enabled. The TS port should match whichever ordering `serde_json` uses to keep diff-reviewable behaviour. Currently this is **alphabetical sort** unless the Cargo.toml enables `preserve_order` for `serde_json` (worth verifying in your Cargo lock).

---

## 2. Add / Edit / Delete (write — single key flow)

All three actions converge on `saveApiKeys(updatedKeys)` (`api-keys.tsx:331-371`), which serialises the **entire** in-memory list and overwrites the credentials block.

### 2a. Add
- Trigger: "Add New Key" button, `api-keys.tsx:585-598`. Sets `modalMode='add'`, clears `newKey`, opens modal.
- Submit: `handleSaveKey` at `api-keys.tsx:217-245`.
  - Empty-name guard (`api-keys.tsx:218-221`).
  - Case-insensitive duplicate guard for **add** only (`api-keys.tsx:230-233`): `apiKeys.some(k => k.key.toLowerCase() === newKey.key.toLowerCase())`.
  - Prepends new entry: `updatedKeys = [{ ...newKey, required: false }, ...apiKeys]` (`api-keys.tsx:234`). New rows appear **at the top** in memory; on next mount they re-sort by serde ordering.
- Calls `saveApiKeys(updatedKeys)`.

### 2b. Edit
- Trigger: row "edit" icon → `handleEditKey(originalIndex)` (`api-keys.tsx:248-254`, button at `api-keys.tsx:696`).
- Modal opens prefilled; on save, `handleSaveKey` writes to `apiKeys[editingKeyIndex]` (`api-keys.tsx:226-227`). No duplicate check on rename — you can rename a key to collide; the dedup happens implicitly when serialised to the credentials object (`api-keys.tsx:353-361` uses `reduce` into a `Record`, last-write-wins).

### 2c. Delete
- Only available from inside the edit modal: `handleDeleteKeyFromModal` (`api-keys.tsx:257-271`). `splice` then `saveApiKeys`.
- There is **no inline-row delete button**. Delete is gated behind an Edit click.

### 2d. The shared write path — `saveApiKeys`
- `api-keys.tsx:331-371`:
  - Re-validates "no empty names" and "no duplicates" (case-sensitive here; differs from add-time check at line 230).
  - Builds `credentials = keysToSave.reduce(...)` into `Record<string, string>` (`api-keys.tsx:353-361`). **Empty values are kept as empty strings, not `null`** — opposite of how `loadData` decodes them.
  - `await invoke("update_user_credentials", { credentials })` (`api-keys.tsx:364`).
  - On success, `setApiKeys(keysToSave)` so the rows reflect the just-saved order.

**Rust handler** — `update_user_credentials` (`credentials.rs:88-91`) → `update_user_credentials_impl` (`credentials.rs:41-86`):
1. Resolve home dir (`credentials.rs:48-51`).
2. `mkdir -p ~/.openbb_platform` if missing (`credentials.rs:56-59`).
3. **Read existing settings** (preserves `preferences`/`defaults`/etc.):
   - If file exists: parse → `serde_json::Value` (`credentials.rs:62-68`).
   - Else: start with `{}` (`credentials.rs:70`).
4. Replace top-level `credentials` key (`credentials.rs:74-76`): `obj.insert("credentials", credentials_value)`.
5. `serde_json::to_string_pretty` then `fs.write(path, json)` (`credentials.rs:79-83`).

**Atomicity / safety concerns**:
- `fs.write` is `std::fs::write` (`helpers.rs:84-86`) — **non-atomic**. A power loss or kill mid-write can truncate `user_settings.json` and lose `preferences`/`defaults` along with credentials.
- No file lock is acquired (contrast with `toggle_theme_impl` at `helpers.rs:211` which uses `fs2::FileExt::try_lock_exclusive`).
- File permissions: inherits `umask`. On Unix this is usually `0644` — **world-readable secrets file**. Nothing in the handler calls `set_permissions` to chmod 0600.
- Concurrent writes from the OpenBB Python process and the desktop UI are not coordinated; last writer wins.

**State updates after write**:
- `setApiKeys(keysToSave)` only on success (`api-keys.tsx:366`).
- On thrown error: `setError(...)` (`api-keys.tsx:368-370`) — the in-memory list still shows pre-write state (good), but the modal has already closed (`handleSaveKey` resets and closes at `api-keys.tsx:238-241` **before** awaiting). User loses unsaved input on failure.

---

## 3. Modal state machine

State variables (`api-keys.tsx:23-39`):
- `isAddKeyModalOpen` (boolean) — modal mounted/unmounted.
- `editingKeyIndex` (`number | null`) — index into `apiKeys` for edit mode.
- `modalMode` (`'add' | 'edit'`) — drives title text and presence of Delete button (`api-keys.tsx:757`, `:868-877`).
- `newKey` (`{key, value}`) — the controlled-input buffer.
- `isModalValueVisible` (bool) — toggles between `<input type="password">` and `<textarea>` for the value field (`api-keys.tsx:835-861`).
- `modalCopied` (bool) — 2-second checkmark feedback for in-modal Copy.

Reset/cancel paths:
- Escape key handler (`api-keys.tsx:475-490`) closes modal and clears form.
- Cancel button (`api-keys.tsx:881-889`) — same reset.
- Close (X) icon (`api-keys.tsx:763-773`) — same reset.
- After successful save: reset happens **before** the await (`api-keys.tsx:238-241`).

---

## 4. Open-file-in-editor flow

**Frontend trigger** — Settings (file icon) button opens `isSettingsModalOpen` (`api-keys.tsx:624-632`), the user picks one of five files via radios (`api-keys.tsx:967-998`), then clicks "Open File" which switches on `selectedSettingsFile` (`api-keys.tsx:1004-1027`).

The five wrapper functions:
- `openUserSettings` (`api-keys.tsx:373-381`) → `invoke("open_credentials_file", { fileName: "user_settings.json" })`
- `openSystemSettings` (`api-keys.tsx:383-393`) → `... fileName: "system_settings.json"`
- `openMcpSettings` (`api-keys.tsx:415-423`) → `... fileName: "mcp_settings.json"`
- `openEnvFile` (`api-keys.tsx:395-403`) → `... fileName: ".env"`
- `openCondarcFile` (`api-keys.tsx:405-413`) → `... fileName: ".condarc"`

**Invoke call** — `open_credentials_file` with `{ fileName: string }`. Returns `Result<bool, String>` (`credentials.rs:184`).

**Rust handler** — `open_credentials_file_impl` (`credentials.rs:93-181`):
1. Resolves home dir via `get_home_directory_impl` (`helpers.rs:1369-1373`) which uses `env_sys.home_dir()` (`std::env::home_dir()` — see `helpers.rs:166-168`). Note this is **different** from `get_user_credentials_impl` which uses `env.var("HOME")` directly. Inconsistency.
2. Resolves `file_path`:
   - `.condarc` → `<installation_dir>/conda/.condarc`. `installation_dir` comes from `system_settings.json["install_settings"]["installation_directory"]` (`helpers.rs:627-650`).
   - Everything else → `~/.openbb_platform/<file_name>`.
3. **Auto-create with default content** if missing (`credentials.rs:121-134`):
   - `user_settings.json` → `{"credentials": {}}`
   - `system_settings.json` → `{}`
   - `mcp_settings.json` → `{}`
   - `.env` → `# Environment variables for OpenBB Platform`
   - `.condarc` → `# Conda configuration file\nchannels:\n  - conda-forge\n  - defaults`
4. Spawn an editor process (`credentials.rs:137-178`):
   - **Windows**: `notepad.exe <path>` (`credentials.rs:140-144`).
   - **macOS**: `open -a TextEdit <path>` (`credentials.rs:151-154`).
   - **Linux**: tries `gedit`, `kate`, `leafpad`, `mousepad`, `xed` in that order; if none spawns successfully, falls back to `xdg-open` (`credentials.rs:160-177`). Note: `.spawn().is_ok()` returns true even if the editor is broken/missing-deps; only `command-not-found`-style errors fall through.
5. Returns `Ok(true)` immediately after spawn (does not wait for editor close, does not refresh the page).

**Behavioural quirk**: spawning an external editor means the user can edit `user_settings.json` outside the app and **the page will not pick up the change**. There is no FS watcher and no `loadData` on focus.

**Validation gap for `fileName`**: The handler matches on the literal string and otherwise joins it as a relative path under `~/.openbb_platform`. There's no allow-list check **on the Rust side** — only the frontend constrains it via the radio. A malicious frontend or tauri-API caller could pass `../something` or absolute paths and exfiltrate / open arbitrary files. The default-content fallback `_ => "{}"` (`credentials.rs:127`) means it would also overwrite an arbitrary JSON path with `{}` if the file is missing.

> ⚠️ BUG: path-traversal vulnerability in `open_credentials_file`. Fix in port via strict allow-list.

---

## 5. Import flow (.env / .json)

### Trigger
- "Import Keys" button (`api-keys.tsx:603-611`) → triggers a hidden `<input type="file" accept=".json,.env">` (`api-keys.tsx:612-618`) via `fileInputRef.current?.click()`.
- `handleFileInputChange` (`api-keys.tsx:439-455`): validates extension, dispatches to `parseImportedFile(file)`, then clears the input.

### Parser — pure frontend, no Rust call
`parseImportedFile` lives entirely in the browser (`api-keys.tsx:46-145`). The file is read via `file.text()` (Web API).

**JSON branch** (`api-keys.tsx:52-95`):
- `JSON.parse(text)`.
- If `jsonData.credentials` exists and is an object → iterate **only the credentials sub-tree** (matches OpenBB schema). Skip `null`/`undefined`. Cast values via `String(value)`.
- Else treat as a flat `{key: value}` map and pull every entry.
- Throws on parse failure with cause-attached `Error`.

**.env branch** (`api-keys.tsx:96-125`):
- Splits by `\n` (LF only — Windows CRLF lines may carry stray `\r` in the value before the quote-strip; the `value.trim()` at `api-keys.tsx:108` removes trailing `\r`, so it's actually fine on CRLF).
- Skips blank lines and lines starting with `#` (so leading whitespace + `#` comments are recognised because of the prior `trim()`).
- Regex: `^([^=]+)=(.*)$` — splits on the first `=`. Value can contain `=`. Multi-line values are **not supported** (each newline is a fresh line; no quote-spanning multiline). Backslash escapes are not interpreted.
- Strips wrapping single or double quotes (`api-keys.tsx:111-116`). Does **not** unescape `\n`, `\t`, etc.
- `export KEY=val` syntax is **not stripped** — the key would parse as `"export KEY"`.

**Other extensions** → throws "Unsupported file format".

### Confirmation modal
- After parse success, populates `importedKeys`, sets `selectedKeys = new Set(all keys)` (default-all-checked), opens `isImportConfirmModalOpen` (`api-keys.tsx:132-138`).
- Modal markup at `api-keys.tsx:1040-1134`: table with row checkboxes, per-row visibility toggle (`importVisibleKeys` Set, `api-keys.tsx:318-328`), select-all checkbox, count in submit button label.
- "Cancel" closes without merging.

### Confirm import (merge + save)
`handleConfirmImport` (`api-keys.tsx:274-298`):
- Filter to selected keys.
- Merge into `apiKeys`: existing key (case-sensitive `findIndex`) → overwrite value; missing → push. **Case-sensitive** match here vs. case-insensitive at add-time (`api-keys.tsx:230`). Inconsistency.
- `await saveApiKeys(mergedKeys)` → standard `update_user_credentials` write path.
- Resets `importedKeys`, `selectedKeys`, closes modal.

### Helper toggles
- `handleToggleSelectAll` (`api-keys.tsx:300-306`): Set length parity with `importedKeys.length` decides clear-vs-select-all.
- `handleToggleKeySelection` (`api-keys.tsx:308-316`): immutable `Set` add/delete pattern.
- `toggleImportKeyVisibility` (`api-keys.tsx:318-328`): same pattern for visibility.

---

## 6. Visibility, copy, search

### Per-row visibility (main list)
- `visibleKeys: Set<string>` (`api-keys.tsx:32`).
- `toggleKeyVisibility(key)` (`api-keys.tsx:493-503`): immutable Set toggle.
- Display logic (`api-keys.tsx:686-690`): when value present and key in `visibleKeys` → show plaintext; else show `"********************"` (literal 20 stars). When value is empty → show `"Undefined"`.
- Toggle button disabled when value is empty (`api-keys.tsx:708`).

### Modal value visibility
- `isModalValueVisible` flips between `<input type="password">` (sensitive autofill-friendly) and `<textarea>` (`api-keys.tsx:835-861`). Switching to textarea is the only way to **wrap and review long keys** (e.g. JWTs).

### Copy to clipboard
- Row copy (`copyToClipboard`, `api-keys.tsx:148-160`): `navigator.clipboard.writeText(value)`. On success, `setCopiedKey(keyName)` for a 2-second checkmark (`api-keys.tsx:152-154`). On failure, surfaces an alert via the `error` channel.
- Modal copy (`copyModalValueToClipboard`, `api-keys.tsx:162-174`): same pattern with `modalCopied`.
- Both paths use the **browser** clipboard API, not `tauri-plugin-clipboard-manager`. The secret never crosses the IPC boundary on copy. Browser clipboard contents may persist; macOS Universal Clipboard may sync to other Apple devices.

### Search / filter
- `searchQuery` state (`api-keys.tsx:26`), search box at `api-keys.tsx:550-557`.
- `filteredApiKeys = useMemo(...)` (`api-keys.tsx:209-214`): case-insensitive `includes` match against `key.key`. Values are **not** searched (sensible — would leak through tooltips).
- Clear-button at `api-keys.tsx:563-569`. Empty-state at `api-keys.tsx:911-932`.

### Documentation button
- `openDocumentation` (`api-keys.tsx:425-436`) → `invoke("open_url_in_window", { url: "https://docs.openbb.co/desktop/api_keys", title: "Open Data Platform Documentation" })`.
- Rust impl (`helpers.rs:957-1020`): builds a new `WebviewWindow` with label `"url_<unix_ms>"`, sets a transparent title bar on macOS, sets background to opaque black via `objc2_app_kit::NSColor`, intercepts `CloseRequested` to call `destroy()`. The title (passed in plain) becomes the OS window title.

---

## 7. Error UX

- Single `error: string | null` state (`api-keys.tsx:22`). Any code path sets it.
- `useEffect([error])` (`api-keys.tsx:466-472`) → `await message(error, { title: "OpenBB", kind: "error" })` → on dismiss, `setError(null)`. So errors always become a native dialog, never an inline banner. Note: errors interpolate `${err}` from `invoke` rejections — Rust string errors are dialog-displayed verbatim.

---

## 8. Layout / scrollbar handling
- `ResizeObserver` (`api-keys.tsx:505-531`) syncs header `padding-right` with the scrollbar width to keep columns aligned. Trivial for the port.

---

## 9. Wiring (main.rs)

- Imports: `main.rs:38-40`.
  ```rust
  use crate::tauri_handlers::credentials::{
      get_user_credentials, open_credentials_file, update_user_credentials,
  };
  ```
- Registered in `tauri::generate_handler!` macro: `main.rs:528-531` (`get_user_credentials`, `open_credentials_file`, `update_user_credentials`, `open_url_in_window`).
- Tray menu also exposes "API Keys" → `navigate_to_page(.., "/api-keys")` at `main.rs:606`, `:660`.

---

## 10. TypeScript-port translation table

| Tauri invoke | Direction | Payload | Returns | TS-port equivalent |
|---|---|---|---|---|
| `get_user_credentials` | read | none | `{ credentials, preferences, defaults, ... }` (full settings tree) | `fs.readFile(join(homedir(), '.openbb_platform/user_settings.json'), 'utf8')` → `JSON.parse` → return whole tree, default to `{credentials:{}}` if ENOENT |
| `update_user_credentials` | write | `{ credentials: Record<string, string> }` | `boolean` | Read existing settings tree (or `{}`), set `tree.credentials = payload`, `JSON.stringify(tree, null, 2)`, atomic write (write to `*.tmp` then `rename`), `chmod 0o600` |
| `open_credentials_file` | spawn | `{ fileName: 'user_settings.json' \| 'system_settings.json' \| 'mcp_settings.json' \| '.env' \| '.condarc' }` | `boolean` | Resolve path (allow-list **only** these names); ensure-exists with default content; spawn editor by platform (Win: `notepad.exe`, mac: `open -a TextEdit`, Linux: try gedit/kate/leafpad/mousepad/xed → fallback `xdg-open`) |
| `open_url_in_window` | UI | `{ url, title? }` | `void` | If your TS runtime is Electron: `new BrowserWindow({...}).loadURL(url)`. If Node + headless: `import('open').then(o => o.default(url))`. For both, **never** put secrets in `title`. |
| (none — clipboard is browser API) | local | `value: string` | — | Keep `navigator.clipboard.writeText` in renderer; don't relay through main process. |
| (none — file parsing is browser-side) | local | `File` | parsed `ApiKey[]` | Keep parser in TS. The `.env` regex `/^([^=]+)=(.*)$/` and `JSON.parse` flow ports verbatim. |

### Path resolution constants (TS)
```ts
const HOME = process.env.HOME ?? process.env.USERPROFILE!;
const PLATFORM_DIR = path.join(HOME, '.openbb_platform');
const USER_SETTINGS = path.join(PLATFORM_DIR, 'user_settings.json');
// .condarc lives under <installation_dir>/conda/.condarc, where installation_dir =
//   JSON.parse(read(PLATFORM_DIR/system_settings.json)).install_settings.installation_directory
```

---

## 11. Security pitfalls callout — DO NOT REPEAT IN THE PORT

> **Read this before touching the port.**
>
> 1. **No event payloads with secrets.** The current Rust implementation only returns secrets via the direct `invoke` reply. Do **not** broadcast credentials over `tauri::Event` / `webContents.send` / SSE. If you add a "credentials updated" notification, send only key **names**, not values.
> 2. **No secrets in window titles or labels.** `open_url_in_window` constructs window labels like `url_<timestamp>` — never put a key value in a label, title, or URL fragment. OS-level window enumeration (Spotlight, Mission Control, AltTab, accessibility APIs) can read titles.
> 3. **Atomic write is missing today.** Port should write `user_settings.json.tmp` then `rename(2)` to avoid corrupting `preferences`/`defaults` on crash. The current Rust uses `std::fs::write` directly (`helpers.rs:84-86`) — a known foot-gun.
> 4. **File permissions are not hardened.** The Rust handler relies on umask; on most Linux installs the file ends up `0644`. The port MUST `chmod 0o600` after every write (Unix only). Windows NTFS ACL hardening is harder; document the limitation.
> 5. **Path traversal in `open_credentials_file`.** The Rust handler trusts the `fileName` argument and joins it under `~/.openbb_platform`. The TS port should reject any string not in the literal allow-list `{user_settings.json, system_settings.json, mcp_settings.json, .env, .condarc}` — the frontend already restricts but defence-in-depth matters.
> 6. **Missing-file auto-create can clobber.** Rust writes `default_content` if the path doesn't exist. Combined with #5 this means a hostile call could write `{}` over an arbitrary missing path. Validate first.
> 7. **Clipboard exfiltration risk.** Both row-Copy and modal-Copy use `navigator.clipboard`. macOS Universal Clipboard syncs across iCloud devices; Windows clipboard history (Win+V) retains entries for ~24 h. Consider documenting this and offering a "Copy and clear in N seconds" option (set a `setTimeout` that calls `clipboard.writeText('')`).
> 8. **Logs.** `log::debug!("Opening URL in a new window: {url}")` (`helpers.rs:963`) — fine because `url` is the docs URL here, but **do not** add `log::debug!` lines that interpolate `credentials` or even key counts in a way that could be diff-fingerprinted. Keep credentials out of `tauri-plugin-log` targets entirely.
> 9. **No FS watcher / no focus reload.** If your port adds a watcher to detect external edits to `user_settings.json`, debounce and **diff** before re-rendering — naively re-reading on every fs event will race with `update_user_credentials` writes.
> 10. **Inconsistent dedup semantics.** Add-time uses case-insensitive comparison (`api-keys.tsx:230`), import-time uses case-sensitive (`api-keys.tsx:285`), serialise-time uses case-sensitive last-write-wins via `Object.assign`-style reduce (`api-keys.tsx:353-361`). The OpenBB Python core lower-cases everything (`credentials.py:54`). Recommendation: lower-case all keys at the IPC boundary in the port to match the platform's expectations and eliminate ambiguity.
> 11. **No file lock.** Concurrent writes from a Python OpenBB process and the desktop UI will race. The port should take an exclusive flock on `user_settings.json` for the read-modify-write cycle (similar to `toggle_theme_impl` at `helpers.rs:211`).
> 12. **Modal closes before await.** `handleSaveKey` (`api-keys.tsx:238-244`) closes the modal and clears `newKey` **before** awaiting `saveApiKeys`. If the IPC fails, the user has lost their input. The port should keep the modal open until the write resolves.

---

## Cross-feature dependencies

- **shares-state-with** `feature-installation.md` via `~/.openbb_platform/user_settings.json` — installation writes the initial empty `credentials: {}`; this page reads/writes the same file forever after. Also `system_settings.json` for resolving `.condarc` path under `<installation_directory>/conda/`.
- **shares-state-with** `feature-platform-rest-api.md` — the Python `openbb-api` server reads `credentials` from the same file at startup and uses them to authenticate with data providers. A write here invalidates the running API's in-memory credentials unless the API is restarted.
- **shares-state-with** `feature-environments.md` via `.condarc` (read by conda inside any env).
- **independent-of** `feature-backend-services.md` (does not consult backend service definitions).
- **OpenBB Python core schema:** `openbb_platform/core/openbb_core/app/model/user_settings.py:18-20` defines `UserSettings`; the dynamic `Credentials` model at `credentials.py:38-80` is the source of truth for which provider keys are valid.

---

### Key file:line citations index
- Frontend triggers: `api-keys.tsx:176-206` (load), `:217-271` (add/edit/delete), `:274-298` (import), `:331-371` (save), `:373-423` (open file), `:425-436` (open docs), `:439-455` (file input), `:493-503` (visibility), `:148-174` (clipboard).
- Rust commands: `credentials.rs:36-39` (`get_user_credentials`), `:88-91` (`update_user_credentials`), `:183-186` (`open_credentials_file`).
- Rust impls: `credentials.rs:3-34`, `:41-86`, `:93-181`.
- Helpers: `helpers.rs:60-129` (`RealFileSystem`), `:134-169` (`RealEnvSystem`), `:627-650` (installation dir resolution for `.condarc`), `:957-1020` (`open_url_in_window`).
- Wiring: `main.rs:38-40` (imports), `:528-531` (handler registration), `:606`/`:660` (tray menu navigation).
- OpenBB schema: `openbb_platform/core/openbb_core/app/model/user_settings.py:18-20`, `credentials.py:38-80`.
