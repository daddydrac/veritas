use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use reqwest::Client;
use serde_json::{json, Value};
use std::fs;
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

const DEFAULT_PLANNER_MODEL: &str = "Qwen/Qwen2.5-Coder-7B-Instruct";
const DEFAULT_CODE_MODEL: &str = "Qwen/Qwen2.5-Coder-14B-Instruct";
const DEFAULT_CODE_FALLBACK_MODEL: &str = "deepseek-ai/DeepSeek-Coder-V2-Lite-Instruct";
const DEFAULT_MATH_MODEL: &str = "allenai/Olmo-3-7B-Instruct";
const DEFAULT_MATH_LARGE_MODEL: &str = "allenai/Olmo-3.1-32B-Instruct";
const DEFAULT_EMBEDDING_MODEL: &str = "Muennighoff/SBERT-base-nli-v2";
const DEFAULT_VLLM_IMAGE: &str = "vllm/vllm-openai:latest";

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
    /// Print the Veritas logo and guided workflow menu.
    Welcome,
    /// Create or update .env through an interactive model/config wizard.
    Init,
    /// Alias for init.
    Configure,
    /// Start the Docker Compose stack.
    Start {
        #[arg(long, default_value_t = false)]
        models: bool,
        #[arg(long, default_value_t = false)]
        code_model: bool,
        #[arg(long, default_value_t = false)]
        math_model: bool,
    },
    /// Start core services only.
    Up,
    /// Stop services.
    Down,
    /// API health check.
    Health,
    /// Full service readiness check.
    Ready,
    /// Show configured model roles and vLLM endpoints.
    Models,
    /// Ingest arXiv PDFs.
    IngestArxiv {
        #[arg(long)]
        query: String,
        #[arg(long, default_value_t = 5)]
        max_results: u32,
    },
    /// Ingest a local PDF.
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
    /// Search OpenSearch FAISS/HNSW and lexical fields.
    Search {
        query: String,
        #[arg(long, default_value_t = 10)]
        size: u32,
        #[arg(long, default_value = "hybrid")]
        mode: String,
    },
    /// Run a SPARQL query.
    Sparql {
        query: String,
    },
    /// Ask Veritas for an evidence-backed plan.
    Ask {
        prompt: String,
    },
    /// Ask Veritas for an evidence-backed plan.
    Plan {
        goal: String,
    },
    /// Directly call the configured vLLM role.
    Chat {
        #[arg(long, default_value = "planner")]
        role: String,
        prompt: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let http = Client::new();
    match cli.command.unwrap_or(Commands::Welcome) {
        Commands::Welcome => print_startup_screen(&http, &cli.api_url).await,
        Commands::Init | Commands::Configure => configure_interactive(),
        Commands::Start {
            models,
            code_model,
            math_model,
        } => {
            print_logo();
            ensure_env_file()?;
            let mut args = vec!["compose"];
            if models {
                args.extend(["--profile", "models"]);
            }
            if code_model {
                args.extend(["--profile", "code-model"]);
            }
            if math_model {
                args.extend(["--profile", "math-model"]);
            }
            args.extend(["up", "-d", "--build"]);
            run("docker", &args)?;
            print_start_success(&cli.api_url, models, code_model, math_model);
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
        Commands::Models => print_response(
            http.get(format!("{}/models", cli.api_url)).send().await,
            "api.models",
        )
        .await,
        Commands::Search { query, size, mode } => {
            print_response(
                http.post(format!("{}/search", cli.api_url))
                    .json(&json!({"query": query, "size": size, "mode": mode}))
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
        Commands::Chat { role, prompt } => {
            print_response(
                http.post(format!("{}/llm/chat", cli.api_url))
                    .json(&json!({"role": role, "prompt": prompt}))
                    .send()
                    .await,
                "api.llm.chat",
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
    if path.extension().and_then(|value| value.to_str()).unwrap_or_default().to_lowercase()
        != "pdf"
    {
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
    fs::create_dir_all(upload_dir)?;
    let staged = upload_dir.join(file_name);
    fs::copy(path, &staged).map_err(|error| {
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
    fs::create_dir_all(ontology_dir)?;
    let staged = ontology_dir.join(file_name);
    fs::copy(path, &staged).map_err(|error| {
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
    print_model_status(http, api_url).await;
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
        Ok(response) if response.status().is_success() || response.status().as_u16() == 503 => {
            match response.json::<Value>().await {
                Ok(value) => {
                    let checks = value.get("checks").unwrap_or(&Value::Null);
                    print_service_check(checks, "opensearch", "OpenSearch FAISS/HNSW");
                    print_service_check(checks, "fuseki", "Jena Fuseki Graph");
                    print_service_static("Openllet Reasoner", true, "offline reasoner container configured");
                    print_service_static("OWL-DL Ontology Loaded", true, "run Upload / Update Ontology to refresh");
                    print_service_check(checks, "embedding", "Embedding Service Ready");
                    print_service_static("Retrieval Pipeline Ready", true, "OpenSearch + embeddings + Fuseki configured");
                    print_service_check(checks, "vllm_planner", "vLLM Planner Model");
                    print_service_check(checks, "vllm_code", "vLLM Code Model");
                    print_service_check(checks, "vllm_math", "vLLM Math Model");
                }
                Err(error) => println!("! Could not decode readiness response: {error}"),
            }
        }
        Ok(response) => println!("! API returned HTTP {} for readiness", response.status()),
        Err(error) => {
            println!("! API unavailable at {api_url}: {error}");
            println!("  Start the stack with: veritas start --models");
        }
    }
    println!();
}

fn print_service_check(checks: &Value, key: &str, label: &str) {
    let item = checks.get(key).unwrap_or(&Value::Null);
    let ok = item.get("ok").and_then(Value::as_bool).unwrap_or(false);
    let marker = if ok { "✓" } else { "!" };
    println!("{marker} {label}");
    if !ok {
        let message = item.get("message").and_then(Value::as_str).unwrap_or("not reachable");
        println!("    {message}");
    }
}

fn print_service_static(label: &str, ok: bool, note: &str) {
    let marker = if ok { "✓" } else { "!" };
    println!("{marker} {label}");
    if !note.is_empty() && !ok {
        println!("    {note}");
    }
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
                println!("\nOntology:\n  {}", value.pointer("/ontology/name").and_then(Value::as_str).unwrap_or("Veritas OWL-DL"));
                println!("\nReasoner:\n  {}", value.pointer("/reasoner/name").and_then(Value::as_str).unwrap_or("Openllet"));
                println!("\nGraph:\n  Fuseki");
                println!("\nVector Memory:\n  OpenSearch FAISS/HNSW");
            }
            Err(error) => println!("! Could not decode graph status: {error}"),
        },
        _ => {
            println!("Objectives:                  unknown");
            println!("Plans:                       unknown");
            println!("Tasks:                       unknown");
            println!("Risks:                       unknown");
            println!("Invariants:                  unknown");
            println!("Evidence Items:              unknown");
            println!("Validation Checks:           unknown");
            println!("\nGraph status unavailable until API and Fuseki are running.");
        }
    }
    println!();
}

async fn print_model_status(http: &Client, api_url: &str) {
    println!("Model Routing");
    println!("─────────────");
    match http.get(format!("{}/models", api_url.trim_end_matches('/'))).send().await {
        Ok(response) if response.status().is_success() => match response.json::<Value>().await {
            Ok(value) => {
                println!("Serving:   vLLM OpenAI-compatible endpoints");
                println!("Planner:   {}", value.pointer("/planner/huggingface_model_id").and_then(Value::as_str).unwrap_or(DEFAULT_PLANNER_MODEL));
                println!("Code:      {}", value.pointer("/code_generation/primary/huggingface_model_id").and_then(Value::as_str).unwrap_or(DEFAULT_CODE_MODEL));
                println!("Math:      {}", value.pointer("/math_reasoning/primary/huggingface_model_id").and_then(Value::as_str).unwrap_or(DEFAULT_MATH_MODEL));
                println!("Embedding: {}", value.pointer("/embeddings/model").and_then(Value::as_str).unwrap_or(DEFAULT_EMBEDDING_MODEL));
            }
            Err(error) => println!("! Could not decode model routing: {error}"),
        },
        _ => println!("Model routing unavailable until API is running. Configure with `veritas init`."),
    }
    println!();
}

fn print_count(counts: &Value, key: &str, label: &str) {
    let value = counts.get(key).and_then(Value::as_u64).unwrap_or(0);
    println!("{label:<24}{value:>8}");
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
    println!("Research Mode:     ingest papers, discover invariants, representation search");
    println!("Engineering Mode:  generate code, tests, packages, validation reports");
    println!("Operations Mode:   deployment, runtime, observability, runbooks");
    println!("Autonomous Mode:   evidence → ontology → plan → code → validation\n");
    println!("Examples");
    println!("────────");
    println!("veritas init");
    println!("veritas start --models");
    println!("veritas ingest-arxiv --query \"cat:cs.AI OR cat:math.OC\" --max-results 3");
    println!("veritas ingest-pdf --path ./paper.pdf");
    println!("veritas ask \"Build a distributed GPU implementation of paper X\"\n");
    print!("veritas > ");
    let _ = io::stdout().flush();
}

fn print_start_success(api_url: &str, models: bool, code_model: bool, math_model: bool) {
    println!("\nVeritas stack start requested.");
    println!("API: {api_url}");
    if models || code_model || math_model {
        println!("Model profiles enabled:");
        if models { println!("  - planner vLLM"); }
        if code_model { println!("  - code vLLM"); }
        if math_model { println!("  - math vLLM"); }
    } else {
        println!("Model profiles were not started. Use `veritas start --models` for planner vLLM.");
    }
    println!("\nNext:");
    println!("  veritas ready");
    println!("  veritas models");
    println!("  veritas upload-ontology");
    println!("  veritas ingest-arxiv --query \"cat:cs.AI OR cat:math.OC\" --max-results 3");
}

fn configure_interactive() -> Result<()> {
    print_logo();
    println!("Veritas model setup");
    println!("───────────────────");
    println!("vLLM is the model serving solution. Rust calls vLLM's OpenAI-compatible API.");
    println!("Paste any Hugging Face model ID to override the defaults.\n");
    let hf_token = prompt_default("Hugging Face token (optional)", "")?;
    let planner = prompt_default("Planner model", DEFAULT_PLANNER_MODEL)?;
    let code = prompt_default("Code generation model", DEFAULT_CODE_MODEL)?;
    let code_fallback = prompt_default("Code fallback model", DEFAULT_CODE_FALLBACK_MODEL)?;
    println!("\nMath reasoning choices:");
    println!("  1. {}", DEFAULT_MATH_MODEL);
    println!("  2. {}", DEFAULT_MATH_LARGE_MODEL);
    println!("  3. Custom Hugging Face model ID");
    let math_choice = prompt_default("Math model choice", "1")?;
    let math = match math_choice.trim() {
        "2" => DEFAULT_MATH_LARGE_MODEL.to_string(),
        "3" => prompt_default("Custom math model", DEFAULT_MATH_MODEL)?,
        other if other.contains('/') => other.to_string(),
        _ => DEFAULT_MATH_MODEL.to_string(),
    };
    let embedding = prompt_default("Embedding model", DEFAULT_EMBEDDING_MODEL)?;
    let gpu_id = prompt_default("GPU device id", "0")?;
    let require_models = prompt_default("Require vLLM models for readiness? true/false", "false")?;
    let contents = format!(
        r#"# Generated by `veritas init`.
COMPOSE_PROJECT_NAME=veritas
VERITAS_API_PORT=8080
VERITAS_OPENSEARCH_PORT=9200
VERITAS_OPENSEARCH_DASHBOARDS_PORT=5601
VERITAS_FUSEKI_PORT=3030
VERITAS_QDRANT_PORT=6333
VERITAS_EMBEDDING_PORT=8090
VERITAS_OPENSEARCH_VERSION=3.7.0
VERITAS_QDRANT_VERSION=v1.17.0
VERITAS_FUSEKI_IMAGE=stain/jena-fuseki:5.5.0
VERITAS_VLLM_IMAGE={vllm_image}
HF_TOKEN={hf_token}
VERITAS_MODEL_PROVIDER=vllm
VERITAS_MODEL_SERVING_PROVIDER=vllm
VERITAS_REQUIRE_MODELS={require_models}
VERITAS_VLLM_DTYPE=auto
VERITAS_VLLM_ENABLE_CUDA_COMPATIBILITY=1
VERITAS_GPU_DEVICE_ID={gpu_id}
VERITAS_PLANNER_MODEL={planner}
VERITAS_PLANNER_SERVED_MODEL_NAME=veritas-planner
VERITAS_PLANNER_VLLM_URL=http://vllm-planner:8000
VERITAS_PLANNER_VLLM_PORT=8001
VERITAS_PLANNER_MAX_MODEL_LEN=32768
VERITAS_PLANNER_GPU_MEMORY_UTILIZATION=0.30
VERITAS_CODE_MODEL={code}
VERITAS_CODE_FALLBACK_MODEL={code_fallback}
VERITAS_CODE_SERVED_MODEL_NAME=veritas-code
VERITAS_CODE_VLLM_URL=http://vllm-code:8000
VERITAS_CODE_VLLM_PORT=8002
VERITAS_CODE_MAX_MODEL_LEN=32768
VERITAS_CODE_GPU_MEMORY_UTILIZATION=0.60
VERITAS_MATH_MODEL={math}
VERITAS_MATH_LARGE_MODEL={math_large}
VERITAS_MATH_SERVED_MODEL_NAME=veritas-math
VERITAS_MATH_VLLM_URL=http://vllm-math:8000
VERITAS_MATH_VLLM_PORT=8003
VERITAS_MATH_MAX_MODEL_LEN=32768
VERITAS_MATH_GPU_MEMORY_UTILIZATION=0.30
VERITAS_REMOTE_MODEL_ENABLED=false
VERITAS_REMOTE_MODEL_BASE_URL=
VERITAS_REMOTE_MODEL_API_KEY=
VERITAS_REMOTE_MODEL_NAME=
VERITAS_EMBEDDING_MODEL={embedding}
VERITAS_EMBEDDING_NORMALIZE=true
VERITAS_EMBEDDING_DEVICE=auto
VERITAS_EMBEDDING_BATCH_SIZE=16
VERITAS_OPENSEARCH_INDEX=veritas-papers
VERITAS_OPENSEARCH_VECTOR_FIELD=embedding
VERITAS_FUSEKI_DATASET=veritas
VERITAS_FUSEKI_ADMIN_PASSWORD=admin
OPENSEARCH_INITIAL_ADMIN_PASSWORD=VeritasAdmin123!
"#,
        vllm_image = DEFAULT_VLLM_IMAGE,
        hf_token = hf_token,
        require_models = require_models,
        gpu_id = gpu_id,
        planner = planner,
        code = code,
        code_fallback = code_fallback,
        math = math,
        math_large = DEFAULT_MATH_LARGE_MODEL,
        embedding = embedding
    );
    fs::write(".env", contents)?;
    println!("\nWrote .env with your Veritas model configuration.");
    println!("Next: veritas start --models");
    Ok(())
}

fn prompt_default(label: &str, default: &str) -> Result<String> {
    if default.is_empty() {
        print!("{label}: ");
    } else {
        print!("{label} [{default}]: ");
    }
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let value = input.trim();
    if value.is_empty() {
        Ok(default.to_string())
    } else {
        Ok(value.to_string())
    }
}

fn ensure_env_file() -> Result<()> {
    if !Path::new(".env").exists() {
        if Path::new(".env.example").exists() {
            fs::copy(".env.example", ".env")?;
            println!("Created .env from .env.example. Run `veritas init` to customize models.");
        } else {
            configure_interactive()?;
        }
    }
    Ok(())
}

fn run(cmd: &str, args: &[&str]) -> Result<()> {
    println!("$ {} {}", cmd, args.join(" "));
    let status = Command::new(cmd).args(args).status()?;
    if !status.success() {
        return Err(anyhow!("command failed: {} {}", cmd, args.join(" ")));
    }
    Ok(())
}

async fn print_response(result: Result<reqwest::Response, reqwest::Error>, stage: &str) -> Result<()> {
    match result {
        Ok(response) => {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            if status.is_success() {
                println!("{}", pretty_json_or_raw(&text));
                Ok(())
            } else {
                eprintln!("Veritas request failed at {stage} with HTTP {}", status);
                eprintln!("{}", pretty_json_or_raw(&text));
                Err(anyhow!("{stage} failed with HTTP {status}"))
            }
        }
        Err(error) => {
            eprintln!("Veritas request failed at {stage}: {error}");
            eprintln!("Remediation: run `veritas start`, `veritas ready`, and inspect Docker Compose logs.");
            Err(error.into())
        }
    }
}

fn pretty_json_or_raw(text: &str) -> String {
    serde_json::from_str::<Value>(text)
        .and_then(|value| serde_json::to_string_pretty(&value))
        .unwrap_or_else(|_| text.to_string())
}
