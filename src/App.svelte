<script lang="ts">
  import { onMount } from "svelte";
  import type { UnlistenFn } from "@tauri-apps/api/event";

  import Sidebar from "./lib/components/Sidebar.svelte";
  import PlanTable from "./lib/components/PlanTable.svelte";
  import DetailsPanel from "./lib/components/DetailsPanel.svelte";
  import SettingsModal from "./lib/components/SettingsModal.svelte";
  import Toast from "./lib/components/Toast.svelte";

  import * as api from "./lib/api";
  import {
    DEMO_CONFIG,
    DEMO_ROOT,
    demoChoices,
    demoPlan,
    demoSearch,
  } from "./lib/demo";
  import { basename, candidateKey, candidateLabel } from "./lib/format";
  import type {
    CandidateChoice,
    Config,
    FilterKey,
    MatchCandidate,
    PlanItem,
    PlanProgress,
    PlanResponse,
    PlanSummary,
    RenamePlan,
    ScanEvent,
  } from "./lib/types";

  const startInDemo = !api.isTauri;

  let demo = $state(startInDemo);
  let root = $state(startInDemo ? DEMO_ROOT : "");
  let plan = $state<RenamePlan | null>(startInDemo ? demoPlan() : null);
  let planId = $state<string | null>(startInDemo ? "demo-plan" : null);
  let config = $state<Config>({ ...DEMO_CONFIG });
  let filter = $state<FilterKey>("all");
  let query = $state("");
  let checked = $state<Set<string>>(
    new Set(startInDemo ? ["v1", "v2", "v5"] : []),
  );
  let activeId = $state<string | null>(startInDemo ? "v3" : null);
  let candidates = $state<CandidateChoice[]>([]);
  let chosenKey = $state<string | null>(null);
  let searchText = $state("");
  let searchBusy = $state(false);
  let settingsOpen = $state(false);
  let savingSettings = $state(false);
  let busy = $state<{
    label: string;
    done: number;
    total: number;
    detail: string;
  } | null>(null);
  let toast = $state<{
    message: string;
    kind: "info" | "success" | "error";
  } | null>(null);
  let lastRunNote = $state<string | null>(null);

  let toastTimer: ReturnType<typeof setTimeout> | null = null;
  let rebuildTimer: ReturnType<typeof setTimeout> | null = null;
  let candidateRequest = 0;
  // Identifies which plan item the details panel is currently populated for.
  // Kept outside reactive state so a plan update does not reset the user's
  // candidate choice unless the selection or the plan itself changed.
  let detailsKey: string | null = null;

  const videoItems = $derived(
    plan ? plan.items.filter((item) => !item.is_sidecar) : [],
  );

  const counts = $derived.by((): PlanSummary => {
    if (demo && plan) {
      return plan.summary;
    }
    const items = videoItems;
    return {
      total: items.length,
      ready: items.filter((item) => item.status === "Ready").length,
      review: items.filter((item) => item.status === "Review").length,
      unmatched: items.filter((item) => item.status === "Unmatched").length,
      conflicts: items.filter((item) => item.status === "Conflict").length,
      skipped: items.filter((item) => item.status === "Skipped").length,
      sidecars: plan ? plan.items.filter((item) => item.is_sidecar).length : 0,
    };
  });

  const visibleItems = $derived.by((): PlanItem[] => {
    const needle = query.trim().toLowerCase();
    return videoItems.filter((item) => {
      if (filter === "matched" && item.status !== "Ready") return false;
      if (filter === "review" && item.status !== "Review") return false;
      if (filter === "unmatched" && item.status !== "Unmatched") return false;
      if (filter === "conflicts" && item.status !== "Conflict") return false;
      if (!needle) return true;
      return (
        basename(item.source).toLowerCase().includes(needle) ||
        item.parsed.title.toLowerCase().includes(needle) ||
        item.target.toLowerCase().includes(needle)
      );
    });
  });

  const activeItem = $derived(
    videoItems.find((item) => item.id === activeId) ?? null,
  );

  const chosenCandidate = $derived(
    candidates.find(
      (choice) => candidateKey(choice.candidate) === chosenKey,
    ) ??
      candidates[0] ??
      null,
  );

  const selectedCount = $derived(checked.size);

  const actionableSelected = $derived(
    [...checked].filter((id) => {
      const item = videoItems.find((entry) => entry.id === id);
      return item && (item.status === "Ready" || item.status === "Review");
    }).length,
  );

  const providerLabel = $derived(
    config.provider === "omdb" ? "OMDb" : "TMDB",
  );

  const progressPercent = $derived(
    busy && busy.total > 0
      ? Math.min(100, Math.round((busy.done / busy.total) * 100))
      : 0,
  );

  const emptyMessage = $derived(
    !plan
      ? "Choose a folder to scan and preview renames."
      : "No files match this filter.",
  );

  const footerText = $derived(
    `${selectedCount} selected · ${counts.ready} ready · ${counts.review} to review · ${counts.conflicts} conflicts · ${
      lastRunNote ?? "dry run, nothing changed yet"
    }`,
  );

  $effect(() => {
    const item = activeItem;
    const isDemo = demo;
    if (!item) {
      detailsKey = null;
      candidates = [];
      chosenKey = null;
      searchText = "";
      return;
    }

    const key = `${planId ?? ""}:${item.id}`;
    if (key === detailsKey) {
      return;
    }
    detailsKey = key;

    searchText = item.parsed.title;
    chosenKey = null;

    if (isDemo) {
      const choices = demoChoices(item.id, item);
      candidates = choices;
      chosenKey =
        choices.length > 0 ? candidateKey(choices[0].candidate) : null;
      return;
    }

    const request = ++candidateRequest;
    candidates = item.candidate
      ? [{ candidate: item.candidate, score: item.score }]
      : [];
    chosenKey = item.candidate ? candidateKey(item.candidate) : null;
    searchBusy = true;

    void (async () => {
      try {
        const results = await api.searchMetadata(
          item.parsed.title,
          item.parsed.year,
          item.parsed.media_type,
        );
        if (request !== candidateRequest) return;
        candidates = results.map((candidate) => ({
          candidate,
          score:
            item.candidate &&
            candidateKey(candidate) === candidateKey(item.candidate)
              ? item.score
              : null,
        }));
        chosenKey = item.candidate
          ? candidateKey(item.candidate)
          : results.length > 0
            ? candidateKey(results[0])
            : null;
      } catch {
        // Keep the plan's candidate; a manual search reports errors to the user.
      } finally {
        if (request === candidateRequest) {
          searchBusy = false;
        }
      }
    })();
  });

  onMount(() => {
    if (!api.isTauri) {
      demo = true;
      root = DEMO_ROOT;
      plan = demoPlan();
      planId = "demo-plan";
      config = loadDemoConfig();
      checked = new Set(["v1", "v2", "v5"]);
      activeId = "v3";
      detailsKey = null;
      void rebuildPreview();
      return;
    }

    demo = false;
    const unlisteners: UnlistenFn[] = [];
    let cancelled = false;

    void (async () => {
      try {
        config = await api.getSettings();
      } catch {
        // Keep defaults if settings cannot be read.
      }
      const scan = await api.listenEvent<ScanEvent>(
        "scan-progress",
        (event) => {
          if (!busy) return;
          if (event.type === "progress") {
            busy = {
              ...busy,
              done: event.files ?? busy.done,
              detail: event.current ?? busy.detail,
            };
          } else if (event.type === "found") {
            busy = {
              ...busy,
              done: busy.done + 1,
              detail: event.path ?? busy.detail,
            };
          }
        },
      );
      const planning = await api.listenEvent<PlanProgress>(
        "plan-progress",
        (event) => {
          busy = {
            label:
              event.phase === "scan"
                ? "Scanning library"
                : "Matching metadata",
            done: event.done,
            total: event.total,
            detail: event.current,
          };
        },
      );
      const applying = await api.listenEvent<{
        done: number;
        total: number;
        current: string;
      }>("apply-progress", (event) => {
        busy = {
          label: "Applying renames",
          done: event.done,
          total: event.total,
          detail: event.current,
        };
      });

      if (cancelled) {
        scan();
        planning();
        applying();
        return;
      }
      unlisteners.push(scan, planning, applying);
    })();

    return () => {
      cancelled = true;
      for (const unlisten of unlisteners) {
        unlisten();
      }
    };
  });

  function loadDemoConfig(): Config {
    try {
      const raw = window.localStorage.getItem("mediaparse.config");
      if (raw) {
        return { ...DEMO_CONFIG, ...(JSON.parse(raw) as Partial<Config>) };
      }
    } catch {
      // Ignore malformed local storage in the demo.
    }
    return { ...DEMO_CONFIG };
  }

  function showToast(
    message: string,
    kind: "info" | "success" | "error" = "info",
  ) {
    toast = { message, kind };
    if (toastTimer) {
      clearTimeout(toastTimer);
    }
    toastTimer = setTimeout(() => {
      toast = null;
      toastTimer = null;
    }, 4500);
  }

  function dismissToast() {
    toast = null;
    if (toastTimer) {
      clearTimeout(toastTimer);
      toastTimer = null;
    }
  }

  function toggleChecked(id: string) {
    const next = new Set(checked);
    if (next.has(id)) {
      next.delete(id);
    } else {
      next.add(id);
    }
    checked = next;
  }

  function activate(id: string) {
    activeId = id;
  }

  function chooseCandidate(candidate: MatchCandidate) {
    chosenKey = candidateKey(candidate);
  }

  function applyResponse(response: PlanResponse, selectReady: boolean) {
    plan = response.plan;
    planId = response.plan_id;
    const videos = response.plan.items.filter((item) => !item.is_sidecar);
    if (selectReady) {
      checked = new Set(
        videos
          .filter((item) => item.status === "Ready")
          .map((item) => item.id),
      );
      const review = videos.find((item) => item.status === "Review");
      activeId = review?.id ?? videos[0]?.id ?? null;
    } else if (
      activeId &&
      !videos.some((item) => item.id === activeId)
    ) {
      activeId = videos[0]?.id ?? null;
    }
  }

  async function chooseRoot() {
    if (demo) {
      showToast(
        "Browser demo — choose a folder in the desktop app to scan a real library.",
        "info",
      );
      return;
    }
    try {
      const selected = await api.chooseFolder();
      if (!selected) {
        return;
      }
      root = selected;
      await refreshPlan(true);
    } catch (error) {
      showToast(errorMessage(error), "error");
    }
  }

  async function refreshPlan(notify: boolean) {
    if (!root) {
      showToast("Choose a movie folder first.", "error");
      return;
    }

    if (demo) {
      plan = demoPlan();
      planId = "demo-plan";
      checked = new Set(["v1", "v2", "v5"]);
      activeId = "v3";
      detailsKey = null;
      filter = "all";
      query = "";
      lastRunNote = null;
      if (notify) {
        showToast("Demo library reloaded.", "success");
      }
      return;
    }

    busy = {
      label: "Scanning library",
      done: 0,
      total: 0,
      detail: root,
    };
    try {
      const response = await api.buildPlan(root, {
        template: config.template,
        tv_template: config.tv_template,
        series_folder_template: config.series_folder_template,
        create_movie_folders: config.create_movie_folders,
        create_tv_folders: config.create_tv_folders,
        move_sidecars: config.move_sidecars,
        offline: config.offline,
        concurrency: config.concurrency,
        max_component_len: config.max_component_len,
      });
      applyResponse(response, true);
      lastRunNote = null;
      if (notify) {
        showToast(
          `Preview ready: ${response.plan.summary.ready} ready, ${response.plan.summary.review} to review.`,
          "success",
        );
      }
    } catch (error) {
      showToast(errorMessage(error), "error");
    } finally {
      busy = null;
    }
  }

  function scheduleRebuild() {
    if (rebuildTimer) {
      clearTimeout(rebuildTimer);
    }
    rebuildTimer = setTimeout(() => {
      rebuildTimer = null;
      void rebuildPreview();
    }, 300);
  }

  async function rebuildPreview() {
    if (!plan || !planId) {
      return;
    }

    if (demo) {
      plan = {
        ...plan,
        items: plan.items.map((item) => ({
          ...item,
          target: formatDemoTarget(item, config),
        })),
      };
      return;
    }

    try {
      const response = await api.rebuildPlan(planId, {
        template: config.template,
        tv_template: config.tv_template,
        series_folder_template: config.series_folder_template,
        create_movie_folders: config.create_movie_folders,
        create_tv_folders: config.create_tv_folders,
        move_sidecars: config.move_sidecars,
        max_component_len: config.max_component_len,
      });
      plan = response.plan;
      planId = response.plan_id;
    } catch (error) {
      showToast(errorMessage(error), "error");
    }
  }

  function setTemplate(value: string) {
    config = { ...config, template: value };
    scheduleRebuild();
  }

  function setTvTemplate(value: string) {
    config = { ...config, tv_template: value };
    scheduleRebuild();
  }

  function setSeriesFolderTemplate(value: string) {
    config = { ...config, series_folder_template: value };
    scheduleRebuild();
  }

  function toggleCreateFolders(value: boolean) {
    config = { ...config, create_movie_folders: value };
    scheduleRebuild();
  }

  function toggleCreateTvFolders(value: boolean) {
    config = { ...config, create_tv_folders: value };
    scheduleRebuild();
  }

  function toggleMoveSidecars(value: boolean) {
    config = { ...config, move_sidecars: value };
    scheduleRebuild();
  }

  function toggleOffline(value: boolean) {
    config = { ...config, offline: value };
  }

  async function useMatch() {
    const item = activeItem;
    const choice = chosenCandidate;
    if (!item || !choice) {
      return;
    }

    if (demo) {
      plan = plan
        ? {
            ...plan,
            items: plan.items.map((entry) =>
              entry.id === item.id
                ? {
                    ...entry,
                    candidate: choice.candidate,
                    score: 1,
                    status: "Ready" as const,
                    reason: null,
                    target: formatDemoTarget(
                      {
                        ...entry,
                        candidate: choice.candidate,
                        status: "Ready",
                      },
                      config,
                    ),
                  }
                : entry,
            ),
          }
        : plan;
      showToast(`Matched ${candidateLabel(choice.candidate)}.`, "success");
      return;
    }

    if (!planId) {
      return;
    }
    try {
      const response = await api.setMatch(planId, item.id, choice.candidate);
      applyResponse(response, false);
      showToast(`Matched ${candidateLabel(choice.candidate)}.`, "success");
    } catch (error) {
      showToast(errorMessage(error), "error");
    }
  }

  async function skipActive() {
    const item = activeItem;
    if (!item) {
      return;
    }

    if (demo) {
      plan = plan
        ? {
            ...plan,
            items: plan.items.map((entry) =>
              entry.id === item.id
                ? {
                    ...entry,
                    status: "Skipped" as const,
                    reason: "skipped by user",
                  }
                : entry,
            ),
          }
        : plan;
      const next = new Set(checked);
      next.delete(item.id);
      checked = next;
      showToast("File skipped.", "info");
      return;
    }

    if (!planId) {
      return;
    }
    try {
      const response = await api.skipItem(planId, item.id);
      applyResponse(response, false);
      const next = new Set(checked);
      next.delete(item.id);
      checked = next;
      showToast("File skipped.", "info");
    } catch (error) {
      showToast(errorMessage(error), "error");
    }
  }

  async function runManualSearch() {
    const item = activeItem;
    if (!item) {
      return;
    }
    searchBusy = true;
    try {
      const results = demo
        ? demoSearch(searchText)
        : await api.searchMetadata(
            searchText,
            item.parsed.year,
            item.parsed.media_type,
          );
      if (results.length === 0) {
        showToast(`No ${providerLabel} candidates found.`, "error");
        return;
      }
      candidates = results.map((candidate) => ({
        candidate,
        score: null,
      }));
      chosenKey = candidateKey(results[0]);
      showToast(
        `Found ${results.length} candidate${results.length === 1 ? "" : "s"}.`,
        "info",
      );
    } catch (error) {
      showToast(errorMessage(error), "error");
    } finally {
      searchBusy = false;
    }
  }

  async function applySelected() {
    if (!plan) {
      return;
    }
    const ids = [...checked].filter((id) => {
      const item = videoItems.find((entry) => entry.id === id);
      return item && (item.status === "Ready" || item.status === "Review");
    });
    if (ids.length === 0) {
      showToast("Select at least one ready or review item.", "error");
      return;
    }

    if (demo) {
      showToast(
        `Dry run: ${ids.length} file${ids.length === 1 ? "" : "s"} would be renamed. Run the desktop app to apply changes.`,
        "info",
      );
      return;
    }

    if (!planId) {
      return;
    }

    busy = {
      label: "Applying renames",
      done: 0,
      total: ids.length,
      detail: "",
    };
    try {
      const report = await api.applyPlan(planId, ids);
      const parts = [`Renamed ${report.renamed}`];
      if (report.failed > 0) {
        parts.push(`${report.failed} failed`);
      }
      if (report.skipped > 0) {
        parts.push(`${report.skipped} skipped`);
      }
      lastRunNote = `${parts.join(" · ")} in the last run`;
      showToast(lastRunNote, report.failed > 0 ? "error" : "success");
      if (report.errors.length > 0) {
        console.warn("rename errors", report.errors);
      }
      await refreshPlan(false);
    } catch (error) {
      showToast(errorMessage(error), "error");
    } finally {
      busy = null;
    }
  }

  async function undoRun() {
    if (demo) {
      showToast(
        "Dry run: undo restores the last run in the desktop app.",
        "info",
      );
      return;
    }

    busy = {
      label: "Undoing last run",
      done: 0,
      total: 0,
      detail: "",
    };
    try {
      const report = await api.undoLast();
      lastRunNote = `Restored ${report.restored} file${
        report.restored === 1 ? "" : "s"
      } from the last run`;
      showToast(lastRunNote, report.failed > 0 ? "error" : "success");
      await refreshPlan(false);
    } catch (error) {
      showToast(errorMessage(error), "error");
    } finally {
      busy = null;
    }
  }

  async function saveSettings(next: Config) {
    savingSettings = true;
    try {
      if (demo) {
        config = next;
        window.localStorage.setItem(
          "mediaparse.config",
          JSON.stringify(next),
        );
        settingsOpen = false;
        await rebuildPreview();
        showToast("Settings saved locally (browser demo).", "success");
      } else {
        config = await api.saveSettings(next);
        settingsOpen = false;
        showToast("Settings saved.", "success");
        await rebuildPreview();
      }
    } catch (error) {
      showToast(errorMessage(error), "error");
    } finally {
      savingSettings = false;
    }
  }

  function formatDemoTarget(item: PlanItem, cfg: Config): string {
    if (item.status === "Unmatched") {
      return item.target;
    }
    const match = item.candidate;
    const title = match?.title || item.parsed.title || "Untitled";
    const year = match?.year ?? item.parsed.year;
    const season = item.parsed.season ?? 1;
    const seasonValue = String(season).padStart(2, "0");
    const episodes = item.parsed.episodes;
    const episodeValue =
      episodes.length === 0
        ? ""
        : episodes.length === 1
          ? String(episodes[0]).padStart(2, "0")
          : `${String(episodes[0]).padStart(2, "0")}-E${String(
              episodes[episodes.length - 1],
            ).padStart(2, "0")}`;
    const isTv = item.parsed.media_type === "tv" && episodes.length > 0;
    const values: Record<string, string> = {
      title,
      year: year ? String(year) : "",
      id: match?.id ?? "",
      provider: match?.provider ?? "",
      quality: item.parsed.quality ?? "",
      source: item.parsed.source ?? "",
      codec: item.parsed.codec ?? "",
      edition: item.parsed.edition ?? "",
      part: item.parsed.part ? `part${item.parsed.part.index}` : "",
      season: seasonValue,
      episode: episodeValue,
      episode_title: item.parsed.episode_title ?? "",
    };

    const template = isTv ? cfg.tv_template : cfg.template;
    let base = renderTemplate(template, values);

    if (!template.includes("{edition}") && item.parsed.edition) {
      base = sanitizeComponent(
        `${base} - ${sanitizeComponent(item.parsed.edition)}`,
      );
    }
    if (!template.includes("{part}") && item.parsed.part) {
      base = sanitizeComponent(`${base} - part${item.parsed.part.index}`);
    }

    const extension =
      basename(item.source).split(".").pop()?.toLowerCase() || "mkv";
    const fileName = `${base}.${extension}`;

    if (isTv) {
      if (!cfg.create_tv_folders) {
        return `${DEMO_ROOT}/${fileName}`;
      }
      const series = sanitizeComponent(
        renderTemplate(cfg.series_folder_template, values),
      );
      return `${DEMO_ROOT}/${series}/Season ${seasonValue}/${fileName}`;
    }
    return cfg.create_movie_folders
      ? `${DEMO_ROOT}/${base}/${fileName}`
      : `${DEMO_ROOT}/${fileName}`;
  }

  function renderTemplate(
    template: string,
    values: Record<string, string>,
  ): string {
    const rendered = template.replace(/\{(\w+)\}/g, (_match, key: string) => {
      return values[key] ?? "";
    });
    return sanitizeComponent(
      rendered
        .replace(/\[\s*tmdbid\s*-\s*\]/gi, " ")
        .replace(/\(\s*\)/g, " ")
        .replace(/\[\s*\]/g, " ")
        .replace(/\s*-\s*$/, " "),
    );
  }

  function sanitizeComponent(input: string): string {
    const cleaned = input
      .replace(/[<>:"/\\|?*\u0000-\u001f]/g, " ")
      .replace(/\s+/g, " ")
      .trim()
      .replace(/[. ]+$/g, "");
    return cleaned || "Untitled";
  }

  function errorMessage(error: unknown): string {
    if (error instanceof Error) {
      return error.message;
    }
    if (typeof error === "string") {
      return error;
    }
    return JSON.stringify(error);
  }
</script>

<div class="app">
  <header class="topbar">
    <div class="brand">
      <strong>mediaParse</strong>
      <span class="root">
        {root || "No folder selected"} · {counts.total} files
      </span>
    </div>
    <div class="top-actions">
      <button type="button" class="btn" onclick={chooseRoot}>Choose folder</button>
      <button
        type="button"
        class="btn"
        onclick={() => refreshPlan(true)}
        disabled={Boolean(busy)}
      >
        Rescan
      </button>
      <button type="button" class="btn" onclick={() => (settingsOpen = true)}>
        Settings
      </button>
    </div>
  </header>

  {#if busy}
    <div class="progress" aria-hidden="true">
      <div
        class="bar"
        class:indeterminate={busy.total === 0}
        style:width={busy.total > 0 ? `${progressPercent}%` : undefined}
      ></div>
    </div>
  {/if}

  <div class="body">
    <Sidebar
      counts={counts}
      {filter}
      template={config.template}
      tvTemplate={config.tv_template}
      seriesFolderTemplate={config.series_folder_template}
      createFolders={config.create_movie_folders}
      createTvFolders={config.create_tv_folders}
      moveSidecars={config.move_sidecars}
      offline={config.offline}
      onFilter={(key) => (filter = key)}
      onTemplate={setTemplate}
      onTvTemplate={setTvTemplate}
      onSeriesFolderTemplate={setSeriesFolderTemplate}
      onToggleCreate={toggleCreateFolders}
      onToggleCreateTv={toggleCreateTvFolders}
      onToggleSidecars={toggleMoveSidecars}
      onToggleOffline={toggleOffline}
    />

    <main class="content">
      <div class="toolbar">
        <label for="search">Search</label>
        <input
          id="search"
          class="field search"
          placeholder="Filter by name"
          bind:value={query}
        />
        {#if busy}
          <span class="busy-text" title={busy.detail}>
            {busy.label}… {busy.done}{busy.total > 0 ? `/${busy.total}` : ""}
          </span>
        {/if}
      </div>

      <PlanTable
        items={visibleItems}
        {root}
        {checked}
        {activeId}
        {emptyMessage}
        onToggle={toggleChecked}
        onActivate={activate}
      />
    </main>

    <DetailsPanel
      item={activeItem}
      {candidates}
      {chosenKey}
      {searchText}
      {searchBusy}
      {providerLabel}
      onChoose={chooseCandidate}
      onUse={useMatch}
      onSkip={skipActive}
      onSearchText={(value) => (searchText = value)}
      onSearch={runManualSearch}
    />
  </div>

  <footer class="footer">
    <span class="summary">{footerText}</span>
    <div class="footer-actions">
      <button type="button" class="btn" onclick={undoRun} disabled={Boolean(busy)}>
        Undo last run
      </button>
      <button
        type="button"
        class="btn pri"
        onclick={applySelected}
        disabled={Boolean(busy) || actionableSelected === 0}
      >
        Apply {actionableSelected} rename{actionableSelected === 1 ? "" : "s"}
      </button>
    </div>
  </footer>
</div>

<SettingsModal
  open={settingsOpen}
  {config}
  saving={savingSettings}
  {demo}
  onSave={saveSettings}
  onClose={() => (settingsOpen = false)}
/>

<Toast {toast} onClose={dismissToast} />

<style>
  .app {
    min-height: 100vh;
    height: 100vh;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    color: var(--text);
    overflow: hidden;
  }

  .topbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 20px;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
    gap: 12px;
    flex: none;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 16px;
    min-width: 0;
  }

  .brand strong {
    font-size: 15px;
    font-weight: 600;
    white-space: nowrap;
  }

  .root {
    font-size: 13px;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .top-actions {
    display: flex;
    gap: 8px;
    flex: none;
  }

  .progress {
    height: 3px;
    background: #eef1fd;
    flex: none;
    overflow: hidden;
  }

  .bar {
    height: 100%;
    background: var(--accent);
    transition: width 120ms linear;
  }

  .bar.indeterminate {
    width: 35%;
    animation: slide 1.1s ease-in-out infinite;
  }

  @keyframes slide {
    0% {
      transform: translateX(-100%);
    }
    100% {
      transform: translateX(320%);
    }
  }

  .body {
    display: flex;
    flex: 1;
    min-height: 0;
  }

  .content {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .toolbar {
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
    display: flex;
    gap: 12px;
    align-items: center;
    flex: none;
  }

  .toolbar label {
    font-size: 12px;
    color: var(--muted);
    white-space: nowrap;
  }

  .search {
    flex: 1;
    border-color: var(--primary);
  }

  .busy-text {
    font-size: 12px;
    color: var(--muted);
    white-space: nowrap;
    max-width: 240px;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 20px;
    border-top: 1px solid var(--border);
    background: var(--surface);
    flex-wrap: wrap;
    gap: 8px;
    flex: none;
  }

  .summary {
    font-size: 13px;
    color: var(--muted);
  }

  .footer-actions {
    display: flex;
    gap: 8px;
  }

  @media (max-width: 900px) {
    .root {
      display: none;
    }
  }
</style>
