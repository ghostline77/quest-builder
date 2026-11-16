//! Loopback-only authoring server. Editor state never enters quest manifests.
use anyhow::{Context, Result, ensure};
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    net::{Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone)]
struct App {
    roots: Arc<Vec<PathBuf>>,
    commands: Arc<Value>,
    jobs: Arc<Mutex<BTreeMap<String, Value>>>,
    writes: Arc<Mutex<()>>,
    token: Arc<String>,
    port: u16,
}

struct ApiError(anyhow::Error);
impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        Self(error)
    }
}
impl From<std::io::Error> for ApiError {
    fn from(error: std::io::Error) -> Self {
        Self(error.into())
    }
}
impl From<serde_json::Error> for ApiError {
    fn from(error: serde_json::Error) -> Self {
        Self(error.into())
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":format!("{:#}",self.0)})),
        )
            .into_response()
    }
}
type ApiResult = std::result::Result<Json<Value>, ApiError>;

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn stamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .to_string()
}

/// Derive every tool form from the same CLI definitions used by the compiler.
pub fn command_catalog(cli: &clap::Command) -> Value {
    let mut cli = cli.clone();
    cli.build();
    json!(cli.get_subcommands().filter(|c| c.get_name() != "serve").map(|c| {
        json!({"name":c.get_name(),"about":c.get_about().map(ToString::to_string),"args":c.get_arguments().filter(|a| !matches!(a.get_id().as_str(),"help"|"version")).map(|a| {
            json!({"id":a.get_id().as_str(),"long":a.get_long(),"positional":a.is_positional(),"required":a.is_required_set(),"help":a.get_help().map(ToString::to_string),"flag":a.get_action().takes_values()==false,"multiple":a.get_num_args().is_some_and(|n|n.max_values()>1),"defaults":a.get_default_values().iter().map(|s|s.to_string_lossy().to_string()).collect::<Vec<_>>()})
        }).collect::<Vec<_>>()})
    }).collect::<Vec<_>>())
}

fn authorize(app: &App, headers: &HeaderMap) -> Result<()> {
    ensure!(
        headers.get("x-quest-token").and_then(|v| v.to_str().ok()) == Some(app.token.as_str()),
        "reload the workbench to reconnect"
    );
    Ok(())
}

fn resolve(app: &App, root: usize, relative: &str, create: bool) -> Result<PathBuf> {
    let base = app.roots.get(root).context("unknown workspace")?;
    let rel = Path::new(relative);
    ensure!(
        !rel.is_absolute()
            && rel
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_))),
        "use a workspace-relative path"
    );
    ensure!(
        !rel.components().any(|c| matches!(
            c.as_os_str().to_str(),
            Some(".git" | "target" | "work" | "generated" | "packed" | "archive")
        )),
        "generated, binary and internal directories are not editable"
    );
    let path = base.join(rel);
    let checked = if create && !path.exists() {
        let mut ancestor = path.parent().context("file parent")?;
        while !ancestor.exists() {
            ancestor = ancestor.parent().context("existing parent")?;
        }
        ancestor.canonicalize()?
    } else {
        path.canonicalize()?
    };
    ensure!(checked.starts_with(base), "path leaves the workspace");
    Ok(path)
}

fn editable(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|s| s.to_str()),
        Some("json" | "md" | "yaml" | "yml" | "toml" | "txt" | "csv" | "xl")
    )
}

fn media_type(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        "wav" => Some("audio/wav"),
        "ogg" => Some("audio/ogg"),
        "mp3" => Some("audio/mpeg"),
        "mp4" => Some("video/mp4"),
        "webm" => Some("video/webm"),
        _ => None,
    }
}

fn walk(base: &Path, dir: &Path, files: &mut Vec<Value>) -> Result<()> {
    for item in fs::read_dir(dir)? {
        let item = item?;
        let ty = item.file_type()?;
        if ty.is_symlink() {
            continue;
        }
        let path = item.path();
        let name = item.file_name().to_string_lossy().to_string();
        if ty.is_dir() {
            if !matches!(
                name.as_str(),
                ".git"
                    | "target"
                    | "work"
                    | "generated"
                    | "packed"
                    | "archive"
                    | "node_modules"
                    | ".venv"
                    | "references"
            ) && !name.starts_with('.')
            {
                walk(base, &path, files)?;
            }
        } else if (editable(&path) && item.metadata()?.len() < 8_000_000)
            || media_type(&path).is_some()
        {
            files.push(json!({"path":path.strip_prefix(base)?.to_string_lossy().replace('\\',"/"),"size":item.metadata()?.len()}));
        }
    }
    Ok(())
}

async fn bootstrap(State(app): State<App>) -> Json<Value> {
    Json(
        json!({"token":app.token.as_str(),"roots":app.roots.iter().enumerate().map(|(id,p)|json!({"id":id,"name":p.file_name().unwrap_or_default().to_string_lossy(),"path":p})).collect::<Vec<_>>(),"commands":app.commands.as_ref(),"schema":ghostline_quest_builder::contracts::SCHEMA.clone()}),
    )
}

async fn inventory(State(app): State<App>, headers: HeaderMap) -> ApiResult {
    authorize(&app, &headers)?;
    let mut roots = Vec::new();
    for (id, base) in app.roots.iter().enumerate() {
        let mut files = Vec::new();
        walk(base, base, &mut files)?;
        files.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
        roots.push(json!({"id":id,"files":files}));
    }
    Ok(Json(json!(roots)))
}

#[derive(Deserialize)]
struct FileRequest {
    root: usize,
    path: String,
}
async fn read_file(
    State(app): State<App>,
    headers: HeaderMap,
    Json(req): Json<FileRequest>,
) -> ApiResult {
    authorize(&app, &headers)?;
    let path = resolve(&app, req.root, &req.path, false)?;
    ensure_editable(&path)?;
    let bytes = fs::read(path).context("read authoring file")?;
    Ok(Json(
        json!({"text":String::from_utf8(bytes.clone()).context("file is not UTF-8 authoring text")?,"hash":hash(&bytes)}),
    ))
}

async fn media_file(
    State(app): State<App>,
    headers: HeaderMap,
    Json(req): Json<FileRequest>,
) -> std::result::Result<Response, ApiError> {
    authorize(&app, &headers)?;
    let path = resolve(&app, req.root, &req.path, false)?;
    let mime = media_type(&path).context("unsupported media format")?;
    if fs::metadata(&path)?.len() > 128_000_000 {
        return Err(anyhow::anyhow!("media exceeds the 128 MB preview limit").into());
    }
    Ok(([("content-type", mime)], fs::read(path)?).into_response())
}
fn ensure_editable(path: &Path) -> Result<()> {
    ensure!(editable(path), "unsupported authoring file type");
    Ok(())
}

#[derive(Deserialize)]
struct SaveRequest {
    root: usize,
    path: String,
    text: String,
    hash: Option<String>,
}
async fn save_file(
    State(app): State<App>,
    headers: HeaderMap,
    Json(req): Json<SaveRequest>,
) -> ApiResult {
    authorize(&app, &headers)?;
    let _guard = app
        .writes
        .lock()
        .map_err(|_| anyhow::anyhow!("save lock unavailable"))?;
    let path = resolve(&app, req.root, &req.path, true)?;
    ensure_editable(&path)?;
    if path.extension().is_some_and(|s| s == "json") {
        serde_json::from_str::<Value>(&req.text).context("invalid JSON; file was not saved")?;
    }
    if path.exists() {
        let bytes = fs::read(&path).context("read current file")?;
        if req.hash.as_deref() != Some(hash(&bytes).as_str()) {
            return Err(
                anyhow::anyhow!("file changed on disk; reload or compare before saving").into(),
            );
        }
        let backup = app.roots[req.root]
            .join("work/editor-history")
            .join(stamp())
            .join(&req.path);
        fs::create_dir_all(backup.parent().context("backup parent")?)?;
        fs::write(backup, bytes)?;
    } else if req.hash.is_some() {
        return Err(anyhow::anyhow!("file was removed on disk; reload before saving").into());
    }
    use std::io::Write;
    fs::create_dir_all(path.parent().context("save parent")?)?;
    let mut temp = tempfile::NamedTempFile::new_in(path.parent().context("save parent")?)?;
    temp.write_all(req.text.as_bytes())?;
    temp.as_file().sync_all()?;
    temp.persist(&path)
        .map_err(|e| anyhow::anyhow!("publish authoring file: {e}"))?;
    Ok(Json(json!({"hash":hash(req.text.as_bytes())})))
}

async fn catalog(State(app): State<App>, headers: HeaderMap) -> ApiResult {
    authorize(&app, &headers)?;
    Ok(Json(ghostline_quest_builder::contracts::catalog_with(
        &[],
        &[],
    )?))
}

#[derive(Deserialize)]
struct HistoryRequest {
    root: usize,
    path: String,
    version: Option<String>,
}
async fn history(
    State(app): State<App>,
    headers: HeaderMap,
    Json(req): Json<HistoryRequest>,
) -> ApiResult {
    authorize(&app, &headers)?;
    resolve(&app, req.root, &req.path, true)?;
    let base = app.roots[req.root].join("work/editor-history");
    if let Some(version) = req.version {
        if version.is_empty() || !version.bytes().all(|b| b.is_ascii_digit()) {
            return Err(anyhow::anyhow!("invalid history version").into());
        }
        let path = base
            .join(version)
            .join(&req.path)
            .canonicalize()
            .context("history version not found")?;
        if !path.starts_with(base.canonicalize()?) {
            return Err(anyhow::anyhow!("history path leaves workspace").into());
        }
        return Ok(Json(json!({"text":fs::read_to_string(path)?})));
    }
    let mut versions = Vec::new();
    if base.exists() {
        for entry in fs::read_dir(base)? {
            let entry = entry?;
            let id = entry.file_name().to_string_lossy().to_string();
            if !entry.file_type()?.is_symlink()
                && id.bytes().all(|b| b.is_ascii_digit())
                && entry.path().join(&req.path).is_file()
            {
                versions.push(id);
            }
        }
    }
    versions.sort_by(|a, b| b.cmp(a));
    versions.truncate(50);
    Ok(Json(json!({"versions":versions})))
}

#[derive(Deserialize)]
struct CancelRequest {
    id: String,
}
async fn cancel_job(
    State(app): State<App>,
    headers: HeaderMap,
    Json(req): Json<CancelRequest>,
) -> ApiResult {
    authorize(&app, &headers)?;
    let mut jobs = app
        .jobs
        .lock()
        .map_err(|_| anyhow::anyhow!("job registry unavailable"))?;
    let job = jobs.get_mut(&req.id).context("unknown job")?;
    if job["status"] == "running" {
        job["cancel_requested"] = json!(true);
    }
    Ok(Json(job.clone()))
}

#[derive(Deserialize)]
struct JobRequest {
    root: usize,
    command: String,
    args: Vec<String>,
}
async fn start_job(
    State(app): State<App>,
    headers: HeaderMap,
    Json(req): Json<JobRequest>,
) -> ApiResult {
    authorize(&app, &headers)?;
    if !app
        .commands
        .as_array()
        .context("command catalog")?
        .iter()
        .any(|c| c["name"] == req.command)
    {
        return Err(anyhow::anyhow!("unknown compiler operation").into());
    }
    let root = app
        .roots
        .get(req.root)
        .context("unknown workspace")?
        .clone();
    if req.args.len() >= 500 || req.args.iter().any(|a| a.len() >= 32768) {
        return Err(anyhow::anyhow!("too many command arguments").into());
    }
    let id = stamp();
    let folder = root.join("work/editor-jobs").join(&id);
    fs::create_dir_all(&folder)?;
    let stdout = folder.join("stdout.log");
    let stderr = folder.join("stderr.log");
    let mut child = Command::new(std::env::current_exe()?)
        .arg(&req.command)
        .args(&req.args)
        .current_dir(&root)
        .stdin(Stdio::null())
        .stdout(fs::File::create(&stdout)?)
        .stderr(fs::File::create(&stderr)?)
        .spawn()
        .context("start compiler operation")?;
    let job = json!({"id":id,"command":req.command,"args":req.args,"root":req.root,"status":"running","pid":child.id(),"stdout_path":stdout,"stderr_path":stderr});
    app.jobs
        .lock()
        .map_err(|_| anyhow::anyhow!("job registry unavailable"))?
        .insert(id.clone(), job);
    let jobs = app.jobs.clone();
    let job_id = id.clone();
    std::thread::spawn(move || {
        let mut cancelled = false;
        let result = loop {
            match child.try_wait() {
                Ok(Some(code)) => break Ok(code),
                Err(e) => break Err(e),
                Ok(None) => {}
            }
            let cancel = jobs
                .lock()
                .ok()
                .and_then(|j| j.get(&job_id).map(|v| v["cancel_requested"] == true))
                .unwrap_or(false);
            if cancel {
                cancelled = true;
                let _ = child.kill();
                break child.wait();
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        };
        if let Ok(mut jobs) = jobs.lock() {
            if let Some(job) = jobs.get_mut(&job_id) {
                match result {
                    Ok(code) => {
                        job["status"] = json!(if cancelled {
                            "cancelled"
                        } else if code.success() {
                            "succeeded"
                        } else {
                            "failed"
                        });
                        job["exit_code"] = json!(code.code());
                    }
                    Err(e) => {
                        job["status"] = json!("failed");
                        job["error"] = json!(e.to_string());
                    }
                }
            }
        }
    });
    Ok(Json(json!({"id":id})))
}

fn tail(path: &str) -> String {
    use std::io::{Seek, SeekFrom};
    let Ok(mut file) = fs::File::open(path) else {
        return String::new();
    };
    if let Ok(meta) = file.metadata() {
        let _ = file.seek(SeekFrom::Start(meta.len().saturating_sub(128_000)));
    }
    let mut bytes = Vec::new();
    let _ = file.read_to_end(&mut bytes);
    String::from_utf8_lossy(&bytes).into_owned()
}
async fn jobs(State(app): State<App>, headers: HeaderMap) -> ApiResult {
    authorize(&app, &headers)?;
    let mut result = app
        .jobs
        .lock()
        .map_err(|_| anyhow::anyhow!("job registry unavailable"))?
        .values()
        .cloned()
        .collect::<Vec<_>>();
    for job in &mut result {
        job["stdout"] = json!(tail(job["stdout_path"].as_str().unwrap_or("")));
        job["stderr"] = json!(tail(job["stderr_path"].as_str().unwrap_or("")));
    }
    Ok(Json(json!(result)))
}

async fn asset(axum::extract::Path(name): axum::extract::Path<String>) -> Response {
    if name == "inter.woff2" {
        return (
            [("content-type", "font/woff2")],
            include_bytes!("../ui/fonts/InterVariable.woff2").as_slice(),
        )
            .into_response();
    }
    let (mime, content) = match name.as_str() {
        "app.js" => (
            "text/javascript; charset=utf-8",
            include_str!("../ui/dist/assets/app.js"),
        ),
        "style.css" => (
            "text/css; charset=utf-8",
            include_str!("../ui/dist/assets/style.css"),
        ),
        "font-license.txt" => (
            "text/plain; charset=utf-8",
            include_str!("../ui/fonts/LICENSE.txt"),
        ),
        "icon-license.txt" => (
            "text/plain; charset=utf-8",
            include_str!("../ui/icons/LICENSE"),
        ),
        "icon-plus.svg" => ("image/svg+xml", include_str!("../ui/icons/plus.svg")),
        "icon-chevron-down.svg" => (
            "image/svg+xml",
            include_str!("../ui/icons/chevron-down.svg"),
        ),
        "icon-chevron-up.svg" => ("image/svg+xml", include_str!("../ui/icons/chevron-up.svg")),
        "icon-grip-vertical.svg" => (
            "image/svg+xml",
            include_str!("../ui/icons/grip-vertical.svg"),
        ),
        "icon-arrow-right.svg" => ("image/svg+xml", include_str!("../ui/icons/arrow-right.svg")),
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    let body = if cfg!(debug_assertions) {
        fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("ui/dist/assets")
                .join(&name),
        )
        .unwrap_or_else(|_| content.to_owned())
    } else {
        content.to_owned()
    };
    (
        [("content-type", mime), ("cache-control", "no-store")],
        body,
    )
        .into_response()
}

/// Start a local workbench. Filesystem writes require an origin-bound session token.
pub fn serve(roots: Vec<PathBuf>, port: u16, commands: Value) -> Result<()> {
    let roots = roots
        .iter()
        .map(|p| p.canonicalize().context("workspace must exist"))
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        roots.iter().all(|p| p.is_dir()),
        "workspace must be a directory"
    );
    let app = App {
        roots: Arc::new(roots),
        commands: Arc::new(commands),
        jobs: Arc::default(),
        writes: Arc::default(),
        token: Arc::new(hash(
            format!("{}-{}", stamp(), std::process::id()).as_bytes(),
        )),
        port,
    };
    let router = Router::new()
        .route(
            "/",
            get(|| async {
                axum::response::Html(if cfg!(debug_assertions) {
                    fs::read_to_string(
                        Path::new(env!("CARGO_MANIFEST_DIR")).join("ui/dist/index.html"),
                    )
                    .unwrap_or_else(|_| include_str!("../ui/dist/index.html").to_owned())
                } else {
                    include_str!("../ui/dist/index.html").to_owned()
                })
            }),
        )
        .route("/assets/{name}", get(asset))
        .route("/api/bootstrap", get(bootstrap))
        .route("/api/files", get(inventory))
        .route("/api/read", post(read_file))
        .route("/api/media", post(media_file))
        .route("/api/save", post(save_file))
        .route("/api/history", post(history))
        .route("/api/catalog", get(catalog))
        .route("/api/jobs", get(jobs).post(start_job))
        .route("/api/jobs/cancel", post(cancel_job))
        .layer(axum::extract::DefaultBodyLimit::max(8_000_000))
        .layer(axum::middleware::from_fn_with_state(
            app.clone(),
            origin_guard,
        ))
        .with_state(app);
    tokio::runtime::Runtime::new()?.block_on(async move {
        let listener =
            tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).await?;
        println!("Quest builder: http://127.0.0.1:{port}");
        axum::serve(listener, router)
            .await
            .context("workbench server")
    })
}

async fn origin_guard(
    State(app): State<App>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let host = format!("127.0.0.1:{}", app.port);
    let valid_host =
        request.headers().get("host").and_then(|v| v.to_str().ok()) == Some(host.as_str());
    let valid_origin = request
        .headers()
        .get("origin")
        .is_none_or(|v| v.to_str().ok() == Some(format!("http://{host}").as_str()));
    let cross_site = request
        .headers()
        .get("sec-fetch-site")
        .is_some_and(|v| v == "cross-site");
    if !valid_host || !valid_origin || cross_site {
        return StatusCode::FORBIDDEN.into_response();
    }
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert("x-content-type-options", "nosniff".parse().unwrap());
    response.headers_mut().insert("content-security-policy","default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; media-src 'self' blob:; frame-ancestors 'none'; connect-src 'self'".parse().unwrap());
    response
}
