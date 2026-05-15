// Entry point — wires the buttons in index.html to the typed wrappers in
// `./ipc.ts` and subscribes to the three event channels the shell emits.
//
// Counting the distinct commands executed by `runAll()` plus the standalone
// handlers, this file exercises 30 IPC commands:
//
//   1. get_installation_state
//   2. get_app_version
//   3. navigate_to_page (→ /dashboard)
//   4. navigate_to_page (→ /)              [second call, "back"]
//   5. register_process_monitoring
//   6. get_process_logs_history
//   7. clear_process_logs_history
//   8. get_home_directory
//   9. select_directory                   (dialog — only when user clicks)
//  10. check_directory_exists
//  11. get_user_credentials
//  12. update_user_credentials
//  13. open_credentials_file
//  14. obb_set_base_url
//  15. obb_get_base_url
//  16. obb_health
//  17. obb_widgets
//  18. equity_price_historical
//  19. obb_call                            (generic equity historical)
//  20. list_all_routes
//  21. provider_list
//  22. read_settings_json
//  23. write_settings_json
//  24. routines_list
//  25. routines_save
//  26. routines_delete
//  27. list_backend_services               (expected error)
//  28. list_conda_environments             (expected error)
//  29. server_attach
//  30. server_health

import { listen } from "@tauri-apps/api/event";
import * as ipc from "./ipc";

// ---------------------------------------------------------------------------
// DOM helpers
// ---------------------------------------------------------------------------

const $result = document.getElementById("result") as HTMLPreElement;
const $navLog = document.getElementById("navigate-log") as HTMLPreElement;
const $procOut = document.getElementById("process-output") as HTMLPreElement;
const $installBar = document.getElementById(
  "install-progress",
) as HTMLProgressElement;
const $installText = document.getElementById(
  "install-progress-text",
) as HTMLSpanElement;
const $versionBadge = document.getElementById(
  "version-badge",
) as HTMLSpanElement;
const $installBadge = document.getElementById(
  "install-badge",
) as HTMLSpanElement;
const $baseUrlBadge = document.getElementById(
  "base-url-badge",
) as HTMLSpanElement;

function show(value: unknown, isError = false): void {
  const text =
    value instanceof Error
      ? `Error: ${value.message}`
      : typeof value === "string"
        ? value
        : JSON.stringify(value, null, 2);
  $result.textContent = text;
  $result.classList.toggle("err", isError);
}

function appendNavLog(line: string): void {
  const prev = $navLog.textContent === "(none yet)" ? "" : $navLog.textContent;
  $navLog.textContent = `${prev ?? ""}${line}\n`;
}

function appendProcOut(line: string): void {
  const prev =
    $procOut.textContent?.startsWith("(register") === true
      ? ""
      : $procOut.textContent;
  $procOut.textContent = `${prev ?? ""}${line}\n`;
}

async function withButtonState(
  btn: HTMLButtonElement,
  task: () => Promise<unknown>,
): Promise<void> {
  btn.classList.remove("ok", "err");
  btn.classList.add("busy");
  try {
    const result = await task();
    show(result);
    btn.classList.add("ok");
  } catch (err) {
    const msg = err instanceof Error ? err.message : JSON.stringify(err);
    show(`Error: ${msg}`, true);
    btn.classList.add("err");
  } finally {
    btn.classList.remove("busy");
  }
}

// ---------------------------------------------------------------------------
// Event listeners
// ---------------------------------------------------------------------------

interface NavigatePayload {
  path: string;
}
interface ProcessOutputPayload {
  processId: string;
  output: string;
  timestamp: number;
  type: string;
}
interface InstallProgressPayload {
  step: string;
  progress: number;
  message: string;
}

async function wireEvents(): Promise<void> {
  await listen<NavigatePayload>("navigate", (event) => {
    appendNavLog(`navigate → ${event.payload.path}`);
  });

  await listen<ProcessOutputPayload>("process-output", (event) => {
    const { processId, output, type } = event.payload;
    appendProcOut(`[${processId}] (${type}) ${output}`);
  });

  await listen<InstallProgressPayload>("install-progress", (event) => {
    const { step, progress, message } = event.payload;
    $installBar.value = Math.round(progress * 100);
    $installText.textContent = `${step}: ${message}`;
  });
}

// ---------------------------------------------------------------------------
// Refresh status badges (best-effort)
// ---------------------------------------------------------------------------

async function refreshBadges(): Promise<void> {
  try {
    $versionBadge.textContent = `version: ${await ipc.getAppVersion()}`;
  } catch {
    $versionBadge.textContent = "version: ?";
  }
  try {
    const snap = await ipc.getInstallationState();
    $installBadge.textContent = `install: ${
      snap.isInstalled ? "yes" : "no"
    }`;
  } catch {
    $installBadge.textContent = "install: ?";
  }
  try {
    $baseUrlBadge.textContent = `base-url: ${await ipc.obbGetBaseUrl()}`;
  } catch {
    $baseUrlBadge.textContent = "base-url: ?";
  }
}

// ---------------------------------------------------------------------------
// Per-button handlers
// ---------------------------------------------------------------------------

const SMOKE_PROCESS_ID = "smoke-test-process";

type Handler = () => Promise<unknown>;

const handlers: Record<string, Handler> = {
  get_installation_state: () => ipc.getInstallationState(),

  get_app_version: () => ipc.getAppVersion(),

  navigate_forward: async () => {
    await ipc.navigateToPage("/dashboard");
    return "navigated → /dashboard (see Navigate events pane)";
  },

  navigate_back: async () => {
    await ipc.navigateToPage("/");
    return "navigated → / (see Navigate events pane)";
  },

  register_process_monitoring: async () => {
    const registered = await ipc.registerProcessMonitoring(SMOKE_PROCESS_ID);
    return { registered, processId: SMOKE_PROCESS_ID };
  },

  get_process_logs_history: async () => {
    await ipc.registerProcessMonitoring(SMOKE_PROCESS_ID);
    const history = await ipc.getProcessLogsHistory(SMOKE_PROCESS_ID, 50);
    const cleared = await ipc.clearProcessLogsHistory(SMOKE_PROCESS_ID);
    return { historyCount: history.length, cleared };
  },

  get_home_directory: () => ipc.getHomeDirectory(),

  select_directory: () => ipc.selectDirectory("Pick any directory"),

  check_directory_exists: async () => {
    const home = await ipc.getHomeDirectory();
    return { path: home, exists: await ipc.checkDirectoryExists(home) };
  },

  get_user_credentials: () => ipc.getUserCredentials(),

  update_user_credentials: async () => {
    // Read current, then merge in a single throwaway flag.
    const current = (await ipc.getUserCredentials()) as Record<
      string,
      unknown
    > | null;
    const next = {
      ...(current ?? {}),
      smoke_test_at: new Date().toISOString(),
    };
    return ipc.updateUserCredentials(next as ipc.JsonValue);
  },

  open_credentials_file: () => ipc.openCredentialsFile("user_settings.json"),

  obb_set_base_url: () => ipc.obbSetBaseUrl("http://127.0.0.1:6900"),

  obb_get_base_url: () => ipc.obbGetBaseUrl(),

  obb_health: () => ipc.obbHealth(),

  obb_widgets: () => ipc.obbWidgets(),

  equity_price_historical: () =>
    ipc.equityPriceHistorical({
      symbol: "AAPL",
      provider: "yfinance",
    }),

  obb_call_equity: () =>
    ipc.obbCall({
      route: "/equity/price/historical",
      params: { symbol: "AAPL", provider: "yfinance" },
      method: "GET",
    }),

  list_all_routes: async () => {
    const routes = await ipc.listAllRoutes();
    return { count: routes.length, first: routes.slice(0, 5) };
  },

  provider_list: () => ipc.providerList(),

  settings_round_trip: async () => {
    const fileName = "smoke_test.json";
    const payload = {
      smoke: true,
      ts: Date.now(),
      nested: { hello: "world" },
    };
    const wrote = await ipc.writeSettingsJson(
      fileName,
      payload as ipc.JsonValue,
    );
    const readBack = await ipc.readSettingsJson(fileName);
    return { wrote, readBack };
  },

  routines_crud: async () => {
    const name = "smoke_test_routine";
    const saved = await ipc.routinesSave(
      name,
      "/* smoke test routine */\nset --provider yfinance\n",
    );
    const list = await ipc.routinesList();
    const deleted = await ipc.routinesDelete(name);
    return {
      saved,
      deleted,
      listedCount: list.length,
      includedBefore: list.some((r) => r.name.startsWith(name)),
    };
  },

  list_backend_services: () => ipc.listBackendServices(),

  list_conda_environments: () => ipc.listCondaEnvironments(),

  server_attach: () => ipc.serverAttach("http://127.0.0.1:6900"),

  server_health: () => ipc.serverHealth(),
};

// ---------------------------------------------------------------------------
// "Run all" — fires every handler sequentially, ignoring per-handler errors
// so a missing Python backend doesn't short-circuit the smoke run.
// ---------------------------------------------------------------------------

async function runAll(): Promise<unknown> {
  // Order matters: navigate twice → produces two `navigate` events; the
  // process registration must precede the history fetch.
  const sequence: Array<keyof typeof handlers> = [
    "get_installation_state",
    "get_app_version",
    "navigate_forward",
    "navigate_back",
    "register_process_monitoring",
    "get_process_logs_history",
    "get_home_directory",
    "check_directory_exists",
    "get_user_credentials",
    "update_user_credentials",
    "open_credentials_file",
    "obb_set_base_url",
    "obb_get_base_url",
    "obb_health",
    "obb_widgets",
    "equity_price_historical",
    "obb_call_equity",
    "list_all_routes",
    "provider_list",
    "settings_round_trip",
    "routines_crud",
    "list_backend_services",
    "list_conda_environments",
    "server_attach",
    "server_health",
  ];

  const results: Array<{ cmd: string; ok: boolean; value?: unknown; err?: string }> =
    [];
  for (const cmd of sequence) {
    try {
      const value = await handlers[cmd]!();
      results.push({ cmd, ok: true, value });
    } catch (err) {
      const msg = err instanceof Error ? err.message : JSON.stringify(err);
      results.push({ cmd, ok: false, err: msg });
    }
  }
  await refreshBadges();
  return {
    total: results.length,
    succeeded: results.filter((r) => r.ok).length,
    failed: results.filter((r) => !r.ok).length,
    results,
  };
}

// ---------------------------------------------------------------------------
// Bootstrap
// ---------------------------------------------------------------------------

function bootstrap(): void {
  for (const btn of document.querySelectorAll<HTMLButtonElement>(
    "button[data-cmd]",
  )) {
    btn.addEventListener("click", () => {
      const cmd = btn.dataset["cmd"] ?? "";
      if (cmd === "run_all") {
        void withButtonState(btn, runAll);
        return;
      }
      const handler = handlers[cmd];
      if (!handler) {
        show(`Unknown command: ${cmd}`, true);
        return;
      }
      void withButtonState(btn, handler);
    });
  }

  void wireEvents();
  void refreshBadges();
}

bootstrap();
