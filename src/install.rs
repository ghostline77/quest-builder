//! Explicit, byte-verified installation with scoped backups and rollback.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

fn relative(value: &str) -> Result<PathBuf> {
    let value = crate::project::depot_path(&format!("mod\\{}", value.replace('/', "\\")))?;
    Ok(value
        .strip_prefix("mod")
        .context("relative install path")?
        .into())
}
fn game_root(game: &Path) -> Result<PathBuf> {
    let root = game.canonicalize()?;
    ensure!(
        root.join("bin/x64/Cyberpunk2077.exe").is_file(),
        "target is not a Cyberpunk 2077 installation"
    );
    Ok(root)
}
fn target(root: &Path, path: &str) -> Result<PathBuf> {
    let relative = relative(path)?;
    let mut target = root.to_owned();
    for component in relative.components() {
        target.push(component);
        if let Ok(metadata) = fs::symlink_metadata(&target) {
            ensure!(
                !metadata.file_type().is_symlink(),
                "installation target traverses a symlink"
            );
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                ensure!(
                    metadata.file_attributes() & 0x400 == 0,
                    "installation target traverses a reparse point"
                );
            }
        }
    }
    Ok(target)
}
fn hash(path: &Path) -> Result<Option<String>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(crate::evidence::digest(&bytes))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}
fn atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("file parent")?;
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}
fn state_path(root: &Path, name: &str) -> Result<PathBuf> {
    ensure!(crate::package::valid_name(name), "invalid package identity");
    Ok(root
        .join(".quest-builder/installed")
        .join(format!("{name}.json")))
}
fn lock(root: &Path) -> Result<fs::File> {
    let path = root.join(".quest-builder/install.lock");
    fs::create_dir_all(path.parent().context("lock parent")?)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    file.try_lock()
        .context("another quest-builder installation transaction is active")?;
    Ok(file)
}
fn read_optional(path: &Path) -> Result<Option<Value>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Checks a package's frozen installation files before any game file is changed.
pub fn verify_package(package: &Path) -> Result<Value> {
    let receipt: Value = serde_json::from_slice(&fs::read(package.join("verification.json"))?)?;
    ensure!(
        receipt["schema_version"] == 1
            && receipt["archive_verified"] == true
            && receipt["zip_verified"] == true,
        "package lacks successful native distribution verification"
    );
    let files = receipt["files"]
        .as_object()
        .context("package file receipt")?;
    ensure!(!files.is_empty(), "empty installation set");
    let mut names = BTreeSet::new();
    for (path, expected) in files {
        ensure!(
            names.insert(path.replace('\\', "/").to_lowercase()),
            "case-colliding installation targets"
        );
        let bytes = fs::read(package.join(relative(path)?))?;
        ensure!(
            expected.as_str() == Some(crate::evidence::digest(&bytes).as_str()),
            "package file changed after verification: {path}"
        );
    }
    Ok(receipt)
}

fn restore(root: &Path, backup: &Path, snapshot: &Value) -> Result<()> {
    let mut failures = Vec::new();
    for (path, expected) in snapshot["before"].as_object().context("backup manifest")? {
        let restored = (|| -> Result<()> {
            let destination = target(root, path)?;
            if expected.is_null() {
                if destination.is_file() {
                    fs::remove_file(&destination)?;
                }
            } else {
                let bytes = fs::read(backup.join("files").join(relative(path)?))?;
                ensure!(
                    expected.as_str() == Some(crate::evidence::digest(&bytes).as_str()),
                    "backup bytes changed for {path}"
                );
                atomic(&destination, &bytes)?;
            }
            Ok(())
        })();
        if let Err(error) = restored {
            failures.push(format!("{path}: {error:#}"));
        }
    }
    ensure!(
        failures.is_empty(),
        "rollback failures: {}",
        failures.join("; ")
    );
    let state = state_path(root, snapshot["name"].as_str().context("snapshot package")?)?;
    if snapshot["previous_receipt"].is_null() {
        if state.is_file() {
            fs::remove_file(state)?;
        }
    } else {
        atomic(
            &state,
            &serde_json::to_vec_pretty(&snapshot["previous_receipt"])?,
        )?;
    }
    Ok(())
}

/// Installs only the verified file set, backing up every overwritten or retired file.
/// This function is invoked explicitly; compilation and packaging never call it.
pub fn apply(package: &Path, game: &Path) -> Result<Value> {
    let root = game_root(game)?;
    let _lock = lock(&root)?;
    let receipt = verify_package(package)?;
    let name = receipt["name"].as_str().context("package name")?;
    let state = state_path(&root, name)?;
    let previous = read_optional(&state)?;
    let files = receipt["files"].as_object().context("package files")?;
    let mut payloads = BTreeMap::new();
    for (path, expected) in files {
        let bytes = fs::read(package.join(relative(path)?))?;
        ensure!(
            expected.as_str() == Some(crate::evidence::digest(&bytes).as_str()),
            "package input changed while freezing installation"
        );
        payloads.insert(path.clone(), bytes);
    }
    let mut affected = files.keys().cloned().collect::<BTreeSet<_>>();
    if let Some(previous) = &previous {
        for (path, expected) in previous["files"]
            .as_object()
            .context("previous installation receipt")?
        {
            let current = hash(&target(&root, path)?)?;
            ensure!(
                current.is_none() || current.as_deref() == expected.as_str(),
                "installed file was changed independently: {path}"
            );
            affected.insert(path.clone());
        }
    }
    let directory = root.join(".quest-builder/installed");
    if directory.is_dir() {
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            if path == state || path.extension().is_none_or(|v| v != "json") {
                continue;
            }
            let foreign: Value = serde_json::from_slice(&fs::read(path)?)?;
            for path in foreign["files"]
                .as_object()
                .context("installed file ownership")?
                .keys()
            {
                ensure!(
                    !affected.iter().any(|v| v.eq_ignore_ascii_case(path)),
                    "target is owned by another installed package: {path}"
                );
            }
        }
    }
    let backup_root = root.join(".quest-builder/backups");
    fs::create_dir_all(&backup_root)?;
    let backup = tempfile::Builder::new()
        .prefix(&format!("{name}-"))
        .tempdir_in(&backup_root)?;
    let backup_id = backup
        .path()
        .file_name()
        .context("backup identity")?
        .to_string_lossy()
        .into_owned();
    let mut before = BTreeMap::new();
    let mut after = BTreeMap::new();
    for path in &affected {
        let destination = target(&root, path)?;
        let previous_bytes = if destination.exists() {
            ensure!(
                destination.is_file(),
                "installation target is not a file: {path}"
            );
            Some(fs::read(&destination)?)
        } else {
            None
        };
        if let Some(bytes) = &previous_bytes {
            let copy = backup.path().join("files").join(relative(path)?);
            fs::create_dir_all(copy.parent().context("backup directory")?)?;
            fs::write(copy, bytes)?;
        }
        before.insert(
            path.clone(),
            previous_bytes.as_ref().map(|v| crate::evidence::digest(v)),
        );
        after.insert(
            path.clone(),
            payloads.get(path).map(|v| crate::evidence::digest(v)),
        );
    }
    let snapshot = json!({"schema_version":1,"name":name,"game":root,"backup_id":backup_id,"before":before,"after":after,"previous_receipt":previous});
    fs::write(
        backup.path().join("snapshot.json"),
        serde_json::to_vec_pretty(&snapshot)?,
    )?;
    let backup = backup.keep();
    let operation = (|| -> Result<Value> {
        for path in &affected {
            let destination = target(&root, path)?;
            ensure!(
                hash(&destination)? == before[path],
                "game file changed during installation: {path}"
            );
            if let Some(bytes) = payloads.get(path) {
                atomic(&destination, bytes)?;
            } else if destination.is_file() {
                fs::remove_file(destination)?;
            }
        }
        for (path, expected) in &after {
            ensure!(
                &hash(&target(&root, path)?)? == expected,
                "installed byte verification failed: {path}"
            );
        }
        let installed = json!({"schema_version":1,"name":name,"files":receipt["files"],"backup_id":backup_id,"package_zip_sha256":receipt["zip_sha256"],"installed":true,"runtime_tested":false});
        atomic(&state, &serde_json::to_vec_pretty(&installed)?)?;
        Ok(installed)
    })();
    match operation {
        Ok(receipt) => Ok(receipt),
        Err(error) => {
            let rollback = restore(&root, &backup, &snapshot);
            match rollback {
                Ok(()) => {
                    Err(error.context("installation failed; previous bytes and ownership restored"))
                }
                Err(rollback) => anyhow::bail!(
                    "installation failed: {error:#}; rollback failed: {rollback:#}; backup retained at {}",
                    backup.display()
                ),
            }
        }
    }
}

/// Verifies the current bytes of an explicitly installed package.
pub fn verify(game: &Path, name: &str) -> Result<Value> {
    let root = game_root(game)?;
    let receipt = read_optional(&state_path(&root, name)?)?
        .context("package is not installed by quest-builder")?;
    let mut problems = Vec::new();
    for (path, expected) in receipt["files"]
        .as_object()
        .context("installation receipt")?
    {
        let actual = hash(&target(&root, path)?)?;
        if actual.as_deref() != expected.as_str() {
            problems.push(json!({"path":path,"expected":expected,"actual":actual}));
        }
    }
    Ok(json!({"name":name,"valid":problems.is_empty(),"problems":problems,"runtime_tested":false}))
}

/// Restores the immediately preceding installation, refusing independently changed files.
pub fn rollback(game: &Path, name: &str) -> Result<Value> {
    let root = game_root(game)?;
    let _lock = lock(&root)?;
    let state = read_optional(&state_path(&root, name)?)?.context("package is not installed")?;
    let id = state["backup_id"].as_str().context("installation backup")?;
    ensure!(
        !id.is_empty() && !id.contains(['/', '\\', ':']) && id != "." && id != "..",
        "invalid backup identity"
    );
    let backup = root.join(".quest-builder/backups").join(id);
    let snapshot: Value = serde_json::from_slice(&fs::read(backup.join("snapshot.json"))?)?;
    ensure!(
        snapshot["schema_version"] == 1
            && snapshot["name"] == name
            && snapshot["game"] == json!(root),
        "backup belongs to another installation"
    );
    for (path, expected) in snapshot["after"]
        .as_object()
        .context("backup installed-byte map")?
    {
        ensure!(
            hash(&target(&root, path)?)?.as_deref() == expected.as_str(),
            "cannot roll back independently changed file {path}"
        );
    }
    restore(&root, &backup, &snapshot)?;
    Ok(
        json!({"name":name,"rolled_back":true,"backup_id":id,"previous_installation":snapshot["previous_receipt"],"runtime_tested":false}),
    )
}
