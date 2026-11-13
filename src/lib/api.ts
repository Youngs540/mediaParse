import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

import type {
  ApplyReport,
  BuildOptions,
  Config,
  MatchCandidate,
  MediaType,
  PlanResponse,
  RunSummary,
  ScanSummary,
  UndoReport,
} from "./types";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

export const isTauri =
  typeof window !== "undefined" && Boolean(window.__TAURI_INTERNALS__);

export async function chooseFolder(): Promise<string | null> {
  if (!isTauri) {
    return null;
  }
  const selected = await open({
    directory: true,
    multiple: false,
    title: "Choose your movie library",
  });
  if (typeof selected === "string") {
    return selected;
  }
  return null;
}

export function scanLibrary(root: string): Promise<ScanSummary> {
  return invoke<ScanSummary>("scan_library", { root });
}

export function buildPlan(
  root: string,
  options: BuildOptions,
): Promise<PlanResponse> {
  return invoke<PlanResponse>("build_plan", { root, options });
}

export function rebuildPlan(
  planId: string,
  options: BuildOptions,
): Promise<PlanResponse> {
  return invoke<PlanResponse>("rebuild_plan", { planId, options });
}

export function applyPlan(
  planId: string,
  itemIds: string[],
): Promise<ApplyReport> {
  return invoke<ApplyReport>("apply_plan", { planId, itemIds });
}

export function undoLast(): Promise<UndoReport> {
  return invoke<UndoReport>("undo_last");
}

export function listRuns(): Promise<RunSummary[]> {
  return invoke<RunSummary[]>("list_runs");
}

export function getSettings(): Promise<Config> {
  return invoke<Config>("get_settings");
}

export function saveSettings(config: Config): Promise<Config> {
  return invoke<Config>("save_settings", { config });
}

export function searchMetadata(
  query: string,
  year: number | null,
  mediaType: MediaType = "movie",
): Promise<MatchCandidate[]> {
  return invoke<MatchCandidate[]>("search_metadata", {
    query,
    year,
    mediaType,
  });
}

export function setMatch(
  planId: string,
  itemId: string,
  candidate: MatchCandidate | null,
): Promise<PlanResponse> {
  return invoke<PlanResponse>("set_match", { planId, itemId, candidate });
}

export function skipItem(
  planId: string,
  itemId: string,
): Promise<PlanResponse> {
  return invoke<PlanResponse>("skip_item", { planId, itemId });
}

export async function listenEvent<T>(
  event: string,
  handler: (payload: T) => void,
): Promise<UnlistenFn> {
  if (!isTauri) {
    return () => {};
  }
  return listen<T>(event, (message) => handler(message.payload));
}
