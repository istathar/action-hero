use anyhow::Result;
use clap::{Arg, ArgAction, Command};
use reqwest::header::{HeaderMap, HeaderValue};
use serde_json::Value;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tracing::debug;
use tracing_subscriber;

const VERSION: &str = concat!("v", env!("CARGO_PKG_VERSION"));

/// A struct holding the configuration being used to retrieve information from
/// GitHub's API.
struct API {
    client: reqwest::Client,
    owner: String,
    repository: String,
    workflow: String,
}

async fn retrieve_workflow_runs(api: &API) -> Result<Vec<String>> {
    // use token to retrieve runs for the given workflow from GitHub API

    let url = format!(
        "https://api.github.com/repos/{}/{}/actions/workflows/{}/runs?per_page=10&page=1",
        api.owner, api.repository, api.workflow
    );
    debug!(?url);

    let response = api
        .client
        .get(&url)
        .send()
        .await?;

    // retrieve the run ID of the most recent 10 runs
    let body: Value = response
        .json()
        .await?;

    let runs: Vec<String> = body["workflow_runs"]
        .as_array()
        .expect("Expected workflow_runs to be an array")
        .iter()
        .take(10)
        .map(|workflow_run| {
            workflow_run["id"]
                .as_i64()
                .expect("Expected run ID to be present and non-empty")
                .to_string()
        })
        .collect();

    Ok(runs)
}

async fn retrieve_run_jobs(api: &API, run_id: &str) -> Result<Vec<Value>> {
    let url = format!(
        "https://api.github.com/repos/{}/{}/actions/runs/{}/jobs",
        api.owner, api.repository, run_id
    );

    debug!(?url);

    let response = api
        .client
        .get(url)
        .send()
        .await?;

    let body = response
        .json::<serde_json::Value>()
        .await?;

    let jobs: Vec<Value> = body["jobs"]
        .as_array()
        .expect("Expected jobs to be an array")
        .to_vec();

    Ok(jobs)
}

fn display_job_steps(jobs: &Vec<serde_json::Value>) {
    for job in jobs {
        let job_name = job["name"]
            .as_str()
            .unwrap();

        println!("{}", job_name);

        let steps = job["steps"]
            .as_array()
            .expect("Expected steps to be an array");

        for step in steps {
            let step_name = step["name"]
                .as_str()
                .unwrap();
            let step_status = step["status"]
                .as_str()
                .unwrap();
            let step_start = step["started_at"]
                .as_str()
                .unwrap();
            let step_finish = step["completed_at"]
                .as_str()
                .unwrap();

            // convert start and stop times to a suitable DateTime type

            let step_start = OffsetDateTime::parse(step_start, &Rfc3339).unwrap();
            let step_finish = OffsetDateTime::parse(step_finish, &Rfc3339).unwrap();

            let step_duration = step_finish - step_start;

            println!("    {}: {}, {}", step_name, step_status, step_duration);
        }
    }
}

fn setup_api_client() -> Result<reqwest::Client> {
    // get GITHUB_TOKEN value from environment variable
    let token = std::env::var("GITHUB_TOKEN").expect("GITHUB_TOKEN environment variable not set");

    // Initialize a request Client as we will be making many requests of
    // the GitHub API.
    let mut headers = HeaderMap::new();

    let mut auth: HeaderValue = format!("Bearer {}", token)
        .parse()
        .unwrap();
    auth.set_sensitive(true);
    headers.insert("Authorization", auth);

    headers.insert(
        "Accept",
        "application/vnd.github+json"
            .parse()
            .unwrap(),
    );

    headers.insert(
        "User-Agent",
        format!("action-hero/{}", VERSION)
            .parse()
            .unwrap(),
    );

    headers.insert(
        "X-GitHub-Api-Version",
        "2022-11-28"
            .parse()
            .unwrap(),
    );
    let client = reqwest::Client::builder()
        .default_headers(headers)
        .build()?;

    Ok(client)
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize the tracing subscriber
    tracing_subscriber::fmt::init();

    let matches = Command::new("hero")
            .version(VERSION)
            .propagate_version(true)
            .author("Andrew Cowie")
            .about("Retrieve workflow and run from GitHub Actions and send to OpenTelemetry as spans and traces.")
            .disable_help_subcommand(true)
            .disable_help_flag(true)
            .disable_version_flag(true)
            .arg(
                Arg::new("help")
                    .long("help")
                    .long_help("Print help")
                    .global(true)
                    .hide(true)
                    .action(ArgAction::Help))
            .arg(
                Arg::new("version")
                    .long("version")
                    .long_help("Print version")
                    .global(true)
                    .hide(true)
                    .action(ArgAction::Version))
            .arg(
                Arg::new("repository")
                    .action(ArgAction::Set)
                    .required(true)
                    .help("Name of the GitHub organization and repository to retrieve workflows from. This must be specified in the form \"owner/repo\""))
            .arg(
                Arg::new("workflow")
                    .action(ArgAction::Set)
                    .required(true)
                    .help("Name of the GitHub Actions workflow to present as a trace. This is typically a filename such as \"check.yaml\""))
            .get_matches();

    let repository = matches
        .get_one::<String>("repository")
        .unwrap()
        .to_string();

    debug!(repository);

    let (owner, repository) = repository
        .split_once('/')
        .expect("Repository must be specified in the form \"owner/repo\"");
    let owner = owner.to_owned();
    let repository = repository.to_owned();

    let workflow = matches
        .get_one::<String>("workflow")
        .unwrap()
        .to_string();

    debug!(workflow);

    let client = setup_api_client()?;

    let api = API {
        client,
        owner,
        repository,
        workflow,
    };

    let runs: Vec<String> = retrieve_workflow_runs(&api).await?;

    println!("runs: {:#?}", runs);

    let run_id: &str = runs
        .first()
        .unwrap()
        .as_ref();

    debug!(run_id);

    let jobs: Vec<Value> = retrieve_run_jobs(&api, &run_id).await?;

    display_job_steps(&jobs);

    Ok(())
}
