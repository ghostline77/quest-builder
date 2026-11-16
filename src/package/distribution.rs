use super::*;
use ghostline_red::archive;
use std::io::{Read, Write};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

pub(super) fn publish(package: &Package, output: &Path) -> Result<Value> {
    publish_layout(package, output, false)
}

pub(super) fn publish_modio(package: &Package, output: &Path) -> Result<Value> {
    super::modio::validate(package)?;
    publish_layout(package, output, true)
}

fn publish_layout(package: &Package, output: &Path, modio: bool) -> Result<Value> {
    ensure!(
        !output.exists(),
        "package output already exists: {}",
        output.display()
    );
    let parent = output
        .parent()
        .filter(|v| !v.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let stage = tempfile::Builder::new()
        .prefix(".quest-package-")
        .tempdir_in(parent)?;
    let depot = tempfile::Builder::new()
        .prefix(".quest-depot-")
        .tempdir_in(parent)?;
    for (path, bytes) in &package.payloads {
        let path = depot.path().join(depot_path(path)?);
        fs::create_dir_all(path.parent().context("depot parent")?)?;
        fs::write(path, bytes)?;
    }
    let archive_directory = if modio {
        package.name.clone()
    } else {
        "archive/pc/mod".to_owned()
    };
    let archive_relative = format!("{archive_directory}/{}.archive", package.name);
    let archive_path = stage.path().join(&archive_relative);
    fs::create_dir_all(archive_path.parent().context("archive output parent")?)?;
    archive::pack(depot.path(), &archive_path)?;
    let index = archive::read_archive(&archive_path)?;
    let expected = package
        .payloads
        .iter()
        .map(|(path, bytes)| (archive::depot_path_hash(path), (path, bytes)))
        .collect::<BTreeMap<_, _>>();
    ensure!(
        index.entries.len() == expected.len()
            && index.custom_paths.iter().collect::<BTreeSet<_>>()
                == package.payloads.keys().collect::<BTreeSet<_>>(),
        "packed archive path set differs from frozen inputs"
    );
    for (i, entry) in index.entries.iter().enumerate() {
        let (path, bytes) = expected
            .get(&entry.name_hash)
            .context("unexpected archive entry")?;
        let actual = archive::extract_entry_bytes(&archive_path, &index, i)?;
        ensure!(
            actual == **bytes,
            "archive byte verification failed for {path}"
        );
    }
    let mut files = BTreeMap::from([(archive_relative, fs::read(&archive_path)?)]);
    let registration_relative = format!("{archive_directory}/{}.archive.xl", package.name);
    files.insert(
        registration_relative,
        serde_json::to_vec_pretty(&package.registration)?,
    );
    if modio {
        files.insert(
            format!("{archive_directory}/{}.archive.json", package.name),
            serde_json::to_vec_pretty(&package.registration)?,
        );
    }
    for (name, bytes) in &package.loose {
        let (name, bytes) = if modio {
            super::modio::loose(name, bytes)?
        } else {
            (name.clone(), bytes.clone())
        };
        let path = loose_path(&name)?;
        let name = path.to_string_lossy().replace('\\', "/");
        ensure!(
            !files.contains_key(&name),
            "loose package path collides with archive output"
        );
        files.insert(name, bytes);
    }
    for (path, bytes) in &files {
        let target = stage.path().join(path);
        fs::create_dir_all(target.parent().context("package file parent")?)?;
        fs::write(target, bytes)?;
    }
    let hashes = files
        .iter()
        .map(|(name, bytes)| (name.clone(), digest(bytes)))
        .collect::<BTreeMap<_, _>>();
    let mut receipt = package.audit.clone();
    receipt["name"] = json!(package.name);
    receipt["files"] = json!(hashes);
    receipt["archive_verified"] = json!(true);
    receipt["installed"] = json!(false);
    receipt["zip_verified"] = json!(true);
    receipt["distribution"] = json!(if modio { "modio" } else { "pc" });
    if modio {
        receipt["console_resource_types_verified"] = json!(true);
        receipt["platform_runtime_verified"] = json!(false);
    }
    let receipt_bytes = serde_json::to_vec_pretty(&receipt)?;
    if !modio {
        files.insert("package.json".into(), receipt_bytes.clone());
    }
    fs::write(stage.path().join("package.json"), receipt_bytes)?;
    let zip_path = stage.path().join(format!("{}.zip", package.name));
    let mut zip = ZipWriter::new(fs::File::create(&zip_path)?);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default())
        .unix_permissions(0o644);
    for (path, bytes) in &files {
        zip.start_file(path, options)?;
        zip.write_all(bytes)?;
    }
    zip.finish()?;
    let mut zip = ZipArchive::new(fs::File::open(&zip_path)?)?;
    ensure!(
        zip.len() == files.len(),
        "ZIP file count differs from frozen package"
    );
    let mut seen = BTreeSet::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let name = entry.name().to_owned();
        ensure!(seen.insert(name.clone()), "duplicate ZIP path");
        let expected = files.get(&name).context("unexpected ZIP entry")?;
        let mut actual = Vec::new();
        entry.read_to_end(&mut actual)?;
        ensure!(
            &actual == expected,
            "ZIP byte verification failed for {name}"
        );
    }
    drop(zip);
    receipt["zip_sha256"] = json!(digest(&fs::read(zip_path)?));
    fs::write(
        stage.path().join("verification.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    if let Err(error) = crate::publication::rename_new(stage.path(), output) {
        let retained = stage.keep();
        return Err(error).with_context(|| {
            format!(
                "publish verified package {}; verified candidate retained at {}",
                output.display(),
                retained.display()
            )
        });
    }
    Ok(receipt)
}
