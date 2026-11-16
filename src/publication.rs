//! Byte-verified build replacement with retained rollback directories.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

/// Atomically publishes a new directory, tolerating brief Windows sharing locks.
/// Existing destinations are never replaced.
pub fn rename_new(source: &Path, destination: &Path) -> Result<()> {
    for attempt in 0..10 {
        ensure!(
            !destination.exists(),
            "publication destination already exists: {}",
            destination.display()
        );
        match fs::rename(source, destination) {
            Ok(()) => return Ok(()),
            Err(error) => {
                let transient = cfg!(windows) && matches!(error.raw_os_error(), Some(5 | 32 | 33));
                if !transient || attempt == 9 {
                    return Err(error)
                        .with_context(|| format!("publish directory {}", destination.display()));
                }
                std::thread::sleep(std::time::Duration::from_millis(20 << attempt));
            }
        }
    }
    unreachable!("publication returns on its final attempt")
}

/// Verifies every declared raw, native and loose payload and rejects unowned files.
pub fn verify(root: &Path) -> Result<Value> {
    let root = root.canonicalize()?;
    let receipt: Value = serde_json::from_slice(&fs::read(root.join("build.json"))?)?;
    ensure!(receipt["schema_version"] == 1, "unsupported build receipt");
    let mut expected = BTreeSet::from([PathBuf::from("build.json")]);
    for (group, prefix, suffix) in [
        ("resources", "source/raw", ".json"),
        ("binaries", "source/archive", ""),
        ("loose_resources", "source/resources", ""),
    ] {
        for (depot, hash) in receipt[group].as_object().context("build hash map")? {
            let relative = if group == "loose_resources" {
                crate::project::depot_path(&format!("mod\\{}", depot.replace('/', "\\")))?
                    .strip_prefix("mod")?
                    .to_owned()
            } else {
                crate::project::depot_path(depot)?
            };
            let mut path = root.join(prefix).join(&relative).into_os_string();
            path.push(suffix);
            let path = PathBuf::from(path);
            let resolved = path.canonicalize()?;
            ensure!(
                resolved.starts_with(&root),
                "build payload escapes output root"
            );
            ensure!(
                hash.as_str() == Some(crate::evidence::digest(&fs::read(&path)?).as_str()),
                "build payload changed: {depot}"
            );
            ensure!(
                expected.insert(path.strip_prefix(&root)?.to_owned()),
                "duplicate build payload"
            );
        }
    }
    fn walk(directory: &Path, root: &Path, files: &mut BTreeSet<PathBuf>) -> Result<()> {
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            let meta = fs::symlink_metadata(&path)?;
            ensure!(!meta.file_type().is_symlink(), "build contains a link");
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                ensure!(
                    meta.file_attributes() & 0x400 == 0,
                    "build contains a reparse point"
                );
            }
            if meta.is_dir() {
                walk(&path, root, files)?;
            } else {
                files.insert(path.strip_prefix(root)?.to_owned());
            }
        }
        Ok(())
    }
    let mut actual = BTreeSet::new();
    walk(&root, &root, &mut actual)?;
    ensure!(
        actual == expected,
        "build contains unowned or missing files: {:?}",
        actual.symmetric_difference(&expected).collect::<Vec<_>>()
    );
    Ok(receipt)
}

/// Moves a fully verified candidate into place, retaining the preceding build intact.
/// The same operation can restore a retained backup; neither tree is recursively deleted.
pub fn replace(candidate: &Path, destination: &Path) -> Result<Value> {
    let candidate = candidate.canonicalize()?;
    let current = verify(&candidate)?;
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .canonicalize()?;
    let destination = parent.join(
        destination
            .file_name()
            .context("destination directory name")?,
    );
    ensure!(
        !candidate.starts_with(&destination) && !destination.starts_with(&candidate),
        "build trees must be separate"
    );
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(parent.join(".quest-builder-publication.lock"))?;
    lock.try_lock()
        .context("another build publication owns this directory")?;
    let old = if destination.exists() {
        Some(verify(&destination)?)
    } else {
        None
    };
    let stale = old
        .as_ref()
        .and_then(|v| v["binaries"].as_object())
        .into_iter()
        .flatten()
        .filter(|(key, _)| current["binaries"].get(*key).is_none())
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();
    let backup = if old.is_some() {
        let directory = tempfile::Builder::new()
            .prefix(".quest-build-backup-")
            .tempdir_in(&parent)?;
        let path = directory.path().join("build");
        fs::rename(&destination, &path)?;
        let _retained = directory.keep();
        Some(path)
    } else {
        None
    };
    if let Err(error) = fs::rename(&candidate, &destination) {
        if let Some(backup) = &backup {
            fs::rename(backup, &destination)
                .context("restore previous build after publication failure")?;
        }
        return Err(error).context("publish replacement build");
    }
    Ok(
        json!({"output":destination,"backup":backup,"stale_resources":stale,"native_serialized":current["native_serialized"],"runtime_tested":false}),
    )
}
