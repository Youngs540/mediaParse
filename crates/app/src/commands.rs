use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use mediaparse_core::{MatchCandidate, MediaType, RenamePlan};
use mediaparse_infra::executor::{ApplyReport, UndoReport};
use mediaparse_infra::journal::RunSummary;
use mediaparse_infra::provider_from_config;

use crate::application::{self, BuildOptions, ScanSummary};
use crate::state::AppState;

type CmdResult<T> = Result<T, String>;

/// A preview plan plus the session id used to guard later edits and apply.
#[derive(Debug, Clone, Serialize)]
pub struct PlanResponse {
    pub plan_id: String,
    pub plan: RenamePlan,
}

fn message(error: impl std::fmt::Display) -> String {
    error.to_string()
}

/// Build the Tauri IPC handler.
///
/// `generate_handler!` refers to hidden macros created next to each
/// `#[tauri::command]`, so it must be expanded in this module rather than in
/// `desktop.rs`.
pub fn handler() -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        scan_library,
        build_plan,
        rebuild_plan,
        apply_plan,
        undo_last,
        list_runs,
        get_settings,
        save_settings,
        search_metadata,
        set_match,
        skip_item,
    ]
}

#[tauri::command]
pub async fn scan_library(
    app: AppHandle,
    state: State<'_, AppState>,
    root: String,
) -> CmdResult<ScanSummary> {
    let config = state.config.read().await.clone();
    let root_path = PathBuf::from(&root);
    let app_handle = app.clone();
    let output = tokio::task::spawn_blocking(move || {
        application::scan_library(&root_path, &config, move |event| {
            let _ = app_handle.emit("scan-progress", &event);
        })
    })
    .await
    .map_err(message)?
    .map_err(message)?;
    Ok(application::summary_from_output(Path::new(&root), &output))
}

#[tauri::command]
pub async fn build_plan(
    app: AppHandle,
    state: State<'_, AppState>,
    root: String,
    options: BuildOptions,
) -> CmdResult<PlanResponse> {
    let config = state.config.read().await.clone();
    let mut effective = config.clone();
    if let Some(offline) = options.offline {
        effective.offline = offline;
    }
    let provider = match provider_from_config(&effective, Arc::clone(&state.cache)) {
        Ok(provider) => provider,
        Err(_) => state.provider().await,
    };

    let app_handle = app.clone();
    let session = application::build_plan(
        Path::new(&root),
        &effective,
        provider,
        options,
        move |progress| {
            let _ = app_handle.emit("plan-progress", &progress);
        },
    )
    .await
    .map_err(message)?;

    let response = PlanResponse {
        plan_id: session.id.clone(),
        plan: session.plan.clone(),
    };
    *state.session.lock().await = Some(session);
    Ok(response)
}

#[tauri::command]
pub async fn rebuild_plan(
    state: State<'_, AppState>,
    plan_id: String,
    options: BuildOptions,
) -> CmdResult<PlanResponse> {
    let mut guard = state.session.lock().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| "no preview plan".to_string())?;
    if session.id != plan_id {
        return Err("the preview changed; rebuild it before editing".to_string());
    }
    if let Some(template) = options.template {
        if !template.trim().is_empty() {
            session.options.template = template;
        }
    }
    if let Some(template) = options.tv_template {
        if !template.trim().is_empty() {
            session.options.tv_template = template;
        }
    }
    if let Some(template) = options.series_folder_template {
        if !template.trim().is_empty() {
            session.options.series_folder_template = template;
        }
    }
    if let Some(create) = options.create_movie_folders {
        session.options.create_movie_folders = create;
    }
    if let Some(create) = options.create_tv_folders {
        session.options.create_tv_folders = create;
    }
    if let Some(move_sidecars) = options.move_sidecars {
        session.move_sidecars = move_sidecars;
    }
    if let Some(max_len) = options.max_component_len {
        session.options.max_component_len = max_len.clamp(32, 240);
    }
    let plan = application::rebuild_plan(session);
    Ok(PlanResponse {
        plan_id: session.id.clone(),
        plan,
    })
}

#[tauri::command]
pub async fn apply_plan(
    app: AppHandle,
    state: State<'_, AppState>,
    plan_id: String,
    item_ids: Vec<String>,
) -> CmdResult<ApplyReport> {
    let session = state
        .session
        .lock()
        .await
        .clone()
        .ok_or_else(|| "no preview plan; build a plan first".to_string())?;
    if session.id != plan_id {
        return Err("the preview changed; rebuild it before applying".to_string());
    }
    let executor = Arc::clone(&state.executor);
    let app_handle = app.clone();
    tokio::task::spawn_blocking(move || {
        application::apply_plan(&executor, &session, &item_ids, move |progress| {
            let _ = app_handle.emit("apply-progress", &progress);
        })
    })
    .await
    .map_err(message)?
    .map_err(message)
}

#[tauri::command]
pub async fn undo_last(state: State<'_, AppState>) -> CmdResult<UndoReport> {
    let executor = Arc::clone(&state.executor);
    tokio::task::spawn_blocking(move || application::undo_last(&executor))
        .await
        .map_err(message)?
        .map_err(message)
}

#[tauri::command]
pub async fn list_runs(state: State<'_, AppState>) -> CmdResult<Vec<RunSummary>> {
    state.journal.list_runs(20).map_err(message)
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> CmdResult<mediaparse_infra::Config> {
    Ok(state.config.read().await.clone())
}

#[tauri::command]
pub async fn save_settings(
    state: State<'_, AppState>,
    config: mediaparse_infra::Config,
) -> CmdResult<mediaparse_infra::Config> {
    state.update_config(config).await.map_err(message)
}

#[tauri::command]
pub async fn search_metadata(
    state: State<'_, AppState>,
    query: String,
    year: Option<u16>,
    media_type: Option<MediaType>,
) -> CmdResult<Vec<MatchCandidate>> {
    let provider = state.provider().await;
    application::search_metadata(provider, &query, year, media_type.unwrap_or_default())
        .await
        .map_err(message)
}

#[tauri::command]
pub async fn set_match(
    state: State<'_, AppState>,
    plan_id: String,
    item_id: String,
    candidate: Option<MatchCandidate>,
) -> CmdResult<PlanResponse> {
    let mut guard = state.session.lock().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| "no preview plan".to_string())?;
    if session.id != plan_id {
        return Err("the preview changed; rebuild it before editing".to_string());
    }
    let plan = application::set_match(session, &item_id, candidate);
    Ok(PlanResponse {
        plan_id: session.id.clone(),
        plan,
    })
}

#[tauri::command]
pub async fn skip_item(
    state: State<'_, AppState>,
    plan_id: String,
    item_id: String,
) -> CmdResult<PlanResponse> {
    let mut guard = state.session.lock().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| "no preview plan".to_string())?;
    if session.id != plan_id {
        return Err("the preview changed; rebuild it before editing".to_string());
    }
    let plan = application::skip_item(session, &item_id);
    Ok(PlanResponse {
        plan_id: session.id.clone(),
        plan,
    })
}
