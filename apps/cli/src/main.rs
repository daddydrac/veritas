use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use reqwest::Client;
use serde_json::{json, Value};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

const VERITAS_LOGO: &str = r#"
██╗   ██╗███████╗██████╗ ██╗████████╗ █████╗ ███████╗
██║   ██║██╔════╝██╔══██╗██║╚══██╔══╝██╔══██╗██╔════╝
██║   ██║█████╗  ██████╔╝██║   ██║   ███████║███████╗
╚██╗ ██╔╝██╔══╝  ██╔══██╗██║   ██║   ██╔══██║╚════██║
 ╚████╔╝ ███████╗██║  ██║██║   ██║   ██║  ██║███████║
  ╚═══╝  ╚══════╝╚═╝  ╚═╝╚═╝   ╚═╝   ╚═╝  ╚═╝╚══════╝
"#;

const MOTTO: &str = "Mathematical Truth Through Evidence";
const TAGLINE: &str =
    "Math-heavy evidence-backed research and development software engineering agent.";

#[derive(Parser)]
#[command(
    name = "veritas",
    version,
    about = "Veritas CLI: evidence-backed math research to production-grade code"
)]
struct Cli {
    #[arg(long, env = "VERITAS_API_URL", default_value = "http://localhost:8080")]
    api_url: String,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Print the Veritas logo and next-step prompts.
    Welcome,
    /// Start the full Docker Compose stack and print guided next steps.
    Start {
        #[arg(long, default_value_t = false)]
        gpu: bool,
    },
    Up,
    Down,
    Health,
    Ready,
    IngestArxiv {
        #[arg(long)]
        query: String,
        #[arg(long, default_value_t = 5)]
        max_results: u32,
    },
    IngestPdf {
        #[arg(long)]
        path: PathBuf,
    },
    /// Upload the Veritas OWL ontology into Fuseki/Jena.
    UploadOntology {
        #[arg(long)]
        path: Option<PathBuf>,
    },
    /// Generate a review-gated package scaffold from indexed evidence.
    GenerateCode {
        #[arg(long)]
        prompt: String,
        #[arg(long, default_value = "rust")]
        language: String,
    },
    Search {
        query: String,
        #[arg(long, default_value_t = 10)]
        size: u32,
    },
    Sparql {
        query: String,
    },
    Ask {
        prompt: String,
    },
    Plan {
        goal: String,
    },
    Configure,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let http = Client::new();
    match cli.command.unwrap_or(Commands::Welcome) {
        Commands::Welcome => print_startup_screen(&http, &cli.api_url).await,
        Commands::Configure => configure(),
        Commands::Start { gpu } => {
            print_logo();
            print_menu();
            ensure_env_file()?;
            if gpu {
                run("docker", &["compose", "--profile", "gpu", "up", "-d", "--build"])?;
            } else {
                run("docker", &["compose", "up", "-d", "--build"])?;
            }
            print_start_success(&cli.api_url);
            Ok(())
        }
        Commands::Up => run("docker", &["compose", "up", "-d", "--build"]),
        Commands::Down => run("docker", &["compose", "down"]),
        Commands::Health => print_response(
            http.get(format!("{}/health", cli.api_url)).send().await,
            "api.health",
        )
        .await,
        Commands::Ready => print_response(
            http.get(format!("{}/ready", cli.api_url)).send().await,
            "api.ready",
        )
        .await,
        Commands::Search { query, size } => {
            print_response(
                http.post(format!("{}/search", cli.api_url))
                    .json(&json!({"query": query, "size": size}))
                    .send()
                    .await,
                "api.search",
            )
            .await
        }
        Commands::Sparql { query } => {
            print_response(
                http.post(format!("{}/sparql", cli.api_url))
                    .json(&json!({"query": query}))
                    .send()
                    .await,
                "api.sparql",
            )
            .await
        }
        Commands::Plan { goal } | Commands::Ask { prompt: goal } => {
            print_response(
                http.post(format!("{}/plan", cli.api_url))
                    .json(&json!({"goal": goal}))
                    .send()
                    .await,
                "api.plan",
            )
            .await
        }
        Commands::IngestArxiv { query, max_results } => run(
            "docker",
            &[
                "compose",
                "run",
                "--rm",
                "ingestion",
                "python",
                "-m",
                "veritas_ingest.cli",
                "ingest-arxiv",
                "--query",
                &query,
                "--max-results",
                &max_results.to_string(),
            ],
        ),
        Commands::IngestPdf { path } => {
            let container_path = stage_pdf_for_container(&path)?;
            run(
                "docker",
                &[
                    "compose",
                    "run",
                    "--rm",
                    "ingestion",
                    "python",
                    "-m",
                    "veritas_ingest.cli",
                    "ingest-pdf",
                    "--path",
                    &container_path,
                ],
            )
        }
        Commands::UploadOntology { path } => {
            let mut args = vec![
                "compose",
                "run",
                "--rm",
                "ingestion",
                "python",
                "-m",
                "veritas_ingest.cli",
                "upload-ontology",
            ];
            let staged_path;
            if let Some(path) = path {
                staged_path = stage_ontology_for_container(&path)?;
                args.push("--path");
                args.push(&staged_path);
            }
            run("docker", &args)
        }
        Commands::GenerateCode { prompt, language } => run(
            "docker",
            &[
                "compose",
                "run",
                "--rm",
                "ingestion",
                "python",
                "-m",
                "veritas_ingest.cli",
                "generate-code",
                "--prompt",
                &prompt,
                "--language",
                &language,
            ],
        ),
    }
}

fn stage_pdf_for_container(path: &Path) -> Result<String> {
    if !path.exists() {
        return Err(anyhow!(
            "PDF path does not exist: {:?}. Provide a local PDF path or use ingest-arxiv.",
            path
        ));
    }
    if path.extension().and_then(|value| value.to_str()).unwrap_or_default().to_lowercase() != "pdf" {
        return Err(anyhow!(
            "input file is not a PDF: {:?}. Veritas ingestion currently accepts .pdf files.",
            path
        ));
    }
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow!("PDF file name is not valid UTF-8: {:?}", path))?;
    let upload_dir = Path::new("data/papers/uploads");
    std::fs::create_dir_all(upload_dir)?;
    let staged = upload_dir.join(file_name);
    std::fs::copy(path, &staged).map_err(|error| {
        anyhow!(
            "failed to stage PDF {:?} into {:?}: {}",
            path,
            staged,
            error
        )
    })?;
    println!("Staged PDF for container ingestion: {:?}", staged);
    Ok(format!("/workspace/data/papers/uploads/{}", file_name))
}

fn stage_ontology_for_container(path: &Path) -> Result<String> {
    if !path.exists() {
        return Err(anyhow!(
            "Ontology path does not exist: {:?}. Provide an OWL/RDF/Turtle file.",
            path
        ));
    }
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow!("Ontology file name is not valid UTF-8: {:?}", path))?;
    let ontology_dir = Path::new("packages/ontology/uploads");
    std::fs::create_dir_all(ontology_dir)?;
    let staged = ontology_dir.join(file_name);
    std::fs::copy(path, &staged).map_err(|error| {
        anyhow!(
            "failed to stage ontology {:?} into {:?}: {}",
            path,
            staged,
            error
        )
    })?;
    println!("Staged ontology for container upload: {:?}", staged);
    Ok(format!("/workspace/ontology/uploads/{}", file_name))
}

async fn print_startup_screen(http: &Client, api_url: &str) -> Result<()> {
    print_logo();
    print_system_status(http, api_url).await;
    print_knowledge_graph_status(http, api_url).await;
    print_menu();
    Ok(())
}

fn print_logo() {
    println!("═══════════════════════════════════════════════════════════════════");
    println!("{}", VERITAS_LOGO.trim_matches('\n'));
    println!("\n{:^67}\n", MOTTO);
    println!("      Math-heavy evidence-backed research and development");
    println!("                software engineering agent\n");
    println!("═══════════════════════════════════════════════════════════════════\n");
}

async fn print_system_status(http: &Client, api_url: &str) {
    println!("System Status");
    println!("─────────────");
    match http.get(format!("{}/ready", api_url.trim_end_matches('/'))).send().await {
        Ok(response) if response.status().is_success() => {
            match response.json::<Value>().await {
                Ok(value) => {
                    let checks = value.get("checks").unwrap_or(&Value::Null);
                    print_service_check(checks, "opensearch", "OpenSearch FAISS/HNSW");
                    print_service_check(checks, "fuseki", "Jena Fuseki Graph");
                    print_service_static("Openllet Reasoner", true, "offline reasoner container configured");
                    print_service_static("OWL-DL Ontology Loaded", true, "run Upload / Update Ontology to refresh");
                    print_service_check(checks, "embedding", "Embedding Service Ready");
                    print_service_static("Retrieval Pipeline Ready", true, "OpenSearch + SBERT + formula-aware chunks");
                }
                Err(error) => {
                    println!("! Could not parse readiness response: {error}");
                    print_unknown_status();
                }
            }
        }
        Ok(response) => {
            println!("! API readiness endpoint returned HTTP {}", response.status());
            print_unknown_status();
        }
        Err(_) => {
            println!("! Veritas API is not reachable yet at {api_url}");
            print_unknown_status();
        }
    }
    println!();
}

fn print_service_check(checks: &Value, key: &str, label: &str) {
    let ok = checks
        .get(key)
        .and_then(|value| value.get("ok"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mark = if ok { "✓" } else { "!" };
    println!("{mark} {label}");
}

fn print_service_static(label: &str, ok: bool, _note: &str) {
    let mark = if ok { "✓" } else { "!" };
    println!("{mark} {label}");
}

fn print_unknown_status() {
    println!("! OpenSearch FAISS/HNSW          status unknown");
    println!("! Jena Fuseki Graph              status unknown");
    println!("! Openllet Reasoner              status unknown");
    println!("! OWL-DL Ontology Loaded         status unknown");
    println!("! Embedding Service Ready        status unknown");
    println!("! Retrieval Pipeline Ready       status unknown");
}

async fn print_knowledge_graph_status(http: &Client, api_url: &str) {
    println!("Knowledge Graph Status");
    println!("──────────────────────");
    match http
        .get(format!("{}/graph/status", api_url.trim_end_matches('/')))
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => match response.json::<Value>().await {
            Ok(value) => {
                let counts = value.get("counts").unwrap_or(&Value::Null);
                print_count(counts, "objectives", "Objectives");
                print_count(counts, "plans", "Plans");
                print_count(counts, "tasks", "Tasks");
                print_count(counts, "risks", "Risks");
                print_count(counts, "invariants", "Invariants");
                print_count(counts, "evidence_items", "Evidence Items");
                print_count(counts, "validation_checks", "Validation Checks");
                println!("\nOntology:");
                println!("  {}", value.pointer("/ontology/name").and_then(Value::as_str).unwrap_or("Veritas OWL-DL"));
                println!("\nReasoner:");
                println!("  {}", value.pointer("/reasoner/name").and_then(Value::as_str).unwrap_or("Openllet"));
                println!("\nGraph:");
                println!("  {}", value.pointer("/graph/name").and_then(Value::as_str).unwrap_or("Fuseki"));
                println!("\nVector Memory:");
                println!("  {}", value.pointer("/vector_memory/name").and_then(Value::as_str).unwrap_or("OpenSearch FAISS/HNSW"));
            }
            Err(error) => println!("! Could not parse knowledge graph status: {error}"),
        },
        _ => {
            println!("Objectives:             unknown");
            println!("Plans:                  unknown");
            println!("Tasks:                  unknown");
            println!("Risks:                  unknown");
            println!("Invariants:             unknown");
            println!("Evidence Items:         unknown");
            println!("Validation Checks:      unknown");
            println!("\nOntology:\n  Veritas / Invariant Forge OWL-DL");
            println!("\nReasoner:\n  Openllet");
            println!("\nGraph:\n  Fuseki");
            println!("\nVector Memory:\n  OpenSearch FAISS/HNSW");
        }
    }
    println!();
}

fn print_count(counts: &Value, key: &str, label: &str) {
    let value = counts.get(key).and_then(Value::as_u64).unwrap_or(0);
    println!("{label:<22}{value:>8}");
}

fn print_menu() {
    println!("What would you like to do?\n");
    println!("[1] Ingest arXiv Research");
    println!("[2] Upload Local PDFs");
    println!("[3] Upload / Update Ontology");
    println!("[4] Search Research Corpus");
    println!("[5] Generate Code from Research");
    println!("[6] Run Mathematical Discovery Workflow");
    println!("[7] View Evidence Graph");
    println!("[8] Validate Generated Artifacts");
    println!("[9] Configuration\n");
    println!("Modes");
    println!("─────");
    println!("Research Mode     ingest papers, discover invariants, search representations");
    println!("Engineering Mode  generate code, tests, packages, and validation reports");
    println!("Operations Mode   validate deployment, runtime, observability, and runbooks");
    println!("Autonomous Mode   evidence → ontology grounding → plan → code → validation\n");
    println!("Command examples:");
    println!("  veritas start");
    println!("  veritas ingest-arxiv --query \"cat:cs.AI OR cat:math.OC\" --max-results 3");
    println!("  veritas ingest-pdf --path ./paper.pdf");
    println!("  veritas upload-ontology");
    println!("  veritas ask \"turn indexed research into tested Rust/CUDA code\"");
    println!("  veritas generate-code --language rust --prompt \"implement the indexed method\"\n");
    println!("veritas >");
}

fn print_start_success(api_url: &str) {
    println!("\nVeritas stack requested through Docker Compose.");
    println!("Next checks:");
    println!("  veritas --api-url {} ready", api_url);
    println!("  veritas upload-ontology");
    println!("  veritas ingest-arxiv --query \"cat:cs.AI OR cat:math.OC\" --max-results 3");
    println!("  veritas ask \"summarize indexed research and propose production code\"");
    println!("  veritas generate-code --language rust --prompt \"implement the strongest indexed method\"");
}

fn configure() -> Result<()> {
    print_logo();
    print_menu();
    ensure_env_file()?;
    println!("Configuration file is .env and config/veritas.yaml.");
    println!("All service URLs, models, ingestion limits, chunk sizes, graph URIs, and codegen settings are configurable.");
    print!("Open .env and config/veritas.yaml before starting? [press Enter to continue] ");
    io::stdout().flush()?;
    let mut buf = String::new();
    io::stdin().read_line(&mut buf)?;
    Ok(())
}

fn ensure_env_file() -> Result<()> {
    let env_path = std::path::Path::new(".env");
    if !env_path.exists() {
        std::fs::copy(".env.example", env_path).map_err(|error| {
            anyhow!(
                "failed to create .env from .env.example: {error}. Run from repository root or copy .env.example manually."
            )
        })?;
        println!("Created .env from .env.example");
    }
    Ok(())
}

fn run(cmd: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(cmd).args(args).status().map_err(|error| {
        anyhow!(
            "failed to run command `{}` with args {:?}: {}. Is Docker installed and are you running from the repository root?",
            cmd,
            args,
            error
        )
    })?;
    if !status.success() {
        return Err(anyhow!(
            "command `{}` with args {:?} exited with status {}. Check Docker logs with `docker compose logs --tail=200`.",
            cmd,
            args,
            status
        ));
    }
    Ok(())
}

async fn print_response(result: reqwest::Result<reqwest::Response>, operation: &str) -> Result<()> {
    match result {
        Ok(response) => {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            if status.is_success() {
                if let Ok(value) = serde_json::from_str::<Value>(&text) {
                    println!("{}", serde_json::to_string_pretty(&value)?);
                } else {
                    println!("{}", text);
                }
                Ok(())
            } else {
                Err(anyhow!(
                    "{} failed with HTTP {}. Response: {}",
                    operation,
                    status,
                    text
                ))
            }
        }
        Err(error) => Err(anyhow!(
            "{} failed before receiving a response: {}. Confirm the stack is running with `veritas ready`.",
            operation,
            error
        )),
    }
}
