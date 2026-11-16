//! Calls a generated questphase from a scene.
use super::*;

pub(super) fn build(v: &Value, _h: &mut Handles) -> Result<Command> {
    let path = string(v, "phase_resource")?.replace('/', "\\");
    depot_path(&path)?;
    ensure!(
        path.ends_with(".questphase"),
        "scene phase command requires a questphase"
    );
    let input = v
        .get("input")
        .map(|v| v.as_str().context("phase input name"))
        .transpose()?
        .unwrap_or("In");
    ensure!(
        !input.is_empty() && input != "CutDestination",
        "invalid phase input name"
    );
    let mut command = Command::new(
        "questPhaseNodeDefinition",
        json!({"phaseResource":resource(&path,false),"phaseInstancePrefabs":[],"saveLock":0}),
    );
    command.inputs = vec!["CutDestination".into(), input.into()];
    if let Some(output) = v.get("output") {
        let output = output
            .as_str()
            .filter(|value| !value.is_empty())
            .context("phase output name")?;
        command.outputs = vec![(output.into(), "on_out".into())];
    }
    if v.get("wait_for_completion").and_then(Value::as_bool) == Some(false) {
        ensure!(
            list(v, "on_out")?.is_empty(),
            "non-terminating scene phase cannot have on_out connections"
        );
        command.outputs.clear();
    }
    Ok(command)
}
