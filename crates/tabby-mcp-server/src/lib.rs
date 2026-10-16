//! Exposes every repository configured in `config.toml` as its own MCP server,
//! all sharing one HTTP port and distinguished by the URL sub-path
//! `/mcp/{repository}`.
//!
//! Requires `TABBY_MCP_TOKEN`; the endpoints are not mounted when it is absent,
//! because Tabby binds to `0.0.0.0` by default.

use std::{
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
use tabby_common::config::RepositoryConfig;
use tabby_git::GrepTextOrBase64;
use tracing::warn;

const MAX_GREP_FILES: usize = 40;
const MAX_GREP_LINES_PER_FILE: usize = 20;
const MAX_RESPONSE_BYTES: usize = 60_000;
const MAX_FILE_BYTES: usize = 200_000;

/// Builds the `/mcp/{repository}` routers for every configured repository.
pub fn routes(config: &tabby_common::config::Config) -> Router {
    let Ok(token) = std::env::var("TABBY_MCP_TOKEN") else {
        warn!("TABBY_MCP_TOKEN is unset: MCP code search endpoints are disabled");
        return Router::new();
    };

    let token = Arc::new(token);
    let mut router = Router::new();
    let mut seen = Vec::new();

    for repository in &config.repositories {
        let slug = url_slug(&repository.display_name());
        if seen.contains(&slug) {
            warn!("Skipped duplicated MCP endpoint name `{slug}`");
            continue;
        }
        seen.push(slug.clone());
        router = router.merge(repo_routes(repository, slug, token.clone()));
    }

    router
}

fn repo_routes(repository: &RepositoryConfig, slug: String, token: Arc<String>) -> Router {
    let endpoint = format!("/mcp/{slug}");
    let state = Arc::new(McpAppState {
        session_store: Arc::new(InMemorySessionStore::new()),
        id_generator: Arc::new(UuidGenerator {}),
        stream_id_gen: Arc::new(FastIdGenerator::new(Some("s_"))),
        server_details: Arc::new(server_details(&repository.display_name())),
        handler: SearchHandler {
            repository: repository.display_name(),
            root: repository.dir(),
        }
        .to_mcp_server_handler(),
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
        .merge(streamable_http_routes::routes(&endpoint))
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

fn server_details(repository: &str) -> InitializeResult {
    InitializeResult {
        server_info: Implementation {
            name: format!("tabby-{repository}"),
            version: env!("CARGO_PKG_VERSION").into(),
            title: Some(format!("Tabby code search: {repository}")),
            description: None,
            icons: vec![],
            website_url: None,
        },
        capabilities: ServerCapabilities {
            tools: Some(ServerCapabilitiesTools { list_changed: None }),
            ..Default::default()
        },
        meta: None,
        instructions: Some(format!(
            "Search the `{repository}` repository. Use search_files to locate paths, grep to find code, and read_file to fetch a whole file."
        )),
        protocol_version: ProtocolVersion::V2025_11_25.into(),
    }
}

fn url_slug(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

struct SearchHandler {
    repository: String,
    root: PathBuf,
}

#[async_trait]
impl ServerHandler for SearchHandler {
    async fn handle_list_tools_request(
        &self,
        _params: Option<PaginatedRequestParams>,
        _runtime: Arc<dyn McpServer>,
    ) -> Result<ListToolsResult, RpcError> {
        Ok(ListToolsResult {
            tools: vec![
                tool(
                    "search_files",
                    "Fuzzy-match file and directory paths in the repository. Returns up to `limit` paths ranked by relevance.",
                    json!({
                        "pattern": { "type": "string", "description": "Fuzzy pattern matched against paths, e.g. `cfgload`." },
                        "limit": { "type": "integer", "description": "Maximum paths to return. Defaults to 40." }
                    }),
                    &["pattern"],
                ),
                tool(
                    "grep",
                    "Regex search over file contents, with a ripgrep-like syntax: bare text is a content pattern, `f:` filters by path, `lang:` filters by language, and any token can be negated with a `-` prefix.",
                    json!({
                        "query": { "type": "string", "description": "Example: `fn load_config lang:rust -f:test`." }
                    }),
                    &["query"],
                ),
                tool(
                    "read_file",
                    "Read a file from the repository at HEAD. Returns at most the first 200 KB.",
                    json!({
                        "path": { "type": "string", "description": "Repository-relative path, e.g. `crates/tabby/src/serve.rs`." }
                    }),
                    &["path"],
                ),
            ],
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
            other => return Err(CallToolError::unknown_tool(other.to_string())),
        };

        Ok(match result {
            Ok(text) => call_tool_result(text, false),
            Err(err) => call_tool_result(format!("{} search failed: {err}", self.repository), true),
        })
    }
}

impl SearchHandler {
    async fn search_files(&self, params: &CallToolRequestParams) -> anyhow::Result<String> {
        let pattern = required_str(params, "pattern")?;
        let limit = optional_usize(params, "limit").unwrap_or(40);

        let results = tabby_git::search_files(&self.root, None, pattern, limit).await?;
        if results.is_empty() {
            return Ok("No matching paths.".to_string());
        }

        Ok(results
            .iter()
            .map(|r| format!("{} ({})", r.path, r.r#type))
            .collect::<Vec<_>>()
            .join("\n"))
    }

    async fn grep(&self, params: &CallToolRequestParams) -> anyhow::Result<String> {
        let query = required_str(params, "query")?;

        let mut stream = Box::pin(tabby_git::grep(&self.root, None, query).await?);
        let mut files = Vec::new();
        let mut bytes = 0;
        let mut truncated = false;

        while let Some(file) = stream.next().await {
            if files.len() >= MAX_GREP_FILES || bytes >= MAX_RESPONSE_BYTES {
                truncated = true;
                break;
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

            bytes += lines.iter().map(|l| l.len()).sum::<usize>() + 2;
            if lines.is_empty() {
                continue;
            }
            files.push(format!("{}\n{}", file.path.display(), lines.join("\n")));
        }

        if files.is_empty() {
            return Ok(format!("No matches for `{query}`."));
        }
        let mut output = files.join("\n\n");
        if truncated {
            output.push_str(&format!(
                "\n\n… truncated after {} files, the pattern matched more",
                MAX_GREP_FILES
            ));
        }
        Ok(output)
    }

    fn read_file(&self, params: &CallToolRequestParams) -> anyhow::Result<String> {
        let path = required_str(params, "path")?;

        let repository = git2::Repository::open(&self.root)?;
        let commit = repository.head()?.peel_to_commit()?;
        let entry = commit.tree()?.get_path(Path::new(path))?;
        let blob = entry.to_object(&repository)?.peel_to_blob()?;

        let content = blob.content();
        let truncated = content.len() > MAX_FILE_BYTES;
        let text = String::from_utf8_lossy(&content[..content.len().min(MAX_FILE_BYTES)]);

        Ok(if truncated {
            format!("{text}\n… truncated at {MAX_FILE_BYTES} bytes")
        } else {
            text.into_owned()
        })
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

fn optional_usize(params: &CallToolRequestParams, key: &str) -> Option<usize> {
    params
        .arguments
        .as_ref()
        .and_then(|arguments| arguments.get(key))
        .and_then(Value::as_u64)
        .map(|value| value as usize)
}
