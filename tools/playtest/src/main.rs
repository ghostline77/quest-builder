//! Developer-only game controls through a small CET file protocol.
mod desktop;

use anyhow::{Context, Result, bail, ensure};
use clap::{Parser, Subcommand};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Parser)]
struct Args {
    #[arg(long)]
    game: PathBuf,
    #[command(subcommand)]
    action: Action,
}

#[derive(Subcommand)]
enum Action {
    Install,
    State,
    ListSaves,
    /// Discards the current unsaved session and loads this exact save folder.
    Load {
        save: String,
    },
    /// Starts a closed game directly into this exact save folder.
    Launch {
        save: String,
    },
    /// Clicks CET's live toolbar and verifies a new controller instance.
    Reload,
    /// Exits without creating a save, allowing archive replacement.
    Quit,
    Freefly {
        #[arg(value_parser = ["on", "off"])]
        state: String,
    },
}

fn read(path: &Path) -> Result<Value> {
    serde_json::from_slice(&fs::read(path)?).with_context(|| format!("Read {}", path.display()))
}

fn state(dir: &Path) -> Result<Value> {
    let path = dir.join("state.json");
    ensure!(
        fs::metadata(&path)?.modified()?.elapsed()? < Duration::from_secs(5),
        "Test controller heartbeat is stale"
    );
    read(&path)
}

fn wait(dir: &Path, seconds: u64, accept: impl Fn(&Value) -> bool) -> Result<Value> {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(seconds) {
        if let Ok(value) = state(dir) {
            if accept(&value) {
                return Ok(value);
            }
        }
        thread::sleep(Duration::from_millis(100));
    }
    bail!(
        "Timed out after {seconds}s; last controller state: {}",
        read(&dir.join("state.json")).unwrap_or(Value::Null)
    )
}

fn binding(game: &Path) -> Result<u64> {
    read(&game.join("bin/x64/plugins/cyber_engine_tweaks/bindings.json"))?["cet"]["overlay_key"]
        .as_u64()
        .context("CET overlay key missing")
}

fn overlay(game: &Path, dir: &Path, enabled: bool) -> Result<Value> {
    for attempt in 0..2 {
        let s = state(dir)?;
        if s["overlay"].as_bool() == Some(enabled) {
            return Ok(s);
        }
        desktop::key(binding(game)?)?;
        match wait(dir, 5, |s| s["overlay"].as_bool() == Some(enabled)) {
            Ok(s) => return Ok(s),
            Err(error) if attempt == 1 => return Err(error),
            Err(_) => {}
        }
    }
    unreachable!()
}

fn request(dir: &Path, mut value: Value) -> Result<Value> {
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_nanos()
        .to_string();
    value["id"] = json!(id);
    fs::write(dir.join("command.json"), serde_json::to_vec(&value)?)?;
    let result = wait(dir, 90, |s| {
        s["result"]["id"] == id
            && matches!(s["result"]["status"].as_str(), Some("complete" | "error"))
    })?;
    ensure!(
        result["result"]["status"] != "error",
        "Controller rejected request: {}",
        result["result"]
    );
    Ok(result)
}

fn main() -> Result<()> {
    let args = Args::parse();
    let dir = args
        .game
        .join("bin/x64/plugins/cyber_engine_tweaks/mods/ghostline_playtest");
    let result = match args.action {
        Action::Install => {
            fs::create_dir_all(&dir)?;
            fs::write(dir.join("init.lua"), include_str!("../cet/init.lua"))?;
            json!({"installed": dir, "activation": "Launch game or reload CET mods"})
        }
        Action::State => state(&dir)?,
        Action::Launch { save } => {
            ensure!(
                !save.is_empty() && save.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
                "Invalid save folder name"
            );
            ensure!(
                !desktop::is_running(),
                "Game already running; use load instead"
            );
            let previous_boot = read(&dir.join("state.json"))
                .ok()
                .map(|s| s["boot"].clone());
            use std::os::windows::process::CommandExt;
            Command::new(args.game.join("bin/x64/Cyberpunk2077.exe"))
                .current_dir(args.game.join("bin/x64"))
                .arg(format!("-save={save}"))
                .creation_flags(0x08000000)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .context("Launch game")?;
            wait(&dir, 180, |s| {
                s["player"] == true
                    && s["pregame"] == false
                    && previous_boot.as_ref() != Some(&s["boot"])
            })?
        }
        Action::ListSaves => request(&dir, json!({"action": "list-saves"}))?,
        Action::Load { save } => {
            overlay(&args.game, &dir, false)?;
            request(&dir, json!({"action": "load-save", "save": save}))?
        }
        Action::Freefly { state: enabled } => {
            overlay(&args.game, &dir, false)?;
            request(
                &dir,
                json!({"action": "freefly", "enabled": enabled == "on"}),
            )?
        }
        Action::Reload => {
            let before = state(&dir)?;
            let open = overlay(&args.game, &dir, true)?;
            let t = &open["toolbar"];
            let n = |key: &str| {
                t[key]
                    .as_f64()
                    .with_context(|| format!("Missing toolbar {key}"))
            };
            // CET has seven equal-width buttons. The reload button is the last.
            let x = n("x")? + n("width")? * 13.0 / 14.0 - 4.0;
            let frame = t["frame"].as_f64().unwrap_or(n("height")? / 2.0 - 8.0);
            let y = n("y")? + n("height")? - 8.0 - frame / 2.0;
            desktop::click(x.round() as i32, y.round() as i32)?;
            wait(&dir, 60, |s| {
                s["boot"] != before["boot"] && s["ticks"].as_u64().unwrap_or(0) >= 2
            })?;
            overlay(&args.game, &dir, false)?
        }
        Action::Quit => {
            overlay(&args.game, &dir, false)?;
            let process = desktop::game_process()?;
            let result = request(&dir, json!({"action": "quit"}))?;
            desktop::wait_exit(&process)?;
            result
        }
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
