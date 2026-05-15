// Typed wrappers around `invoke<T>(...)` for the representative subset of
// tauri-shell commands exercised by this example. The full catalog (162
// commands) lives in `tauri-shell/SPEC.md` §4 — this file only ports the
// ones the example actually calls.
//
// In a real app you'd auto-generate these from `ts-rs` (Slice A). Here we
// hand-write them so the example has zero external dependencies on the
// bindings crate.

import { invoke } from "@tauri-apps/api/core";

// ---------------------------------------------------------------------------
// Shared types (mirror the Rust structs).
// ---------------------------------------------------------------------------

export interface InstallationSnapshot {
  isInstalled: boolean;
  installationDirectory: string | null;
}

export interface LogEntry {
  timestamp: number;
  content: string;
  process_id: string;
}

export interface RouteInfo {
  path: string;
  method: string;
  model?: string;
  tags?: string[];
  summary?: string;
  providers?: string[];
}

export interface RoutineMetadata {
  name: string;
  modifiedUnix: number;
  sizeBytes: number;
}

// `serde_json::Value` -> arbitrary JSON.
export type JsonValue =
  | null
  | boolean
  | number
  | string
  | JsonValue[]
  | { [k: string]: JsonValue };

// ---------------------------------------------------------------------------
// app
// ---------------------------------------------------------------------------

export function getInstallationState(): Promise<InstallationSnapshot> {
  return invoke<InstallationSnapshot>("get_installation_state");
}

export function getAppVersion(): Promise<string> {
  return invoke<string>("get_app_version");
}

export function navigateToPage(path: string): Promise<void> {
  return invoke<void>("navigate_to_page", { path });
}

// ---------------------------------------------------------------------------
// infrastructure
// ---------------------------------------------------------------------------

export function registerProcessMonitoring(processId: string): Promise<boolean> {
  return invoke<boolean>("register_process_monitoring", { processId });
}

export function getProcessLogsHistory(
  processId: string,
  count?: number,
): Promise<LogEntry[]> {
  return invoke<LogEntry[]>("get_process_logs_history", {
    processId,
    count: count ?? null,
  });
}

export function clearProcessLogsHistory(processId: string): Promise<boolean> {
  return invoke<boolean>("clear_process_logs_history", { processId });
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

export function getHomeDirectory(): Promise<string> {
  return invoke<string>("get_home_directory");
}

export function selectDirectory(prompt?: string): Promise<string> {
  return invoke<string>("select_directory", { prompt: prompt ?? null });
}

export function checkDirectoryExists(path: string): Promise<boolean> {
  return invoke<boolean>("check_directory_exists", { path });
}

// ---------------------------------------------------------------------------
// credentials
// ---------------------------------------------------------------------------

export function getUserCredentials(): Promise<JsonValue> {
  return invoke<JsonValue>("get_user_credentials");
}

export function updateUserCredentials(credentials: JsonValue): Promise<boolean> {
  // Note the inner `args` wrapper — the Rust handler takes a single struct
  // parameter named `args`, so the JS payload mirrors that shape.
  return invoke<boolean>("update_user_credentials", {
    args: { credentials },
  });
}

export function openCredentialsFile(fileName: string): Promise<boolean> {
  return invoke<boolean>("open_credentials_file", { fileName });
}

// ---------------------------------------------------------------------------
// obb (generic REST proxy)
// ---------------------------------------------------------------------------

export function obbSetBaseUrl(url: string): Promise<void> {
  return invoke<void>("obb_set_base_url", { url });
}

export function obbGetBaseUrl(): Promise<string> {
  return invoke<string>("obb_get_base_url");
}

export function obbHealth(): Promise<JsonValue> {
  return invoke<JsonValue>("obb_health");
}

export function obbWidgets(): Promise<JsonValue> {
  return invoke<JsonValue>("obb_widgets");
}

export interface ObbCallArgs {
  route: string;
  params?: Record<string, JsonValue>;
  method?: "GET" | "POST";
}

export function obbCall(args: ObbCallArgs): Promise<JsonValue> {
  return invoke<JsonValue>("obb_call", { args });
}

// ---------------------------------------------------------------------------
// obb_routes (typed wrappers)
// ---------------------------------------------------------------------------

export function equityPriceHistorical(
  params: Record<string, JsonValue>,
): Promise<JsonValue> {
  return invoke<JsonValue>("equity_price_historical", { params });
}

// ---------------------------------------------------------------------------
// openbb_meta + provider
// ---------------------------------------------------------------------------

export function listAllRoutes(): Promise<RouteInfo[]> {
  return invoke<RouteInfo[]>("list_all_routes");
}

export function providerList(): Promise<JsonValue> {
  return invoke<JsonValue>("provider_list");
}

// ---------------------------------------------------------------------------
// settings_files
// ---------------------------------------------------------------------------

export function readSettingsJson(fileName: string): Promise<JsonValue | null> {
  return invoke<JsonValue | null>("read_settings_json", {
    args: { fileName },
  });
}

export function writeSettingsJson(
  fileName: string,
  content: JsonValue,
): Promise<boolean> {
  return invoke<boolean>("write_settings_json", {
    args: { fileName, content },
  });
}

// ---------------------------------------------------------------------------
// routines
// ---------------------------------------------------------------------------

export function routinesList(): Promise<RoutineMetadata[]> {
  return invoke<RoutineMetadata[]>("routines_list");
}

export function routinesSave(name: string, content: string): Promise<boolean> {
  return invoke<boolean>("routines_save", { args: { name, content } });
}

export function routinesDelete(name: string): Promise<boolean> {
  return invoke<boolean>("routines_delete", { args: { name } });
}

// ---------------------------------------------------------------------------
// stubs (return `Err(NotImplemented)` until a connector is wired up)
// ---------------------------------------------------------------------------

export function listBackendServices(): Promise<JsonValue> {
  return invoke<JsonValue>("list_backend_services");
}

export function listCondaEnvironments(directory?: string): Promise<JsonValue> {
  return invoke<JsonValue>("list_conda_environments", {
    directory: directory ?? null,
  });
}

// ---------------------------------------------------------------------------
// server
// ---------------------------------------------------------------------------

export function serverAttach(url: string): Promise<void> {
  return invoke<void>("server_attach", { url });
}

export function serverHealth(): Promise<JsonValue> {
  return invoke<JsonValue>("server_health");
}
