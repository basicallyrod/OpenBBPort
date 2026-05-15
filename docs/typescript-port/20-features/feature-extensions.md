# Feature: Extensions

> Owns: install / update / remove Python packages ("extensions") into an
> existing conda env. Does NOT own env creation (see `feature-environments.md`)
> or the Python REST server that consumes them (see `feature-platform-rest-api.md`).

## Purpose

The desktop curates a set of Python packages inside each managed conda env so
the embedded OpenBB Platform server can expose their FastAPI routes. This
feature is the pip/conda glue between the catalog JSON, the env-on-disk, and
the per-env `<env>.yaml` manifest used by later operations.

## User flows

1. **Golden — add to existing env.** "Extensions" on env card → "Add Extension"
   tab (`AddExtensionSelector.tsx`) → pick categories / type free-text → "Install".
   `handleInstallExtensions` (`environments.tsx:1361-1427`) fires
   `install_extensions`. On success `refreshEnvironmentUIState` re-fetches via
   `get_environment_extensions`.
2. **Wizard Step 3 — first install.** Same handler family, different selector
   (`InstallComponents.tsx::ExtensionSelector`), called by
   `installation-progress.tsx:1137-1184`. Targets the freshly-created `openbb` env.
3. **Remove single / Update single.** Trash / refresh icons →
   `handleRemoveExtension` (`environments.tsx:1430-1459`) /
   `handleUpdateExtension` (`environments.tsx:1461-1507`). Update tries pip
   upgrade then falls back to conda. YAML is mutated on remove, untouched on update.
4. **Edge — concurrent install.** Only guard is `installExtensionsLoading`
   button-disable. Conda's lockfile protects env data; YAML merge is racy.
5. **Edge — cancel mid-install.** `handleCancelExtensionInstall`
   (`environments.tsx:1726-1728`) hides the modal only; backend runs to
   completion.

## UI surface

| Surface | File:line | Notes |
|---|---|---|
| Add Extension modal (existing env) | `components/AddExtensionSelector.tsx` | Loaded from `environments.tsx:2576-2581` |
| Extension selector (wizard) | `components/InstallComponents.tsx::ExtensionSelector` | Step 3 of create-env modal |
| Tabs | `conda`, `extras`, `provider`, `router`, `other-openbb` | First two free-text; rest checklists |
| Conda channel field | `AddExtensionSelector.tsx:203-213` | Builds `${channel}:${pkg}` |
| Extension row | `components/ExtensionRow.tsx` (`environments.tsx:213-227`) | Trash + refresh icons |
| Error banners | `extensionsError`, `extensionRemoveError`, `updateExtensionError` (`environments.tsx:2617-2682`) | Inline in panel |

### The catalog (frontend-only, no Tauri, no caching)

Both selectors `fetch()` on every modal open from three public OpenBB GitHub raw URLs:
`https://raw.githubusercontent.com/OpenBB-finance/OpenBB/refs/heads/main/assets/extensions/{provider,router,obbject}.json`.
Entry shape (`installation-progress.tsx:23-29`):
`{ packageName, reprName?, description?, credentials?: string[], instructions?: string|null }`.
Four UI categories plus hard-coded `extrasExtensions`:

| Category | Source | Wire format on install |
|---|---|---|
| `conda` | Free-text user input + channel dropdown | `conda:<channel>:<pkg>` |
| `extras` | Hard-coded: `openbb-mcp-server`, `pywry`, `openbb-cli`, `openbb-cookiecutter` (last only in `InstallComponents.tsx`) | plain `<pkg>` (PyPI) |
| `provider` | `provider.json` | plain `<pkg>` (PyPI) |
| `router` | `router.json` + `obbject.json` | plain `<pkg>` (PyPI) |
| `other-openbb` | leftover / hard-coded | plain `<pkg>` (PyPI) |

The literal `openbb` (case-insensitive) is intercepted by Rust and uses the
special `--no-deps + openbb-build` path (see below).

## Data flow

```mermaid
flowchart TD
  UI[Add Extension selector] -- fetch on modal open --> catalog[GitHub raw JSON<br/>provider/router/obbject]
  UI -- builds string[] in wire format --> encode

  subgraph encode[wire format]
    A[conda:&lt;channel&gt;:&lt;pkg&gt;]
    B[plain PyPI string]
    C["openbb literal"]
  end

  encode -- invoke install_extensions --> handler[install_extensions_impl<br/>environments.rs:2344-2679]
  handler --> split[split into conda_pkgs / pip_pkgs / has_openbb]
  split -- conda_pkgs --> condaCmd[conda install -n &lt;env&gt; -y &lt;pkgs&gt;]
  split -- pip_pkgs --> pipCmd[&lt;env_python&gt; -m pip install &lt;pkgs&gt;]
  split -- has_openbb --> openbbCmd["pip install openbb --no-deps<br/>then openbb-build"]
  condaCmd --> yaml[read &lt;env&gt;.yaml<br/>merge<br/>save_environment_as_yaml_impl]
  pipCmd --> yaml
  openbbCmd --> yaml
  yaml --> done[returns bool]
  done --> ui2[refreshEnvironmentUIState → get_environment_extensions → cache]
```

## IPC contract

| Direction | Name | Payload (TS) | Returns | Used by |
|---|---|---|---|---|
| TS → Rust | `install_extensions` | `{ extensions: string[], environment: string, directory: string }` | `boolean` | env page "Add Extension", wizard step 3 |
| TS → Rust | `remove_extension` | `{ package: string, environment: string, directory: string }` | `boolean` | env page row trash |
| TS → Rust | `update_extension` | `{ package: string, environment: string, directory: string }` | `boolean` | env page row refresh |
| TS → Rust | `get_environment_extensions` | `{ name: string }` | `{ extensions: Extension[] }` | post-mutation refresh; cache hydrate |

`Extension = { package: string; version: string; install_method: "pip" \| "conda"; channel: string }`.

> ⚠️ BUG: `directory` is accepted by the frontend on all three mutating
> commands but the Rust signatures ignore it and re-read the install dir from
> `system_settings.json` (`environments.rs:2682-2687`).

### Wire-format encoding parser (Rust side)

`install_extensions_impl` (`environments.rs:2390-2422`): starts with `conda:` →
strip prefix, push **verbatim rest** to `conda_packages` (e.g. `conda-forge:numpy`);
equals `openbb` (case-insensitive) → set `has_openbb=true`, skip both lists;
otherwise → push to `pip_packages`.

`remove_extension` mirror parser (`environments.rs:2059-2070`) splits on first
`:` → contains `:` becomes `("conda", &package[idx+1..])` (drops channel for
`conda remove`); no `:` becomes `("pip", package)`. So the on-disk YAML stores
`<channel>:<pkg>` for conda items and the parser strips it for the actual
`conda remove` call.

## State surfaces

- **React state** (in `environments.tsx`): `extensions`, `environmentPackages`,
  `installExtensionsLoading`, `updatingExtension`, `removingExtension`,
  `extensionSelectorKey` (forces selector remount after install).
- **Rust state**: none. Handlers are stateless; every call re-reads disk.
- **Disk files**: `~/.openbb_platform/system_settings.json` (install dir);
  `~/.openbb_platform/environments/<env>.yaml` (mutated);
  `<install>/conda/envs/<env>/...` (modified via conda).
- **localStorage**: `env-extensions-cache` — frontend mirror,
  `{[env]: {extensions, pythonVersion}}`, no TTL (owned by `feature-environments.md`).

## Persistence — the `<env>.yaml` rewrite

`save_environment_as_yaml_impl` (`helpers.rs:436-503`) writes a conda-env file:

```yaml
name: <env>
channels: [defaults, conda-forge, ...]
dependencies:
  - python=3.12
  - numpy              # conda; optionally <channel>:<name>=<ver>
  - pip
  - pip:
    - openbb-yfinance
    - openbb-platform-api
```

On install (`environments.rs:2620-2648`): parse existing YAML, split each new
package on `=`/`<`/`>` for its name-part, drop existing entries matching that
name, append the new ones. Last-writer-wins. See Known Bugs for the missing-YAML
fall-through and the version-pin parser hole.

## The `openbb` special case — the only special handler

When the extensions array contains the bare string `"openbb"` (case-insensitive),
the Rust handler runs two extra commands (`environments.rs:2480-2533`):

1. `<env_python> -m pip install openbb --no-deps` — the `openbb` PyPI package
   is a meta-package that would pull in every provider at a pinned version;
   the desktop wants to manage those itself.
2. `<install>/conda/envs/<env>/bin/openbb-build` (or `Scripts/openbb-build.exe`).

What `openbb-build` does (`openbb_platform/core/openbb_core/build.py`):
runs `<sys.executable> -c "import openbb"`, which triggers
`PackageBuilder.auto_build()`. That walks every registered router's command
signature and generates a static Python module tree under
`<openbb>/static/package/` so user code can `from openbb import obb` with IDE
completion. Acquires `flock` on `<openbb>/static/.build.lock` (concurrent
calls error). Writes `<openbb>/static/assets/reference.json` (used by future
`auto_build()` to short-circuit). Takes 30-90s on a full extension set, with
**no streaming** to the frontend.

## Error handling

Rust returns `Result<bool, String>` formatted as
`"Failed to install pip packages: \nStdout: ...\nStderr: ..."`. Frontend:

- `isFutureWarningOnly` (`environments.tsx:79-89`) — contains `"FutureWarning:"`
  AND none of `"Error:"` / `"failed"` / `"Pip subprocess error:"` → treated as
  success, warning surfaced.
- `isPipSubprocessError` (`environments.tsx:91-94`) — contains
  `"Pip subprocess error:"` → surfaces verbatim (pip writes errors to stdout;
  `extractStderr` knows this).
- Else → red banner with raw string + Retry / Dismiss.

Cancel is modal-hide only; no abort signal reaches the backend.

## ▸ Interfaces with

- **depends-on** `feature-environments.md` — env must exist on disk; env page
  owns the `<env>.yaml` location and the `env-extensions-cache` localStorage
  schema.
- **depends-on** `feature-installation.md` — wizard Step 3 IS this feature,
  called from `InstallComponents.tsx` instead of `AddExtensionSelector.tsx`.
  Wizard also runs an explicit `execute_in_environment("openbb-build")` after
  (footgun above).
- **depended-on-by** `feature-platform-rest-api.md` — installed packages are
  what the REST server imports at boot.
- **depended-on-by** `feature-jupyter.md` — presence of `notebook`/`jupyter`/
  `jupyterlab` enables the "Open in Jupyter" button.
- **shares-state-with** `feature-environments.md` via `<env>.yaml` and
  `localStorage["env-extensions-cache"]`.

## TS port mapping

| Tauri call | TS-server equivalent | Notes |
|---|---|---|
| `install_extensions` | `POST /environments/:env/extensions` (body: `{ extensions: string[] }`) | Long-running; stream via WS/SSE keyed by server-generated `installId`. Per-env serialization (see Open Q's). Re-read install dir from `system_settings.json`; ignore client-supplied dir. |
| `remove_extension` | `DELETE /environments/:env/extensions/:pkg` | URL-encode `:pkg` — conda packages contain `:`. |
| `update_extension` | `PATCH /environments/:env/extensions/:pkg` | Pip first, conda fallback. |
| `get_environment_extensions` | `GET /environments/:env/extensions` | Read `<env>.yaml` + `conda list --name <env> --json`; merge. |
| catalog fetch (frontend) | unchanged — direct `fetch()` to GitHub raw | CORS works; cache in IndexedDB with TTL. |

### Wire-format parser (port-critical)

```ts
type ExtensionSpec =
  | { kind: "conda"; channel: string; pkg: string }
  | { kind: "openbb" }                       // bare "openbb": --no-deps + build
  | { kind: "pip"; pkg: string };

function parseExtensionString(s: string): ExtensionSpec {
  if (s.startsWith("conda:")) {
    const rest = s.slice("conda:".length);
    const idx = rest.indexOf(":");
    if (idx < 0) return { kind: "conda", channel: "conda-forge", pkg: rest };
    return { kind: "conda", channel: rest.slice(0, idx), pkg: rest.slice(idx + 1) };
  }
  if (s.toLowerCase() === "openbb") return { kind: "openbb" };
  return { kind: "pip", pkg: s };
}
```

### Shell-out reproduction

Every spawn must use the conda env-var preamble (`CONDA_ROOT`/`CONDA_ENVS_PATH`/
`CONDA_PKGS_DIRS`/`CONDARC` set; `CONDA_DEFAULT_ENV`/`CONDA_PREFIX`/
`CONDA_SHLVL` **unset** — see `environments.v2.md §12`). On Windows: set
`CREATE_NO_WINDOW (0x08000000)`. Capture stdout AND stderr — pip errors go to
stdout. Commands: `conda install -n <env> -y <pkgs>`,
`<env_python> -m pip install <pkgs>`,
`<env_python> -m pip install openbb --no-deps`, `<env>/bin/openbb-build`,
`conda remove -n <env> <pkg> -y`, `<env_python> -m pip uninstall <pkg> -y`,
`<env_python> -m pip install --upgrade <pkg>`, `conda install -n <env> <pkg> -y`
(update fallback), `conda list --name <env> --json` (for list).

## Known bugs and port-time fixes

> ⚠️ BUG 1: `directory` payload is ignored by all three mutating handlers
> (`environments.v2.md §6.8`). Port: drop param or honour it.

> ⚠️ BUG 2: Concurrent `install_extensions` race on the YAML rewrite
> (`environments.md §6.5`). Port: per-env handler-level lock.

> ⚠️ BUG 3: Missing `<env>.yaml` → install succeeds but YAML merge silently
> skipped; later `update_environment` hard-fails (`environments.v2.md §4`).
> Port: synthesise a minimal YAML if absent.

> ⚠️ BUG 4: Wizard step 3's standalone `openbb-build` call fails by default
> because `install_extensions` never installs the binary (bare `"openbb"`
> isn't in the default set — `installation.v2.md §4.3`). Port: detect any
> `openbb-*` package and run `openbb-build` then.

> ⚠️ BUG 5: Cancel only hides the modal; backend keeps running. Port: track
> spawned child PID and signal on cancel.

> ⚠️ BUG 6: Catalog re-fetches on every modal open. Port: cache in IndexedDB
> with TTL.

> ⚠️ BUG 7: Version-pin parser doesn't handle `~=`, `===`, `!=` — duplicate
> YAML entries (`environments.v2.md §3`). Port: PEP 440 specifier parser.

> ⚠️ BUG 8: `install_extensions` emits no `process-output` stream
> (`installation.v2.md §4.2`). User sees only a spinner for a 5-10 min
> install. Port: stream pip/conda output per-request.

## Open questions

1. **Should the `<env>.yaml` rewrite be transactional?** Today's read → mutate
   → write back leaves YAML out of sync with conda on crash. Options: atomic
   `.tmp` + rename; journal under `~/.openbb_platform/environments/.journal/`;
   demote YAML to a regen-on-demand cache (breaks its role as seed for
   `update_environment` / `create_environment_from_requirements`).
2. **Should the port serialize concurrent installs per env?** Per-env
   `AsyncMutex` minimum. Reject with 409 "busy" or queue? Queuing lets the
   user stack confusing operations.
3. **Catalog source of truth.** Frontend-direct fetch, proxy through the TS
   server (cache + offline), or bundle as static assets refreshed on app
   update?
4. **Always run `openbb-build` after any `openbb-*` install?** Eliminates Bug
   4 at 30-90s extra per touch.
5. **Encoding ambiguity.** `<channel>:<name>` breaks if a conda version
   string contains `:`. Switch to structured `{kind, channel, name, version}` JSON?
6. **Consolidate the dual selector?** `AddExtensionSelector.tsx` and
   `InstallComponents.tsx::ExtensionSelector` are near-duplicates.

## Cross-feature dependencies

- **depends-on** `feature-environments.md`, `feature-installation.md`.
- **depended-on-by** `feature-platform-rest-api.md`, `feature-jupyter.md`.
- **shares-state-with** `feature-environments.md` via
  `~/.openbb_platform/environments/<env>.yaml` and
  `localStorage["env-extensions-cache"]`.
