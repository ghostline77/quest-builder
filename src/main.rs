use anyhow::{Context, Result, bail};
use clap::{CommandFactory, Parser, Subcommand};
use ghostline_quest_builder::{compiler, contracts, manifest};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

mod character_build;
mod workbench;

#[derive(Parser)]
#[command(version, about = "Native Cyberpunk 2077 quest authoring")]
struct Cli {
    /// Generate inspection documents without serializing native resources.
    #[arg(long, global = true)]
    documents_only: bool,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Open the local authoring workbench for one or more quest repositories.
    Serve {
        #[arg(long, required = true, num_args = 1..)]
        workspace: Vec<PathBuf>,
        #[arg(long, default_value_t = 8790)]
        port: u16,
    },
    /// Build every ready quest in a series; validate and report planned entries.
    Series {
        project: PathBuf,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        quest: Option<String>,
        #[arg(long)]
        allow_planned: bool,
        #[arg(long)]
        schema: Option<PathBuf>,
        #[arg(long)]
        game: Option<PathBuf>,
        #[command(flatten)]
        character_tools: character_build::Options,
    },
    /// Generate subtitles and voiceover maps for reviewed text and published WEMs.
    Localization {
        spec: PathBuf,
        #[arg(long)]
        manifest: Option<PathBuf>,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        schema: Option<PathBuf>,
        #[arg(long)]
        game: Option<PathBuf>,
    },
    /// Generate and inspect world placement/bounds in memory without publishing files.
    WorldPlan {
        spec: PathBuf,
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Compute exact native world NodeRef identities.
    WorldHash {
        #[arg(required=true,num_args=1..)]
        references: Vec<String>,
    },
    /// Audit generated scene graph, actors, resources and timeline without publication.
    SceneAudit {
        spec: PathBuf,
        #[arg(long)]
        dialogue: Option<PathBuf>,
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Re-extract hash-verified indexed bytes, or capture exact source-pointer evidence.
    CorpusExtract {
        index: PathBuf,
        source: String,
        #[arg(long)]
        pointer: Option<String>,
        #[arg(long)]
        schema: Option<PathBuf>,
        #[arg(long)]
        game: Option<PathBuf>,
        #[arg(long)]
        out: PathBuf,
    },
    /// Resolve explicit project dependencies and the longest matching depot owner.
    Project {
        root: PathBuf,
        #[arg(long)]
        depot: Option<String>,
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Verify and replace an existing generated build, retaining a rollback copy.
    ReplaceBuild {
        candidate: PathBuf,
        destination: PathBuf,
    },
    /// Check a generated build receipt and its complete owned file set.
    VerifyBuild { build: PathBuf },
    /// Render an authoring graph as Markdown and Mermaid without compiling resources.
    Graph {
        manifest: PathBuf,
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Install an explicitly verified package with scoped backup and byte checks.
    Install {
        package: PathBuf,
        #[arg(long)]
        game: PathBuf,
    },
    /// Verify bytes belonging to an installed package.
    VerifyInstalled {
        name: String,
        #[arg(long)]
        game: PathBuf,
    },
    /// Restore the immediately preceding installation of a package.
    Rollback {
        name: String,
        #[arg(long)]
        game: PathBuf,
    },
    /// Inspect native or JSON resources without dumping embedded buffers.
    Inspect {
        resource: PathBuf,
        #[arg(long)]
        schema: Option<PathBuf>,
        #[arg(long)]
        game: Option<PathBuf>,
        #[arg(long)]
        query: Option<String>,
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Build or refresh a source-hashed resource research index.
    Index {
        #[arg(required=true,num_args=1..)]
        sources: Vec<PathBuf>,
        #[arg(long)]
        schema: Option<PathBuf>,
        #[arg(long)]
        game: Option<PathBuf>,
        #[arg(long)]
        resume: Option<PathBuf>,
        #[arg(long)]
        known_paths: Option<PathBuf>,
        #[arg(long)]
        out: PathBuf,
    },
    /// Search a research index by content and native resource class.
    Query {
        index: PathBuf,
        query: String,
        #[arg(long)]
        kind: Option<String>,
        #[arg(long, default_value_t = 50)]
        limit: usize,
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Analyze resource dependencies, facts and cinematic event usage.
    Analyze {
        index: PathBuf,
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Exercise sequential routing signals and journal-validated snapshots.
    Scenarios {
        manifest: PathBuf,
        suite: PathBuf,
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Bind explicitly supplied observations and attachments to exact input bytes.
    EvidenceRecord {
        #[arg(long,required=true,num_args=1..)]
        inputs: Vec<PathBuf>,
        #[arg(long,num_args=1..)]
        attachments: Vec<PathBuf>,
        #[arg(long)]
        observations: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Check evidence for missing files, changed inputs and failed observations.
    EvidenceVerify {
        receipt: PathBuf,
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Generate a protected meeting, personal-link or completion lifecycle.
    Lifecycle {
        spec: PathBuf,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        schema: Option<PathBuf>,
        #[arg(long)]
        game: Option<PathBuf>,
    },
    /// Resolve a package profile, then produce a verified native archive and ZIP.
    Package {
        profile: PathBuf,
        #[arg(long, required_unless_present = "plan")]
        out: Option<PathBuf>,
        #[arg(long)]
        plan: bool,
    },
    /// Print the complete canonical authoring schema.
    Schema {
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// List every stage contract and its actual native builder availability.
    Catalog {
        #[arg(long,num_args=1..)]
        examples: Vec<PathBuf>,
        #[arg(long,num_args=1..)]
        evidence: Vec<PathBuf>,
        #[arg(long)]
        markdown: bool,
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Expand aliases/recipes without generating resources.
    Expand {
        manifest: PathBuf,
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Check schema, project-independent routing and declared fact dataflow.
    Validate {
        manifest: PathBuf,
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Produce a structural quest plan without writing game resources.
    Plan {
        manifest: PathBuf,
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Generate world sectors, devices and ArchiveXL registration directly.
    World {
        spec: PathBuf,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        schema: Option<PathBuf>,
        #[arg(long)]
        game: Option<PathBuf>,
    },
    /// Generate a native scene directly from authoring and dialogue declarations.
    Scene {
        spec: PathBuf,
        #[arg(long)]
        dialogue: Option<PathBuf>,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        schema: Option<PathBuf>,
        #[arg(long)]
        game: Option<PathBuf>,
    },
    /// Compile the complete artifact set to a new, isolated raw-output directory.
    Compile {
        manifest: PathBuf,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        inputs: Option<PathBuf>,
        #[arg(long)]
        allow_planned: bool,
        #[arg(long)]
        schema: Option<PathBuf>,
        #[arg(long)]
        game: Option<PathBuf>,
        #[command(flatten)]
        character_tools: character_build::Options,
    },
}

fn read(path: &Path) -> Result<Value> {
    serde_json::from_slice(&fs::read(path).with_context(|| format!("read {}", path.display()))?)
        .with_context(|| format!("parse {}", path.display()))
}
fn load_schema(
    path: Option<&Path>,
    game: Option<&Path>,
) -> Result<Option<ghostline_red::schema::RedSchema>> {
    Ok(Some(ghostline_red::schema::RedSchema::load(game, path)?))
}
fn emit(value: &Value, path: Option<&Path>) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    if let Some(path) = path {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, bytes)?;
    } else {
        println!("{}", String::from_utf8(bytes)?);
    }
    Ok(())
}

fn encode(
    artifacts: &compiler::Artifacts,
    schema: Option<PathBuf>,
    game: Option<PathBuf>,
) -> Result<ghostline_quest_builder::native::Binaries> {
    let schema = ghostline_red::schema::RedSchema::load(game.as_deref(), schema.as_deref())?;
    ghostline_quest_builder::native::encode(artifacts, &schema)
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let documents_only = cli.documents_only;
    let load_schema = |path: Option<&Path>,
                       game: Option<&Path>|
     -> Result<Option<ghostline_red::schema::RedSchema>> {
        if documents_only {
            Ok(None)
        } else {
            load_schema(path, game)
        }
    };
    let encode =
        |artifacts: &compiler::Artifacts, schema: Option<PathBuf>, game: Option<PathBuf>| {
            if documents_only {
                Ok(BTreeMap::new())
            } else {
                encode(artifacts, schema, game)
            }
        };
    match cli.command {
        Command::Serve { workspace, port } => {
            let commands = workbench::command_catalog(&Cli::command());
            workbench::serve(workspace, port, commands)
        }
        Command::Series {
            project,
            out,
            quest,
            allow_planned,
            schema,
            game,
            character_tools,
        } => build_series(
            &project,
            &out,
            quest.as_deref(),
            allow_planned,
            schema.as_deref(),
            game.as_deref(),
            &character_tools,
            documents_only,
        ),
        Command::Localization {
            spec,
            manifest,
            out,
            schema,
            game,
        } => {
            let source = read(&spec)?;
            let manifest = manifest.as_deref().map(read).transpose()?.map_or_else(
                || {
                    authoring_value(
                        source
                            .get("manifest")
                            .context("localization needs a dialogue manifest")?,
                        spec.parent()
                            .context("localization specification directory")?,
                    )
                },
                Ok,
            )?;
            let build = ghostline_quest_builder::localization::compile(&source, &manifest)?;
            let binaries = encode(&build.artifacts, schema, game)?;
            let resources = BTreeMap::from([(
                format!(
                    "{}_{}.archive.xl",
                    source["namespace"]
                        .as_str()
                        .context("localization namespace")?,
                    source["dialogue"]
                        .as_str()
                        .context("localization dialogue")?
                ),
                serde_json::to_vec_pretty(&build.registration)?,
            )]);
            compiler::publish_complete(&out, &build.artifacts, &binaries, &resources, false)?;
            emit(
                &json!({"output":out,"resources":build.artifacts.keys().collect::<Vec<_>>(),"native_serialized":!binaries.is_empty(),"audio_generated":false,"installed":false}),
                None,
            )
        }
        Command::WorldHash { references } => emit(
            &json!(
                references
                    .iter()
                    .map(|reference| (
                        reference,
                        ghostline_quest_builder::world::node_ref_hash(reference).to_string()
                    ))
                    .collect::<BTreeMap<_, _>>()
            ),
            None,
        ),
        Command::WorldPlan { spec, out } => {
            let build = ghostline_quest_builder::world::compile(&read(&spec)?)?;
            let resources = build
                .artifacts
                .iter()
                .map(|(depot, document)| {
                    Ok((
                        depot,
                        ghostline_quest_builder::inspect::summarize(document)?,
                    ))
                })
                .collect::<Result<BTreeMap<_, _>>>()?;
            emit(
                &json!({"resources":resources,"loose_resources":build.loose.keys().collect::<Vec<_>>(),"registration":build.registration,"native_serialized":false,"runtime_tested":false}),
                out.as_deref(),
            )
        }
        Command::SceneAudit {
            spec,
            dialogue,
            out,
        } => {
            let source = read(&spec)?;
            let dialogue = dialogue.as_deref().map(read).transpose()?.map_or_else(
                || {
                    authoring_value(
                        source
                            .get("manifest")
                            .context("scene requires dialogue manifest")?,
                        spec.parent().context("scene directory")?,
                    )
                },
                Ok,
            )?;
            let document = ghostline_quest_builder::scene::compile(&source, &dialogue)?;
            emit(
                &ghostline_quest_builder::inspect::summarize(&document)?,
                out.as_deref(),
            )
        }
        Command::CorpusExtract {
            index,
            source,
            pointer,
            schema,
            game,
            out,
        } => {
            let index = read(&index)?;
            if let Some(pointer) = pointer {
                let schema = load_schema(schema.as_deref(), game.as_deref())?;
                emit(
                    &ghostline_quest_builder::corpus::evidence(
                        &index,
                        &source,
                        &pointer,
                        schema.as_ref(),
                    )?,
                    Some(&out),
                )
            } else {
                let bytes = ghostline_quest_builder::corpus::extract(&index, &source)?;
                if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
                    fs::create_dir_all(parent)?;
                }
                anyhow::ensure!(!out.exists(), "extraction output already exists");
                fs::write(out, bytes)?;
                Ok(())
            }
        }
        Command::Project { root, depot, out } => {
            let project = ghostline_quest_builder::project::Project::open(&root)?;
            let projects=project.closure()?.iter().map(|p|json!({"root":p.root,"config":p.config,"source_raw":p.root.join("source/raw"),"source_archive":p.root.join("source/archive"),"source_resources":p.root.join("source/resources")})).collect::<Vec<_>>();
            let owner=depot.map(|depot|{let owner=project.owner(&depot)?;let(raw,archive)=owner.paths(&depot)?;Ok::<_,anyhow::Error>(json!({"depot":depot,"project":owner.config.id,"raw":raw,"archive":archive}))}).transpose()?;
            emit(&json!({"projects":projects,"owner":owner}), out.as_deref())
        }
        Command::ReplaceBuild {
            candidate,
            destination,
        } => emit(
            &ghostline_quest_builder::publication::replace(&candidate, &destination)?,
            None,
        ),
        Command::VerifyBuild { build } => {
            let receipt = ghostline_quest_builder::publication::verify(&build)?;
            emit(
                &json!({"valid":true,"build":build,"native_serialized":receipt["native_serialized"],"runtime_tested":false}),
                None,
            )
        }
        Command::Graph {
            manifest: source,
            out,
        } => {
            let checked = manifest::check(&read(&source)?)?;
            let manifest = checked
                .manifest
                .context(serde_json::to_string(&checked.diagnostics)?)?;
            let markdown = ghostline_quest_builder::scenario::graph(&manifest.plan()?)?;
            if let Some(path) = out {
                if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                    fs::create_dir_all(parent)?;
                }
                fs::write(path, markdown)?;
            } else {
                print!("{markdown}");
            }
            Ok(())
        }
        Command::Install { package, game } => emit(
            &ghostline_quest_builder::install::apply(&package, &game)?,
            None,
        ),
        Command::VerifyInstalled { name, game } => {
            let report = ghostline_quest_builder::install::verify(&game, &name)?;
            emit(&report, None)?;
            anyhow::ensure!(
                report["valid"] == true,
                "installed files differ from the receipt"
            );
            Ok(())
        }
        Command::Rollback { name, game } => emit(
            &ghostline_quest_builder::install::rollback(&game, &name)?,
            None,
        ),
        Command::Inspect {
            resource,
            schema,
            game,
            query,
            out,
        } => {
            let schema = load_schema(schema.as_deref(), game.as_deref())?;
            let summary =
                ghostline_quest_builder::inspect::read_summary(&resource, schema.as_ref())?;
            emit(
                &query.map_or(summary.clone(), |q| {
                    ghostline_quest_builder::inspect::query(&summary, &q)
                }),
                out.as_deref(),
            )
        }
        Command::Index {
            sources,
            schema,
            game,
            resume,
            known_paths,
            out,
        } => {
            let schema = load_schema(schema.as_deref(), game.as_deref())?;
            let previous = resume.as_deref().map(read).transpose()?;
            let paths = known_paths
                .as_deref()
                .map(read)
                .transpose()?
                .map(serde_json::from_value::<Vec<String>>)
                .transpose()?
                .unwrap_or_default();
            let index = ghostline_quest_builder::corpus::build(
                &sources,
                schema.as_ref(),
                previous.as_ref(),
                &paths,
            )?;
            emit(&index, Some(&out))
        }
        Command::Query {
            index,
            query,
            kind,
            limit,
            out,
        } => emit(
            &ghostline_quest_builder::corpus::query(
                &read(&index)?,
                &query,
                kind.as_deref(),
                limit,
            )?,
            out.as_deref(),
        ),
        Command::Analyze { index, out } => emit(
            &ghostline_quest_builder::corpus::analyze(&read(&index)?)?,
            out.as_deref(),
        ),
        Command::Scenarios {
            manifest: source,
            suite,
            out,
        } => {
            let checked = manifest::check(&read(&source)?)?;
            let manifest = checked
                .manifest
                .context(serde_json::to_string(&checked.diagnostics)?)?;
            let result = ghostline_quest_builder::scenario::run(&manifest.plan()?, &read(&suite)?)?;
            emit(&result, out.as_deref())?;
            anyhow::ensure!(result["passed"] == true, "scenario suite failed");
            Ok(())
        }
        Command::EvidenceRecord {
            inputs,
            attachments,
            observations,
            out,
        } => emit(
            &ghostline_quest_builder::evidence::record(
                &inputs,
                &attachments,
                &read(&observations)?,
            )?,
            Some(&out),
        ),
        Command::EvidenceVerify { receipt, out } => {
            let result = ghostline_quest_builder::evidence::verify(&read(&receipt)?)?;
            emit(&result, out.as_deref())?;
            anyhow::ensure!(
                result["valid"] == true,
                "evidence is stale, missing or failed"
            );
            Ok(())
        }
        Command::Lifecycle {
            spec,
            out,
            schema,
            game,
        } => {
            let spec = read(&spec)?;
            let document = ghostline_quest_builder::lifecycle::compile(&spec)?;
            let depot = document["Header"]["ArchiveFileName"]
                .as_str()
                .context("lifecycle depot")?
                .to_owned();
            let artifacts = BTreeMap::from([(depot, document)]);
            let binaries = encode(&artifacts, schema, game)?;
            compiler::publish(&out, &artifacts, &binaries, false)?;
            emit(
                &json!({"output":out,"resources":artifacts.keys().collect::<Vec<_>>(),"native_serialized":!binaries.is_empty()}),
                None,
            )
        }
        Command::Package { profile, out, plan } => {
            let configuration: ghostline_quest_builder::package::Profile =
                serde_json::from_value(read(&profile)?)?;
            let package = ghostline_quest_builder::package::prepare(
                &configuration,
                profile.parent().context("profile directory")?,
            )?;
            if plan {
                emit(&package.audit, out.as_deref())
            } else {
                let receipt = ghostline_quest_builder::package::publish(
                    &package,
                    &out.context("package output required")?,
                )?;
                emit(&receipt, None)
            }
        }
        Command::Scene {
            spec,
            dialogue,
            out,
            schema,
            game,
        } => {
            let source = read(&spec)?;
            let dialogue = if let Some(path) = dialogue {
                read(&path)?
            } else {
                authoring_value(
                    source
                        .get("manifest")
                        .context("scene requires a dialogue manifest")?,
                    spec.parent().context("scene authoring directory")?,
                )?
            };
            let document = ghostline_quest_builder::scene::compile(&source, &dialogue)?;
            let depot = document["Header"]["ArchiveFileName"]
                .as_str()
                .context("scene depot")?
                .to_owned();
            let artifacts = BTreeMap::from([(depot, document)]);
            let binaries = encode(&artifacts, schema, game)?;
            compiler::publish(&out, &artifacts, &binaries, false)?;
            emit(
                &json!({"output":out,"resources":artifacts.keys().collect::<Vec<_>>(),"native_serialized":!binaries.is_empty(),"installed":false}),
                None,
            )
        }
        Command::World {
            spec,
            out,
            schema,
            game,
        } => {
            let spec = read(&spec)?;
            let build = ghostline_quest_builder::world::compile(&spec)?;
            let binaries = encode(&build.artifacts, schema, game)?;
            let name = spec["name"].as_str().context("world namespace")?;
            // JSON is valid YAML, preserving the registration's typed values.
            let mut resources = BTreeMap::from([(
                format!("{name}.archive.xl"),
                serde_json::to_vec_pretty(&build.registration)?,
            )]);
            resources.extend(build.loose);
            compiler::publish_complete(&out, &build.artifacts, &binaries, &resources, false)?;
            emit(
                &json!({"output":out,"resources":build.artifacts.keys().collect::<Vec<_>>(),"native_serialized":!binaries.is_empty(),"installed":false}),
                None,
            )
        }
        Command::Schema { out } => emit(&contracts::SCHEMA, out.as_deref()),
        Command::Catalog {
            examples,
            evidence,
            markdown,
            out,
        } => {
            let examples = examples
                .iter()
                .map(|p| read(p))
                .collect::<Result<Vec<_>>>()?;
            let evidence = evidence
                .iter()
                .map(|p| read(p))
                .collect::<Result<Vec<_>>>()?;
            let catalog = contracts::catalog_with(&examples, &evidence)?;
            if markdown {
                let text = contracts::catalog_markdown(&catalog)?;
                if let Some(path) = out {
                    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                        fs::create_dir_all(parent)?;
                    }
                    fs::write(path, text)?;
                } else {
                    print!("{text}");
                }
                Ok(())
            } else {
                emit(&catalog, out.as_deref())
            }
        }
        Command::Expand {
            manifest: source,
            out,
        } => {
            let value = ghostline_quest_builder::composition::prepare(&read(&source)?)?;
            emit(
                &json!({"manifest":value.manifest,"bindings":value.bindings,"text":value.text}),
                out.as_deref(),
            )
        }
        Command::Validate {
            manifest: source,
            out,
        } => {
            let checked = manifest::check(&read(&source)?)?;
            emit(
                &json!({"valid":checked.manifest.is_some(),"scope":"schema, routing, fact dataflow, block semantics and ownership; generated resource bindings are audited by compile","diagnostics":checked.diagnostics}),
                out.as_deref(),
            )?;
            if checked.manifest.is_none() {
                bail!("quest validation failed")
            }
            Ok(())
        }
        Command::Plan {
            manifest: source,
            out,
        } => {
            let checked = manifest::check(&read(&source)?)?;
            let manifest = checked
                .manifest
                .context(serde_json::to_string(&checked.diagnostics)?)?;
            emit(&manifest.plan()?, out.as_deref())
        }
        Command::Compile {
            manifest: source,
            out,
            inputs,
            allow_planned,
            schema,
            game,
            character_tools,
        } => {
            anyhow::ensure!(
                !out.exists(),
                "build destination already exists: {}",
                out.display()
            );
            let checked = manifest::check(&read(&source)?)?;
            let manifest = checked
                .manifest
                .as_ref()
                .context(serde_json::to_string(&checked.diagnostics)?)?;
            let mut supplied = if let Some(path) = inputs.as_deref() {
                load_inputs(path)?
            } else if let Some(resources) = &manifest.resources {
                load_inputs_value(
                    resources.clone(),
                    source.parent().context("quest authoring directory")?,
                )?
            } else {
                compiler::Inputs::default()
            };
            // Reject quest and media errors before exporting the series cast.
            preflight(manifest, &checked.composition, &mut supplied, allow_planned)?;
            let loaded_schema = load_schema(schema.as_deref(), game.as_deref())?;
            let character_output = tempfile::tempdir()?;
            let mut character_report = json!({});
            if !supplied.characters.is_empty() {
                let mut characters = character_build::for_manifest(&source)?;
                characters.bind(
                    &mut supplied,
                    character_output.path(),
                    &character_tools,
                    game.as_deref(),
                    schema.as_deref(),
                )?;
                character_report = characters.report();
            }
            let build = compiler::compile(
                manifest,
                &checked.composition,
                &supplied,
                None,
                allow_planned,
            )?;
            let binaries = loaded_schema
                .as_ref()
                .map(|schema| {
                    ghostline_quest_builder::native::encode_build(&build, schema, game.as_deref())
                })
                .transpose()?
                .unwrap_or_default();
            compiler::publish_build(&out, &build, &binaries, allow_planned)?;
            emit(
                &json!({"output":out,"resources":build.artifacts.keys().collect::<Vec<_>>(),"loose_resources":build.resources.keys().collect::<Vec<_>>(),"characters":character_report,"prototype":allow_planned,"native_serialized":!binaries.is_empty(),"installed":false}),
                None,
            )
        }
    }
}

fn authoring_value(value: &Value, base: &Path) -> Result<Value> {
    if let Some(path) = value.as_str() {
        read(&base.join(path))
    } else {
        anyhow::ensure!(
            value.is_object(),
            "authoring input must be a declaration object or path"
        );
        Ok(value.clone())
    }
}

/// Compiles every selected resource set before publishing the new series directory.
fn build_series(
    project: &Path,
    out: &Path,
    selected: Option<&str>,
    allow_planned: bool,
    schema: Option<&Path>,
    game: Option<&Path>,
    character_tools: &character_build::Options,
    documents_only: bool,
) -> Result<()> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Series {
        schema_version: u32,
        #[serde(default)]
        characters: BTreeMap<String, PathBuf>,
        quests: Vec<Entry>,
    }
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Entry {
        id: String,
        manifest: Option<PathBuf>,
        inputs: Option<PathBuf>,
        #[serde(default)]
        plans: Vec<PathBuf>,
    }
    let owner = ghostline_quest_builder::project::Project::open(project)?;
    let source: Series = serde_json::from_value(read(&owner.root.join("series.json"))?)?;
    anyhow::ensure!(source.schema_version == 1, "unsupported series version");
    anyhow::ensure!(
        !out.exists(),
        "series destination already exists: {}",
        out.display()
    );
    let loaded_schema = if documents_only {
        None
    } else {
        load_schema(schema, game)?
    };
    let parent = out
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let candidate = tempfile::Builder::new()
        .prefix(".series-")
        .tempdir_in(parent)?;
    let mut characters = character_build::Characters::new(&owner.root, source.characters)?;
    let mut ids = std::collections::BTreeSet::new();
    let mut report = Vec::new();
    let mut builds = Vec::new();
    for entry in source.quests {
        anyhow::ensure!(
            ghostline_quest_builder::manifest::identifier(&entry.id)
                && ids.insert(entry.id.clone()),
            "invalid or duplicate series entry"
        );
        if selected.is_some_and(|id| id != entry.id) {
            continue;
        }
        for plan in &entry.plans {
            anyhow::ensure!(
                owner.root.join(plan).is_file(),
                "missing authored plan {}",
                plan.display()
            );
        }
        let Some(path) = entry.manifest else {
            anyhow::ensure!(
                !entry.plans.is_empty(),
                "series entry {} has no authoring",
                entry.id
            );
            report.push(json!({"id":entry.id,"status":"planned","plans":entry.plans}));
            continue;
        };
        let checked = manifest::check(&read(&owner.root.join(&path))?)?;
        let manifest = checked.manifest.as_ref().with_context(|| {
            format!(
                "{}: {}",
                entry.id,
                serde_json::to_string(&checked.diagnostics).unwrap_or_default()
            )
        })?;
        let mut inputs = if let Some(path) = entry.inputs.as_ref() {
            load_inputs(&owner.root.join(path))?
        } else if let Some(resources) = &manifest.resources {
            load_inputs_value(
                resources.clone(),
                owner
                    .root
                    .join(&path)
                    .parent()
                    .context("quest authoring directory")?,
            )?
        } else {
            compiler::Inputs::default()
        };
        characters.check(&inputs.characters)?;
        let declared_characters = inputs.characters.clone();
        if !allow_planned && manifest.stages.iter().any(|s| s.status != "ready") {
            report.push(json!({"id":entry.id,"status":"planned","manifest_valid":true,"characters":declared_characters}));
            continue;
        }
        preflight(manifest, &checked.composition, &mut inputs, allow_planned)?;
        characters.bind(
            &mut inputs,
            &candidate.path().join("characters"),
            character_tools,
            game,
            schema,
        )?;
        let build = compiler::compile(manifest, &checked.composition, &inputs, None, allow_planned)
            .with_context(|| format!("compile {}", entry.id))?;
        let binaries = loaded_schema
            .as_ref()
            .map(|schema| ghostline_quest_builder::native::encode_build(&build, schema, game))
            .transpose()?
            .unwrap_or_default();
        report.push(json!({"id":entry.id,"status":"built","resources":build.artifacts.len(),"characters":declared_characters,"native_serialized":!binaries.is_empty()}));
        builds.push((entry.id, build, binaries));
    }
    anyhow::ensure!(
        selected.is_none_or(|id| ids.contains(id)),
        "unknown series quest"
    );
    for (id, build, binaries) in builds {
        compiler::publish_build(&candidate.path().join(id), &build, &binaries, allow_planned)?;
    }
    let report = json!({"schema_version":1,"project":owner.config.id,"quests":report,"characters":characters.report(),"installed":false});
    fs::write(
        candidate.path().join("series-build.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    characters.relocate_reports(out)?;
    ghostline_quest_builder::publication::rename_new(candidate.path(), out)
        .with_context(|| format!("publish series {}", out.display()))?;
    emit(&report, None)
}

fn preflight(
    manifest: &manifest::Manifest,
    composition: &ghostline_quest_builder::composition::Composition,
    inputs: &mut compiler::Inputs,
    allow_planned: bool,
) -> Result<()> {
    let requested = std::mem::take(&mut inputs.characters);
    let result = compiler::compile(manifest, composition, inputs, None, allow_planned);
    inputs.characters = requested;
    let build = result?;
    ghostline_quest_builder::audit::bound_interfaces(&build.artifacts)?;
    ghostline_quest_builder::audit::declared_interfaces(&build.bindings, &build.artifacts)?;
    Ok(())
}

fn load_inputs(path: &Path) -> Result<compiler::Inputs> {
    load_inputs_value(read(path)?, path.parent().context("build input directory")?)
}

fn load_inputs_value(mut value: Value, base: &Path) -> Result<compiler::Inputs> {
    if let Some(paths) = value.get_mut("tweaks") {
        for path in paths
            .as_object_mut()
            .context("tweaks must map installed paths to authored files")?
            .values_mut()
        {
            *path = json!(base.join(path.as_str().context("TweakXL authoring file")?));
        }
    }
    for field in ["media", "archives"] {
        if let Some(paths) = value.get_mut(field) {
            for path in paths
                .as_array_mut()
                .context("media and archives must list published paths")?
            {
                *path = json!(base.join(path.as_str().context("media directory")?));
            }
        }
    }
    for field in ["assets", "loose"] {
        if let Some(inputs) = value.get_mut(field).and_then(Value::as_object_mut) {
            for input in inputs.values_mut() {
                let source = input["path"].as_str().context("external artifact path")?;
                input["path"] = json!(base.join(source));
            }
        }
    }
    if let Some(resources) = value.get_mut("extra_resources") {
        for resource in resources
            .as_object_mut()
            .context("extra_resources must be a depot map")?
            .values_mut()
        {
            *resource = authoring_value(resource, base)?;
        }
    }
    for field in [
        "worlds",
        "lifecycles",
        "atlases",
        "font_families",
        "websites",
        "effects",
        "workspots",
    ] {
        if let Some(worlds) = value.get_mut(field) {
            for spec in worlds
                .as_array_mut()
                .context("worlds must be a list of authoring specifications")?
            {
                *spec = authoring_value(spec, base)?;
            }
        }
    }
    if let Some(scenes) = value.get_mut("scenes") {
        for input in scenes
            .as_array_mut()
            .context("scenes must be a list of authoring/dialogue pairs")?
        {
            let source = input.get("spec").context("scene input requires spec")?;
            let scene_base = source
                .as_str()
                .map(|p| base.join(p))
                .and_then(|p| p.parent().map(Path::to_path_buf))
                .unwrap_or_else(|| base.into());
            let spec = authoring_value(source, base)?;
            let dialogue = if let Some(dialogue) = input.get("dialogue") {
                authoring_value(dialogue, base)?
            } else {
                authoring_value(
                    spec.get("manifest")
                        .context("scene input requires dialogue")?,
                    &scene_base,
                )?
            };
            input["spec"] = spec;
            input["dialogue"] = dialogue;
        }
    }
    if let Some(localizations) = value.get_mut("localizations") {
        for input in localizations
            .as_array_mut()
            .context("localizations must be a list of spec/manifest pairs")?
        {
            let source = input.get("spec").context("localization input needs spec")?;
            let spec_base = source
                .as_str()
                .map(|p| base.join(p))
                .and_then(|p| p.parent().map(Path::to_path_buf))
                .unwrap_or_else(|| base.into());
            let spec = authoring_value(source, base)?;
            let manifest = if let Some(manifest) = input.get("manifest") {
                authoring_value(manifest, base)?
            } else {
                authoring_value(
                    spec.get("manifest")
                        .context("localization input needs dialogue manifest")?,
                    &spec_base,
                )?
            };
            input["spec"] = spec;
            input["manifest"] = manifest;
        }
    }
    serde_json::from_value(value).context("invalid generated build inputs")
}
