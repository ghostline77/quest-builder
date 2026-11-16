//! Cross-platform payload restrictions and mod.io loose-file conversion.
use super::*;

/// Checks the published cross-platform resource allowlist before packing.
pub(super) fn validate(package: &Package) -> Result<()> {
    const SUPPORTED: &[&str] = &[
        "mesh",
        "rig",
        "anims",
        "animgraph",
        "facialsetup",
        "hp",
        "xbm",
        "texarray",
        "mi",
        "mt",
        "mlsetup",
        "mltemplate",
        "mlmask",
        "inkatlas",
        "inkwidget",
        "inkstyle",
        "inkanim",
        "inkfontfamily",
        "fnt",
        "json",
        "csv",
        "streamingsector",
        "streamingblock",
        "quest",
        "questphase",
        "scene",
        "scenerid",
        "journal",
        "behavior",
        "curveset",
        "effect",
        "envparam",
        "env",
    ];
    ensure!(valid_name(&package.name), "Invalid mod.io package name");
    for path in package.payloads.keys() {
        let extension = path.rsplit('.').next().unwrap_or_default();
        ensure!(
            SUPPORTED.contains(&extension),
            "Resource is not supported on consoles: {path}"
        );
    }
    for (path, bytes) in &package.loose {
        loose(path, bytes)?;
    }
    let registration = package
        .registration
        .as_object()
        .context("mod.io registration object")?;
    for key in registration.keys() {
        ensure!(
            [
                "streaming",
                "journal",
                "localization",
                "customizations",
                "factories",
                "resource"
            ]
            .contains(&key.as_str()),
            "ArchiveXL feature is not supported by this mod.io exporter: {key}"
        );
    }
    if let Some(resource) = registration.get("resource") {
        ensure!(
            resource.get("patch").is_none(),
            "ArchiveXL resource patching is not supported on consoles"
        );
    }
    Ok(())
}

pub(super) fn loose(name: &str, bytes: &[u8]) -> Result<(String, Vec<u8>)> {
    let normalized = name.replace('\\', "/");
    let name = normalized.strip_prefix("r6/").unwrap_or(&normalized);
    if let Some(tweak) = name.strip_prefix("tweaks/") {
        let stem = tweak.rsplit_once('.').context("Tweak extension")?.0;
        let value: Value = serde_yaml::from_slice(bytes).context("Tweak JSON/YAML document")?;
        ensure!(value.is_object(), "Tweaks must contain a record mapping");
        return Ok((
            format!("tweaks/{stem}.json"),
            serde_json::to_vec_pretty(&value)?,
        ));
    }
    anyhow::bail!(
        "Loose file requires platform compilation or is not supported by the mod.io exporter: {name}"
    )
}
