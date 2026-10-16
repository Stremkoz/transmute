//! One Tauri command per `transmute_api::Api` method, and nothing else.
//!
//! Every command is `async` so it runs on the async runtime rather than the
//! main (UI) thread. Arguments arrive in camelCase (`relativePath`), as
//! Tauri renames them; the reference is `docs/api.md`. When a method is
//! added to `Api`, add its command here, register it in `main.rs` and add
//! it to `src/api/tauriApi.ts` (a frontend test compares the lists).

use tauri::{AppHandle, State};
use transmute_api::{
    Api, ApiError, ApiResult, AppSettings, EntryId, ErrorCode, InputsSummary, JobFilter, JobPage,
    Overview, PlanSummary, Preset, Preview, PreviewTarget, RootId, Sample, StoredPreset, ToolInfo,
    TreeNode, UserDefaults,
};

/// Lets the webview load one file the API chose to show. The asset
/// protocol's scope starts empty, so only previews are ever reachable.
fn allow_preview(app: &AppHandle, preview: &Preview) -> ApiResult<()> {
    use tauri::Manager;
    app.asset_protocol_scope()
        .allow_file(&preview.path)
        .map_err(|err| ApiError::new(ErrorCode::Internal, err.to_string()))
}

#[tauri::command]
pub async fn overview(api: State<'_, Api>) -> ApiResult<Overview> {
    Ok(api.overview())
}

#[tauri::command]
pub async fn dismiss_notices(api: State<'_, Api>) -> ApiResult<()> {
    api.dismiss_notices();
    Ok(())
}

#[tauri::command]
pub async fn add_inputs(
    api: State<'_, Api>,
    paths: Vec<String>,
    recursive: bool,
) -> ApiResult<InputsSummary> {
    api.add_inputs(paths, recursive).await
}

#[tauri::command]
pub async fn remove_root(api: State<'_, Api>, root: RootId) -> ApiResult<InputsSummary> {
    api.remove_root(root).await
}

#[tauri::command]
pub async fn clear_inputs(api: State<'_, Api>) -> ApiResult<InputsSummary> {
    api.clear_inputs()
}

#[tauri::command]
pub async fn tree(api: State<'_, Api>, root: RootId, dir: String) -> ApiResult<Vec<TreeNode>> {
    api.tree(root, &dir)
}

#[tauri::command]
pub async fn set_excluded(
    api: State<'_, Api>,
    root: RootId,
    relative_path: String,
    excluded: bool,
) -> ApiResult<InputsSummary> {
    api.set_excluded(root, &relative_path, excluded)
}

#[tauri::command]
pub async fn preset(api: State<'_, Api>) -> ApiResult<Preset> {
    Ok(api.preset())
}

#[tauri::command]
pub async fn set_preset(api: State<'_, Api>, preset: Preset) -> ApiResult<()> {
    api.set_preset(preset)
}

#[tauri::command]
pub async fn stored_presets(api: State<'_, Api>) -> ApiResult<Vec<StoredPreset>> {
    api.stored_presets()
}

#[tauri::command]
pub async fn load_preset(api: State<'_, Api>, id: String) -> ApiResult<Preset> {
    api.load_preset(&id)
}

#[tauri::command]
pub async fn save_preset(
    api: State<'_, Api>,
    id: Option<String>,
    preset: Preset,
) -> ApiResult<String> {
    api.save_preset(id, preset)
}

#[tauri::command]
pub async fn delete_preset(api: State<'_, Api>, id: String) -> ApiResult<()> {
    api.delete_preset(&id)
}

#[tauri::command]
pub async fn defaults(api: State<'_, Api>) -> ApiResult<UserDefaults> {
    Ok(api.defaults())
}

#[tauri::command]
pub async fn set_defaults(api: State<'_, Api>, defaults: UserDefaults) -> ApiResult<()> {
    api.set_defaults(defaults)
}

#[tauri::command]
pub async fn settings(api: State<'_, Api>) -> ApiResult<AppSettings> {
    Ok(api.settings())
}

#[tauri::command]
pub async fn set_settings(api: State<'_, Api>, settings: AppSettings) -> ApiResult<()> {
    api.set_settings(settings).await
}

#[tauri::command]
pub async fn tools(api: State<'_, Api>) -> ApiResult<Vec<ToolInfo>> {
    Ok(api.tools())
}

#[tauri::command]
pub async fn refresh_tools(api: State<'_, Api>) -> ApiResult<Vec<ToolInfo>> {
    api.refresh_tools().await
}

#[tauri::command]
pub async fn plan(api: State<'_, Api>) -> ApiResult<PlanSummary> {
    api.plan().await
}

#[tauri::command]
pub async fn jobs(
    api: State<'_, Api>,
    filter: JobFilter,
    offset: u32,
    limit: u32,
) -> ApiResult<JobPage> {
    api.jobs(filter, offset, limit)
}

#[tauri::command]
pub async fn start_run(api: State<'_, Api>) -> ApiResult<()> {
    api.start_run().await
}

#[tauri::command]
pub async fn pause(api: State<'_, Api>) -> ApiResult<()> {
    api.pause()
}

#[tauri::command]
pub async fn resume(api: State<'_, Api>) -> ApiResult<()> {
    api.resume()
}

#[tauri::command]
pub async fn cancel(api: State<'_, Api>) -> ApiResult<()> {
    api.cancel()
}

#[tauri::command]
pub async fn report(api: State<'_, Api>) -> ApiResult<String> {
    api.report()
}

#[tauri::command]
pub async fn preview(
    app: AppHandle,
    api: State<'_, Api>,
    target: PreviewTarget,
) -> ApiResult<Preview> {
    let preview = api.preview(target).await?;
    allow_preview(&app, &preview)?;
    Ok(preview)
}

#[tauri::command]
pub async fn sample(
    app: AppHandle,
    api: State<'_, Api>,
    entry: EntryId,
    target: u32,
) -> ApiResult<Sample> {
    let sample = api.sample(entry, target).await?;
    allow_preview(&app, &sample.preview)?;
    Ok(sample)
}

#[tauri::command]
pub async fn clear_previews(api: State<'_, Api>) -> ApiResult<()> {
    api.clear_previews()
}
