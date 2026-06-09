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

const TAGLINE: &str =
    "Math heavy evidence backed research and development software engineering agent.";

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
        Commands::Welcome => {
            print_welcome();
            Ok(())
        }
        Commands::Configure => configure(),
        Commands::Start { gpu } => {
            print_welcome();
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

fn print_welcome() {
    println!("{}", VERITAS_LOGO);
    println!("{}\n", TAGLINE);
    println!("Start:        veritas start");
    println!("GPU start:    veritas start --gpu");
    println!("Health:       veritas ready");
    println!("Upload OWL:   veritas upload-ontology");
    println!("Ingest arXiv: veritas ingest-arxiv --query \"cat:cs.AI OR cat:math.OC\" --max-results 3");
    println!("Upload PDF:   veritas ingest-pdf --path ./paper.pdf");
    println!("Ask:          veritas ask \"turn this formula into tested Rust and CUDA code\"");
    println!("Generate:     veritas generate-code --language rust --prompt \"implement the indexed method\"\n");
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
    print_welcome();
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
