//! One Tauri command per `transmute_api::Api` method, and nothing else.
//!
//! Every command is `async` so it runs on the async runtime rather than the
//! main (UI) thread. Arguments arrive in camelCase (`relativePath`), as
//! Tauri renames them; the reference is `docs/api.md`. When a method is
//! added to `Api`, add its command here, register it in `main.rs` and add
//! it to `src/api/tauriApi.ts` (a frontend test compares the lists).

use tauri::{AppHandle, State};
use transmute_api::{
    Api, ApiError, ApiResult, AppSettings, CodecDefaults, EntryDetails, EntryId, ErrorCode,
    FileCounts, FileFilter, FilePage, InputsSummary, JobFilter, JobPage, Overview, PlanSummary,
    Preset, Preview, PreviewTarget, RootId, Sample, StoredPreset, TargetTotals, ToolInfo, TreeNode,
    UserDefaults,
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
pub async fn file_counts(api: State<'_, Api>) -> ApiResult<FileCounts> {
    Ok(api.file_counts())
}

#[tauri::command]
pub async fn files(
    api: State<'_, Api>,
    filter: FileFilter,
    offset: u32,
    limit: u32,
) -> ApiResult<FilePage> {
    Ok(api.files(filter, offset, limit))
}

#[tauri::command]
pub async fn entry(api: State<'_, Api>, entry: EntryId) -> ApiResult<EntryDetails> {
    api.entry(entry)
}

#[tauri::command]
pub async fn clear_finished(api: State<'_, Api>) -> ApiResult<InputsSummary> {
    api.clear_finished()
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
pub async fn new_preset(api: State<'_, Api>) -> ApiResult<Preset> {
    api.new_preset()
}

#[tauri::command]
pub async fn save_working(
    api: State<'_, Api>,
    id: Option<String>,
    name: String,
    description: Option<String>,
) -> ApiResult<String> {
    api.save_working(id, name, description)
}

#[tauri::command]
pub async fn import_preset(api: State<'_, Api>, path: String) -> ApiResult<StoredPreset> {
    api.import_preset(&path)
}

#[tauri::command]
pub async fn export_preset(api: State<'_, Api>, id: String, path: String) -> ApiResult<()> {
    api.export_preset(&id, &path)
}

#[tauri::command]
pub async fn codecs(api: State<'_, Api>) -> ApiResult<Vec<CodecDefaults>> {
    Ok(api.codecs())
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
pub async fn save_report(api: State<'_, Api>, path: String) -> ApiResult<()> {
    api.save_report(&path)
}

#[tauri::command]
pub async fn run_totals(api: State<'_, Api>) -> ApiResult<Vec<TargetTotals>> {
    api.run_totals()
}

/// Shows a source or an output in the file manager. Done here, not by the
/// webview, so the webview needs no access to paths.
#[tauri::command]
pub async fn reveal(api: State<'_, Api>, target: PreviewTarget) -> ApiResult<()> {
    let path = api.locate(target)?;
    tauri_plugin_opener::reveal_item_in_dir(path)
        .map_err(|err| ApiError::new(ErrorCode::Io, format!("couldn't open the folder: {err}")))
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
