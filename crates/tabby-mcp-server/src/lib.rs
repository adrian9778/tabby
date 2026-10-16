//! Exposes every repository configured in `config.toml` as its own MCP server,
//! all sharing one HTTP port and distinguished by the URL sub-path
//! `/mcp/{repository}`.
//!
//! When more than one repository is configured, `/mcp/all` is mounted as well:
//! the same tools, but fanned out over every repository at once.
//!
//! Requires `TABBY_MCP_TOKEN`; the endpoints are not mounted when it is absent,
//! because Tabby binds to `0.0.0.0` by default.

use std::{
    cmp::Ordering,
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use axum::{
    extract::{Request, State},
    http::{header::AUTHORIZATION, HeaderMap, StatusCode},
    middleware::{from_fn_with_state, Next},
    response::{IntoResponse, Response},
    Extension, Router,
};
use futures::StreamExt;
use rust_mcp_axum::routes::streamable_http_routes;
use rust_mcp_sdk::{
    id_generator::{FastIdGenerator, UuidGenerator},
    mcp_http::{McpAppState, McpHttpHandler},
    mcp_server::ServerHandler,
    schema::{
        CallToolError, CallToolRequestParams, CallToolResult, Implementation, InitializeResult,
        ListToolsResult, PaginatedRequestParams, ProtocolVersion, RpcError, ServerCapabilities,
        ServerCapabilitiesTools, TextContent, Tool, ToolInputSchema,
    },
    session_store::InMemorySessionStore,
    McpServer, ToMcpServerHandler,
};
use serde_json::{json, Value};
use tabby_common::{
    api::code::{CodeSearch, CodeSearchError, CodeSearchHit, CodeSearchParams, CodeSearchQuery},
    config::config_index_to_id,
};
use tabby_git::GrepTextOrBase64;
use tracing::warn;

const AGGREGATE_SLUG: &str = "all";

const MAX_GREP_FILES: usize = 15;
const MAX_GREP_LINES_PER_FILE: usize = 12;
const MAX_RESPONSE_BYTES: usize = 24_000;
const MAX_FILE_BYTES: usize = 64_000;
const DEFAULT_PATH_LIMIT: usize = 40;
const MAX_PATH_LIMIT: usize = 200;
const DEFAULT_FILE_LINES: usize = 400;
const MAX_FILE_LINES: usize = 2_000;
const MAX_SNIPPET_BYTES: usize = 1_200;
const DEFAULT_SEMANTIC_LIMIT: usize = 10;
const MAX_SEMANTIC_LIMIT: usize = 20;

/// Builds the `/mcp/{repository}` routers for every configured repository, plus
/// the aggregate `/mcp/all` router.
///
/// `code` is Tabby's embedding + BM25 search service; `semantic_search` is only
/// advertised when it is present.
pub fn routes(config: &tabby_common::config::Config, code: Option<Arc<dyn CodeSearch>>) -> Router {
    let Ok(token) = std::env::var("TABBY_MCP_TOKEN") else {
        warn!("TABBY_MCP_TOKEN is unset: MCP code search endpoints are disabled");
        return Router::new();
    };
    let token = Arc::new(token);

    let mut router = Router::new();
    let mut repos: Vec<Repo> = Vec::new();
    let mut slugs: Vec<String> = Vec::new();

    for (index, repository) in config.repositories.iter().enumerate() {
        let slug = url_slug(&repository.display_name());
        if slugs.contains(&slug) {
            warn!("Skipped duplicated MCP endpoint name `{slug}`");
            continue;
        }
        slugs.push(slug.clone());

        // `config_index_to_id` is the id the hourly `scheduler_git` job indexes this
        // repository under, so it must be derived from the same config position.
        let repo = Repo {
            name: repository.display_name(),
            root: repository.dir(),
            source_id: config_index_to_id(index),
        };

        router = router.merge(endpoint(
            &format!("/mcp/{slug}"),
            vec![repo.clone()],
            code.clone(),
            token.clone(),
        ));
        repos.push(repo);
    }

    if repos.len() > 1 && !slugs.iter().any(|slug| slug == AGGREGATE_SLUG) {
        router = router.merge(endpoint(
            &format!("/mcp/{AGGREGATE_SLUG}"),
            repos,
            code,
            token,
        ));
    }

    router
}

fn endpoint(
    path: &str,
    repos: Vec<Repo>,
    code: Option<Arc<dyn CodeSearch>>,
    token: Arc<String>,
) -> Router {
    let state = Arc::new(McpAppState {
        session_store: Arc::new(InMemorySessionStore::new()),
        id_generator: Arc::new(UuidGenerator {}),
        stream_id_gen: Arc::new(FastIdGenerator::new(Some("s_"))),
        server_details: Arc::new(server_details(&repos, code.is_some())),
        handler: SearchHandler { repos, code }.to_mcp_server_handler(),
        ping_interval: Duration::from_secs(12),
        transport_options: Default::default(),
        // Keep the initialize response as an open SSE stream: with a single JSON
        // response the session transport ends early and `notifications/initialized`
        // fails with 422 for real MCP clients.
        enable_json_response: false,
        event_store: None,
        task_store: None,
        client_task_store: None,
        message_observer: None,
    });

    Router::new()
        .merge(streamable_http_routes::routes(path))
        .with_state(state)
        .layer(Extension(Arc::new(McpHttpHandler::new(None, vec![], None))))
        .layer(from_fn_with_state(token, require_token))
}

async fn require_token(
    State(token): State<Arc<String>>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Response {
    let expected = format!("Bearer {token}");
    let authorized = headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == expected);

    if authorized {
        next.run(request).await
    } else {
        (StatusCode::UNAUTHORIZED, "Missing or invalid bearer token").into_response()
    }
}

fn server_details(repos: &[Repo], semantic: bool) -> InitializeResult {
    let (name, title, instructions) = match repos {
        [repo] => (
            repo.name.clone(),
            format!("Tabby code search: {}", repo.name),
            format!(
                "Search the `{}` repository only. Use search_files to locate paths, grep to find code, read_file to fetch a line window{}.",
                repo.name,
                if semantic { ", and semantic_search for natural-language questions" } else { "" }
            ),
        ),
        repos => {
            let listed = repo_names(repos);
            (
                AGGREGATE_SLUG.to_string(),
                format!("Tabby code search: {} repositories", repos.len()),
                format!(
                    "Search all configured repositories at once ({listed}). Every result line is prefixed with its repository name; read_file additionally takes a `repository` argument."
                ),
            )
        }
    };

    InitializeResult {
        server_info: Implementation {
            name: format!("tabby-{name}"),
            version: env!("CARGO_PKG_VERSION").into(),
            title: Some(title),
            description: None,
            icons: vec![],
            website_url: None,
        },
        capabilities: ServerCapabilities {
            tools: Some(ServerCapabilitiesTools { list_changed: None }),
            ..Default::default()
        },
        meta: None,
        instructions: Some(instructions),
        protocol_version: ProtocolVersion::V2025_11_25.into(),
    }
}

fn url_slug(name: &str) -> String {
    name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[derive(Clone)]
struct Repo {
    name: String,
    root: PathBuf,
    /// The id the code index stores this repository's chunks under.
    source_id: String,
}

impl Repo {
    fn describe(&self) -> String {
        format!("{} (source_id = {})", self.name, self.source_id)
    }
}

fn repo_names(repos: &[Repo]) -> String {
    repos
        .iter()
        .map(|repo| repo.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

struct SearchHandler {
    repos: Vec<Repo>,
    code: Option<Arc<dyn CodeSearch>>,
}

impl SearchHandler {
    fn is_aggregate(&self) -> bool {
        self.repos.len() > 1
    }

    fn describe_repos(&self) -> String {
        self.repos
            .iter()
            .map(Repo::describe)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[async_trait]
impl ServerHandler for SearchHandler {
    async fn handle_list_tools_request(
        &self,
        _params: Option<PaginatedRequestParams>,
        _runtime: Arc<dyn McpServer>,
    ) -> Result<ListToolsResult, RpcError> {
        let aggregate = self.is_aggregate();
        let mut tools = vec![
            tool(
                "search_files",
                if aggregate {
                    "Fuzzy-match file and directory paths across every configured repository. Returns up to `limit` paths, each prefixed with its repository name."
                } else {
                    "Fuzzy-match file and directory paths in the repository. Returns up to `limit` paths ranked by relevance."
                },
                json!({
                    "pattern": { "type": "string", "description": "Fuzzy pattern matched against paths, e.g. `cfgload`." },
                    "limit": { "type": "integer", "description": format!("Maximum paths to return. Defaults to {DEFAULT_PATH_LIMIT}.") }
                }),
                &["pattern"],
            ),
            tool(
                "grep",
                "Regex search over file contents, with a ripgrep-like syntax: bare text is a content pattern, `f:` filters by path, `lang:` filters by language, and any token can be negated with a `-` prefix. Results are capped per file and per response, so prefer narrow patterns.",
                json!({
                    "query": { "type": "string", "description": "Example: `fn load_config lang:rust -f:test`." }
                }),
                &["query"],
            ),
            tool(
                "read_file",
                "Read a line window of a file at HEAD, with `n: text` line prefixes. Defaults to the first 400 lines; use `offset` and `limit` to page instead of reading whole files.",
                read_file_schema(self.repos.len() > 1, &repo_names(&self.repos)),
                if aggregate {
                    &["path", "repository"][..]
                } else {
                    &["path"][..]
                },
            ),
        ];

        if self.code.is_some() {
            tools.push(tool(
                "semantic_search",
                if aggregate {
                    "Semantic code search over every indexed repository: embedding recall fused with BM25 via RRF, merged across repositories by score. Best for natural-language questions like \"where is rate limiting handled\"; use grep instead for exact identifiers."
                } else {
                    "Semantic code search over the indexed repository: embedding recall fused with BM25 via RRF. Best for natural-language questions like \"where is rate limiting handled\"; use grep instead for exact identifiers."
                },
                json!({
                    "query": { "type": "string", "description": "Natural-language or code query." },
                    "language": { "type": "string", "description": "Optional language filter, e.g. `rust`." },
                    "filepath": { "type": "string", "description": "Optional path filter, e.g. `crates/tabby/src/serve.rs`." },
                    "limit": { "type": "integer", "description": format!("Maximum snippets to return, max {MAX_SEMANTIC_LIMIT}. Defaults to {DEFAULT_SEMANTIC_LIMIT}.") }
                }),
                &["query"],
            ));
        }

        Ok(ListToolsResult {
            tools,
            meta: None,
            next_cursor: None,
        })
    }

    async fn handle_call_tool_request(
        &self,
        params: CallToolRequestParams,
        _runtime: Arc<dyn McpServer>,
    ) -> Result<CallToolResult, CallToolError> {
        let result = match params.name.as_str() {
            "search_files" => self.search_files(&params).await,
            "grep" => self.grep(&params).await,
            "read_file" => self.read_file(&params),
            "semantic_search" => self.semantic_search(&params).await,
            other => return Err(CallToolError::unknown_tool(other.to_string())),
        };

        Ok(match result {
            Ok(text) => call_tool_result(text, false),
            Err(err) => {
                call_tool_result(format!("{} search failed: {err}", repo_names(&self.repos)), true)
            }
        })
    }
}

impl SearchHandler {
    async fn search_files(&self, params: &CallToolRequestParams) -> anyhow::Result<String> {
        let pattern = required_str(params, "pattern")?;
        let limit = optional_usize(params, "limit")
            .unwrap_or(DEFAULT_PATH_LIMIT)
            .clamp(1, MAX_PATH_LIMIT);

        let mut ranked: Vec<(String, Vec<String>)> = Vec::new();
        for repo in &self.repos {
            let paths: Vec<String> = tabby_git::search_files(&repo.root, None, pattern, limit)
                .await?
                .iter()
                .map(|r| format!("{} ({})", r.path, r.r#type))
                .collect();
            if !paths.is_empty() {
                ranked.push((repo.name.clone(), paths));
            }
        }

        if ranked.is_empty() {
            return Ok("No matching paths.".to_string());
        }

        let mut lines = round_robin(&ranked, limit, self.is_aggregate());
        if lines.len() >= limit && ranked.iter().any(|(_, paths)| paths.len() >= limit) {
            lines.push(format!(
                "… capped at {limit} paths: raise `limit` or narrow `{pattern}`"
            ));
        }
        Ok(lines.join("\n"))
    }

    async fn grep(&self, params: &CallToolRequestParams) -> anyhow::Result<String> {
        let query = required_str(params, "query")?;
        let aggregate = self.is_aggregate();

        let mut files = Vec::new();
        let mut bytes = 0;
        let mut truncated = false;

        'repos: for repo in &self.repos {
            let prefix = if aggregate {
                format!("[{}] ", repo.name)
            } else {
                String::new()
            };
            let mut stream = Box::pin(tabby_git::grep(&repo.root, None, query).await?);

            while let Some(file) = stream.next().await {
                if files.len() >= MAX_GREP_FILES || bytes >= MAX_RESPONSE_BYTES {
                    truncated = true;
                    break 'repos;
                }

                let lines: Vec<String> = file
                    .lines
                    .iter()
                    .take(MAX_GREP_LINES_PER_FILE)
                    .filter_map(|line| match &line.line {
                        GrepTextOrBase64::Text(text) => {
                            Some(format!("{}: {}", line.line_number, text.trim_end()))
                        }
                        GrepTextOrBase64::Base64(_) => None,
                    })
                    .collect();

                bytes += lines.iter().map(|l| l.len()).sum::<usize>() + prefix.len() + 2;
                if lines.is_empty() {
                    continue;
                }
                files.push(format!("{}{}\n{}", prefix, file.path.display(), lines.join("\n")));
            }
        }

        if files.is_empty() {
            return Ok(format!("No matches for `{query}`."));
        }
        let mut output = files.join("\n\n");
        if truncated {
            output.push_str(&format!(
                "\n\n… truncated at {MAX_GREP_FILES} files / {MAX_RESPONSE_BYTES} bytes: narrow `{query}`"
            ));
        }
        Ok(output)
    }

    fn read_file(&self, params: &CallToolRequestParams) -> anyhow::Result<String> {
        let path = required_str(params, "path")?;
        let repo = self.select_repo(params)?;
        let offset = optional_usize(params, "offset").unwrap_or(1).max(1);
        let limit = optional_usize(params, "limit")
            .unwrap_or(DEFAULT_FILE_LINES)
            .clamp(1, MAX_FILE_LINES);

        let repository = git2::Repository::open(&repo.root)?;
        let commit = repository.head()?.peel_to_commit()?;
        let entry = commit.tree()?.get_path(Path::new(path))?;
        let blob = entry.to_object(&repository)?.peel_to_blob()?;

        let content = blob.content();
        let too_big = content.len() > MAX_FILE_BYTES;
        let text = String::from_utf8_lossy(&content[..content.len().min(MAX_FILE_BYTES)]);
        let lines: Vec<&str> = text.lines().collect();

        let start = (offset - 1).min(lines.len());
        let end = (start + limit).min(lines.len());

        let mut notes = vec![format!(
            "[{}] lines {}-{end} of {}, next offset={}",
            repo.name,
            start + 1,
            lines.len(),
            end + 1
        )];
        if too_big {
            notes.push(format!(
                "content cut at {MAX_FILE_BYTES} bytes: lines after that byte offset are not readable here"
            ));
        }
        if end == 0 {
            notes.push(format!("offset {offset} is past the end of the file"));
        }

        let body = lines[start..end]
            .iter()
            .enumerate()
            .map(|(i, line)| format!("{}: {}", start + i + 1, line))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(format!("{}\n\n{body}", notes.join("; ")))
    }

    async fn semantic_search(&self, params: &CallToolRequestParams) -> anyhow::Result<String> {
        let code = self.code.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "semantic search is unavailable: Tabby started without an embedding service"
            )
        })?;
        let query = required_str(params, "query")?;
        let language = optional_str(params, "language").map(str::to_owned);
        let filepath = optional_str(params, "filepath").map(str::to_owned);
        let limit = optional_usize(params, "limit")
            .unwrap_or(DEFAULT_SEMANTIC_LIMIT)
            .clamp(1, MAX_SEMANTIC_LIMIT);

        let search_params = CodeSearchParams {
            num_to_return: limit,
            num_to_score: limit * 2,
            ..Default::default()
        };

        let mut hits: Vec<(String, CodeSearchHit)> = Vec::new();
        let mut unavailable = Vec::new();

        for repo in &self.repos {
            let request = CodeSearchQuery::new(
                filepath.clone(),
                language.clone(),
                query.to_owned(),
                repo.source_id.clone(),
            );
            match code.search_in_language(request, search_params.clone()).await {
                Ok(response) => hits.extend(
                    response
                        .hits
                        .into_iter()
                        .map(|hit| (repo.name.clone(), hit)),
                ),
                Err(CodeSearchError::NotReady) => unavailable.push(repo.describe()),
                Err(other) => unavailable.push(format!("{}: {other}", repo.describe())),
            }
        }

        if hits.is_empty() {
            let mut message = format!(
                "No semantic matches for `{query}`. Searched: {}.",
                self.describe_repos()
            );
            if !unavailable.is_empty() {
                message.push_str(&format!(
                    " Not indexed yet (index not ready): {}.",
                    unavailable.join(", ")
                ));
            }
            return Ok(message);
        }

        hits.sort_by(|a, b| {
            b.1.scores
                .rrf
                .partial_cmp(&a.1.scores.rrf)
                .unwrap_or(Ordering::Equal)
        });
        hits.truncate(limit);

        let aggregate = self.is_aggregate();
        let mut out = String::new();
        if aggregate {
            out.push_str(&format!(
                "Searched {} repositories: {}.\n\n",
                self.repos.len(),
                self.describe_repos()
            ));
        }
        if !unavailable.is_empty() {
            out.push_str(&format!(
                "Skipped (index not ready): {}.\n\n",
                unavailable.join(", ")
            ));
        }
        for (name, hit) in &hits {
            let label = if aggregate {
                format!("[{name}] ")
            } else {
                String::new()
            };
            out.push_str(&format!(
                "{label}{}  rrf={:.4} bm25={:.1} embedding={:.3}  src={}\n{}\n\n",
                hit.doc.filepath,
                hit.scores.rrf,
                hit.scores.bm25,
                hit.scores.embedding,
                hit.doc.git_url,
                truncate_chars(&hit.doc.body, MAX_SNIPPET_BYTES)
            ));
        }
        Ok(out.trim_end().to_string())
    }

    fn select_repo(&self, params: &CallToolRequestParams) -> anyhow::Result<&Repo> {
        if self.repos.len() == 1 {
            return Ok(&self.repos[0]);
        }
        let name = required_str(params, "repository")?;
        self.repos
            .iter()
            .find(|repo| repo.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "unknown repository `{name}`, expected one of: {}",
                    repo_names(&self.repos)
                )
            })
    }
}

fn read_file_schema(aggregate: bool, repositories: &str) -> Value {
    let mut properties = json!({
        "path": { "type": "string", "description": "Repository-relative path, e.g. `crates/tabby/src/serve.rs`." },
        "offset": { "type": "integer", "description": "1-based first line to read. Defaults to 1." },
        "limit": { "type": "integer", "description": format!("Number of lines to read, max {MAX_FILE_LINES}. Defaults to {DEFAULT_FILE_LINES}.") }
    });
    if aggregate {
        properties.as_object_mut().unwrap().insert(
            "repository".to_string(),
            json!({ "type": "string", "description": format!("Which repository `path` belongs to: one of {repositories}.") }),
        );
    }
    properties
}

/// Merge per-repository result lists by rank so no single repository can crowd
/// the others out of the shared limit.
fn round_robin(groups: &[(String, Vec<String>)], limit: usize, label: bool) -> Vec<String> {
    let mut out = Vec::new();
    let mut rank = 0;
    loop {
        let mut advanced = false;
        for (name, items) in groups {
            if let Some(item) = items.get(rank) {
                out.push(if label {
                    format!("[{name}] {item}")
                } else {
                    item.clone()
                });
                advanced = true;
                if out.len() >= limit {
                    return out;
                }
            }
        }
        if !advanced {
            return out;
        }
        rank += 1;
    }
}

fn call_tool_result(text: String, is_error: bool) -> CallToolResult {
    CallToolResult {
        content: vec![TextContent::new(text, None, None).into()],
        is_error: is_error.then_some(true),
        meta: None,
        structured_content: None,
    }
}

fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Tool {
    let properties: BTreeMap<String, serde_json::Map<String, Value>> = properties
        .as_object()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|(name, value)| (name, value.as_object().cloned().unwrap_or_default()))
        .collect();

    Tool {
        annotations: None,
        description: Some(description.to_string()),
        execution: None,
        icons: vec![],
        input_schema: ToolInputSchema::new(
            required.iter().map(|name| name.to_string()).collect(),
            Some(properties),
            None,
        ),
        meta: None,
        name: name.to_string(),
        output_schema: None,
        title: None,
    }
}

fn required_str<'a>(params: &'a CallToolRequestParams, key: &str) -> anyhow::Result<&'a str> {
    params
        .arguments
        .as_ref()
        .and_then(|arguments| arguments.get(key))
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing string argument `{key}`"))
}

fn optional_str<'a>(params: &'a CallToolRequestParams, key: &str) -> Option<&'a str> {
    params
        .arguments
        .as_ref()
        .and_then(|arguments| arguments.get(key))
        .and_then(Value::as_str)
}

fn optional_usize(params: &CallToolRequestParams, key: &str) -> Option<usize> {
    params
        .arguments
        .as_ref()
        .and_then(|arguments| arguments.get(key))
        .and_then(Value::as_u64)
        .map(|value| value as usize)
}

/// Cut a string down to `max` bytes without splitting a utf-8 character.
fn truncate_chars(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n… truncated", &text[..end])
}
