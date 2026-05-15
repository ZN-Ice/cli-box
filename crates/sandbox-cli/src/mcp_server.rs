use rmcp::model::{
    CallToolRequestParams, CallToolResult, Content, ErrorCode, Implementation, InitializeResult,
    ListToolsResult, ServerCapabilities, Tool,
};
use rmcp::service::{RequestContext, RoleServer};
use rmcp::ServerHandler;
use sandbox_core::automation::ax_ui::UiInspector;
use sandbox_core::automation::cg_event::{InputSimulator, MouseButton};
use sandbox_core::capture::ScreenCapture;
use sandbox_core::diff::{diff_images, DiffOptions};
use sandbox_core::player::ActionPlayer;
use sandbox_core::process::ProcessManager;
use sandbox_core::recorder::{Action, ActionRecorder};
use sandbox_core::scenario::ScenarioRunner;
use serde::Deserialize;

#[derive(Clone)]
pub struct SandboxMcpServer {
    recorder: std::sync::Arc<ActionRecorder>,
}

impl SandboxMcpServer {
    pub fn new() -> Self {
        Self {
            recorder: std::sync::Arc::new(ActionRecorder::new()),
        }
    }
}

// ── Tool implementations (called by MCP handler) ────────

impl SandboxMcpServer {
    async fn do_screenshot(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let png_data = ScreenCapture::capture_sandbox()
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;

        let b64 = base64_encode(&png_data);
        Ok(CallToolResult::success(vec![Content::text(format!(
            "Screenshot captured ({} bytes base64)",
            b64.len()
        ))]))
    }

    async fn do_click(&self, params: ClickParams) -> Result<CallToolResult, rmcp::ErrorData> {
        let button = match params.button.to_lowercase().as_str() {
            "left" => MouseButton::Left,
            "right" => MouseButton::Right,
            "middle" => MouseButton::Middle,
            _ => MouseButton::Left,
        };
        InputSimulator::click(params.x, params.y, button)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(
            "ok".to_string(),
        )]))
    }

    async fn do_type_text(
        &self,
        params: TypeTextParams,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        InputSimulator::type_text(&params.text)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(
            "ok".to_string(),
        )]))
    }

    async fn do_press_key(
        &self,
        params: PressKeyParams,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let mod_refs: Vec<&str> = params.modifiers.iter().map(|s| s.as_str()).collect();
        InputSimulator::press_key(&params.key, &mod_refs)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(
            "ok".to_string(),
        )]))
    }

    async fn do_spawn_cli(
        &self,
        params: SpawnCliParams,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let info = ProcessManager::spawn_cli(&params.command, &params.args)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(format!(
            "Spawned PID={}",
            info.pid
        ))]))
    }

    async fn do_kill_process(
        &self,
        params: KillProcessParams,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        ProcessManager::kill_process(params.pid)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(
            "ok".to_string(),
        )]))
    }

    async fn do_list_processes(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let processes = ProcessManager::list_processes()
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        let out: Vec<String> = processes
            .iter()
            .map(|p| format!("PID={}: {}", p.pid, p.name))
            .collect();
        Ok(CallToolResult::success(vec![Content::text(out.join("\n"))]))
    }

    async fn do_list_windows(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let windows = ScreenCapture::list_windows()
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        let out: Vec<String> = windows
            .iter()
            .map(|(id, title)| format!("ID={id}: {title}"))
            .collect();
        Ok(CallToolResult::success(vec![Content::text(out.join("\n"))]))
    }

    async fn do_double_click(
        &self,
        params: ClickParams,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        InputSimulator::double_click(params.x, params.y)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(
            "ok".to_string(),
        )]))
    }

    async fn do_inspect_ui(
        &self,
        params: InspectUiParams,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let element = UiInspector::inspect_window(params.window_id)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        let json = serde_json::to_string_pretty(&element)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    async fn do_find_element(
        &self,
        params: FindElementParams,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let elements = UiInspector::find_elements(
            params.window_id,
            params.role.as_deref(),
            params.title.as_deref(),
        )
        .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        let json = serde_json::to_string_pretty(&elements)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    async fn do_get_element_value(
        &self,
        params: GetElementValueParams,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let value = UiInspector::get_element_value(&params.element_id)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(format!(
            "{value:?}"
        ))]))
    }

    async fn do_record_action(
        &self,
        params: RecordActionParams,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let action = params.to_action();
        self.recorder
            .record(action)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(
            "recorded".to_string(),
        )]))
    }

    async fn do_record_start(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        self.recorder
            .start(None)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(
            "recording_started".to_string(),
        )]))
    }

    async fn do_record_stop(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let actions = self
            .recorder
            .stop()
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        let json = serde_json::to_string_pretty(&actions)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    async fn do_play_actions(
        &self,
        params: PlayActionsParams,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let mut player = ActionPlayer::new(params.speed);
        let results = player.play(&params.actions).await;
        Ok(CallToolResult::success(vec![Content::text(format!(
            "{results:?}"
        ))]))
    }

    async fn do_run_scenario(
        &self,
        params: RunScenarioParams,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let scenario = ScenarioRunner::load_from_str(&params.yaml)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        let report = ScenarioRunner::run(&scenario, params.speed).await;
        Ok(CallToolResult::success(vec![Content::text(
            report.to_markdown(),
        )]))
    }

    async fn do_diff_screenshot(
        &self,
        params: DiffScreenshotParams,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        use base64::Engine;
        let expected = base64::engine::general_purpose::STANDARD
            .decode(&params.expected)
            .map_err(|e| rmcp::ErrorData::invalid_params(format!("Invalid base64: {e}"), None))?;
        let actual = base64::engine::general_purpose::STANDARD
            .decode(&params.actual)
            .map_err(|e| rmcp::ErrorData::invalid_params(format!("Invalid base64: {e}"), None))?;

        let mut options = DiffOptions::default();
        if let Some(t) = params.threshold {
            options.threshold = t;
        }
        if let Some(m) = params.max_diff_percentage {
            options.max_diff_percentage = m;
        }

        let result = diff_images(&expected, &actual, &options)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        let json = serde_json::to_string_pretty(&result)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[allow(clippy::type_complexity)]
    fn dispatch_tool(
        &self,
        name: &str,
        args: Option<rmcp::model::JsonObject>,
    ) -> Result<
        std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<CallToolResult, rmcp::ErrorData>>
                    + Send
                    + '_,
            >,
        >,
        rmcp::ErrorData,
    > {
        let args = args.unwrap_or_default();
        match name {
            "screenshot" => Ok(Box::pin(self.do_screenshot())),
            "click" => {
                let p: ClickParams = serde_json::from_value(serde_json::Value::Object(args))
                    .map_err(|e| rmcp::ErrorData::invalid_params(e.to_string(), None))?;
                Ok(Box::pin(self.do_click(p)))
            }
            "type_text" => {
                let p: TypeTextParams = serde_json::from_value(serde_json::Value::Object(args))
                    .map_err(|e| rmcp::ErrorData::invalid_params(e.to_string(), None))?;
                Ok(Box::pin(self.do_type_text(p)))
            }
            "press_key" => {
                let p: PressKeyParams = serde_json::from_value(serde_json::Value::Object(args))
                    .map_err(|e| rmcp::ErrorData::invalid_params(e.to_string(), None))?;
                Ok(Box::pin(self.do_press_key(p)))
            }
            "spawn_cli" => {
                let p: SpawnCliParams = serde_json::from_value(serde_json::Value::Object(args))
                    .map_err(|e| rmcp::ErrorData::invalid_params(e.to_string(), None))?;
                Ok(Box::pin(self.do_spawn_cli(p)))
            }
            "kill_process" => {
                let p: KillProcessParams = serde_json::from_value(serde_json::Value::Object(args))
                    .map_err(|e| rmcp::ErrorData::invalid_params(e.to_string(), None))?;
                Ok(Box::pin(self.do_kill_process(p)))
            }
            "list_processes" => Ok(Box::pin(self.do_list_processes())),
            "list_windows" => Ok(Box::pin(self.do_list_windows())),
            "double_click" => {
                let p: ClickParams = serde_json::from_value(serde_json::Value::Object(args))
                    .map_err(|e| rmcp::ErrorData::invalid_params(e.to_string(), None))?;
                Ok(Box::pin(self.do_double_click(p)))
            }
            "inspect_ui" => {
                let p: InspectUiParams = serde_json::from_value(serde_json::Value::Object(args))
                    .map_err(|e| rmcp::ErrorData::invalid_params(e.to_string(), None))?;
                Ok(Box::pin(self.do_inspect_ui(p)))
            }
            "find_element" => {
                let p: FindElementParams = serde_json::from_value(serde_json::Value::Object(args))
                    .map_err(|e| rmcp::ErrorData::invalid_params(e.to_string(), None))?;
                Ok(Box::pin(self.do_find_element(p)))
            }
            "get_element_value" => {
                let p: GetElementValueParams =
                    serde_json::from_value(serde_json::Value::Object(args))
                        .map_err(|e| rmcp::ErrorData::invalid_params(e.to_string(), None))?;
                Ok(Box::pin(self.do_get_element_value(p)))
            }
            "record_action" => {
                let p: RecordActionParams = serde_json::from_value(serde_json::Value::Object(args))
                    .map_err(|e| rmcp::ErrorData::invalid_params(e.to_string(), None))?;
                Ok(Box::pin(self.do_record_action(p)))
            }
            "record_start" => Ok(Box::pin(self.do_record_start())),
            "record_stop" => Ok(Box::pin(self.do_record_stop())),
            "play_actions" => {
                let p: PlayActionsParams = serde_json::from_value(serde_json::Value::Object(args))
                    .map_err(|e| rmcp::ErrorData::invalid_params(e.to_string(), None))?;
                Ok(Box::pin(self.do_play_actions(p)))
            }
            "run_scenario" => {
                let p: RunScenarioParams = serde_json::from_value(serde_json::Value::Object(args))
                    .map_err(|e| rmcp::ErrorData::invalid_params(e.to_string(), None))?;
                Ok(Box::pin(self.do_run_scenario(p)))
            }
            "diff_screenshot" => {
                let p: DiffScreenshotParams =
                    serde_json::from_value(serde_json::Value::Object(args))
                        .map_err(|e| rmcp::ErrorData::invalid_params(e.to_string(), None))?;
                Ok(Box::pin(self.do_diff_screenshot(p)))
            }
            _ => Err(rmcp::ErrorData::new(
                ErrorCode::METHOD_NOT_FOUND,
                format!("Tool not found: {name}"),
                None,
            )),
        }
    }
}

// ── Parameter types ──────────────────────────────────────

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ClickParams {
    #[schemars(description = "X coordinate")]
    pub x: f64,
    #[schemars(description = "Y coordinate")]
    pub y: f64,
    #[schemars(description = "Mouse button")]
    #[serde(default = "default_button")]
    pub button: String,
}

fn default_button() -> String {
    "left".to_string()
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct TypeTextParams {
    #[schemars(description = "Text to type")]
    pub text: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct PressKeyParams {
    #[schemars(description = "Key name")]
    pub key: String,
    #[schemars(description = "Modifier keys")]
    #[serde(default)]
    pub modifiers: Vec<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SpawnCliParams {
    #[schemars(description = "Command to execute")]
    pub command: String,
    #[schemars(description = "Command arguments")]
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct KillProcessParams {
    #[schemars(description = "Process ID to kill")]
    pub pid: u32,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct InspectUiParams {
    #[schemars(description = "Window ID to inspect")]
    pub window_id: u32,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct FindElementParams {
    #[schemars(description = "Window ID to search in")]
    pub window_id: u32,
    #[schemars(description = "AXRole filter (e.g., AXButton, AXTextField)")]
    #[serde(default)]
    pub role: Option<String>,
    #[schemars(description = "Title substring filter")]
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct GetElementValueParams {
    #[schemars(description = "Element ID path (format: pid:window_idx:child_idx...)")]
    pub element_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct RecordActionParams {
    #[schemars(
        description = "Action type: click, type_text, press_key, scroll, drag, screenshot, wait"
    )]
    pub action_type: String,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub button: Option<String>,
    pub text: Option<String>,
    pub key: Option<String>,
    pub modifiers: Option<Vec<String>>,
    pub direction: Option<String>,
    pub amount: Option<i32>,
    pub from_x: Option<f64>,
    pub from_y: Option<f64>,
    pub to_x: Option<f64>,
    pub to_y: Option<f64>,
    pub label: Option<String>,
    pub duration_ms: Option<u64>,
}

impl RecordActionParams {
    fn to_action(&self) -> Action {
        match self.action_type.as_str() {
            "click" => Action::Click {
                x: self.x.unwrap_or(0.0),
                y: self.y.unwrap_or(0.0),
                button: self.button.clone().unwrap_or_else(|| "left".to_string()),
                timestamp_ms: None,
            },
            "double_click" => Action::DoubleClick {
                x: self.x.unwrap_or(0.0),
                y: self.y.unwrap_or(0.0),
                timestamp_ms: None,
            },
            "type_text" => Action::TypeText {
                text: self.text.clone().unwrap_or_default(),
                timestamp_ms: None,
            },
            "press_key" => Action::PressKey {
                key: self.key.clone().unwrap_or_default(),
                modifiers: self.modifiers.clone().unwrap_or_default(),
                timestamp_ms: None,
            },
            "scroll" => Action::Scroll {
                x: self.x.unwrap_or(0.0),
                y: self.y.unwrap_or(0.0),
                direction: self.direction.clone().unwrap_or_else(|| "down".to_string()),
                amount: self.amount.unwrap_or(1),
                timestamp_ms: None,
            },
            "drag" => Action::Drag {
                from_x: self.from_x.unwrap_or(0.0),
                from_y: self.from_y.unwrap_or(0.0),
                to_x: self.to_x.unwrap_or(0.0),
                to_y: self.to_y.unwrap_or(0.0),
                timestamp_ms: None,
            },
            "screenshot" => Action::Screenshot {
                label: self.label.clone(),
                timestamp_ms: None,
            },
            "wait" => Action::Wait {
                duration_ms: self.duration_ms.unwrap_or(1000),
                timestamp_ms: None,
            },
            _ => Action::Wait {
                duration_ms: 0,
                timestamp_ms: None,
            },
        }
    }
}

#[derive(Deserialize)]
pub struct PlayActionsParams {
    pub actions: Vec<Action>,
    #[serde(default = "default_speed")]
    pub speed: f64,
}

fn default_speed() -> f64 {
    1.0
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct RunScenarioParams {
    #[schemars(description = "YAML scenario content")]
    pub yaml: String,
    #[schemars(description = "Speed multiplier (1.0 = original speed)")]
    #[serde(default = "default_speed")]
    pub speed: f64,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct DiffScreenshotParams {
    #[schemars(description = "Base64-encoded expected PNG image")]
    pub expected: String,
    #[schemars(description = "Base64-encoded actual PNG image")]
    pub actual: String,
    #[schemars(description = "Pixel difference threshold (0-255)")]
    #[serde(default)]
    pub threshold: Option<u8>,
    #[schemars(description = "Maximum diff percentage for identical (0.0-100.0)")]
    #[serde(default)]
    pub max_diff_percentage: Option<f64>,
}

// ── Tool definitions (for list_tools) ────────────────────

fn tool_definitions() -> Vec<Tool> {
    let empty_schema = serde_json::Map::new();
    vec![
        Tool::new(
            "screenshot",
            "Take a screenshot of the sandbox window. Returns base64-encoded PNG image data.",
            empty_schema.clone(),
        ),
        Tool::new(
            "click",
            "Simulate a mouse click at coordinates (x, y) with button (left/right/middle).",
            empty_schema.clone(),
        ),
        Tool::new(
            "type_text",
            "Type text into the currently focused element.",
            empty_schema.clone(),
        ),
        Tool::new(
            "press_key",
            "Press a key (Return, Tab, Escape, etc.) with optional modifiers.",
            empty_schema.clone(),
        ),
        Tool::new(
            "spawn_cli",
            "Spawn a CLI process in the sandbox.",
            empty_schema.clone(),
        ),
        Tool::new(
            "kill_process",
            "Kill a running process by its PID.",
            empty_schema.clone(),
        ),
        Tool::new(
            "list_processes",
            "List all processes running in the sandbox.",
            empty_schema.clone(),
        ),
        Tool::new(
            "list_windows",
            "List all available windows with their IDs and titles.",
            empty_schema.clone(),
        ),
        Tool::new(
            "double_click",
            "Simulate a double click at the specified coordinates.",
            empty_schema.clone(),
        ),
        Tool::new(
            "inspect_ui",
            "Read the AXUIElement tree for a window. Returns the full UI element hierarchy.",
            empty_schema.clone(),
        ),
        Tool::new(
            "find_element",
            "Find UI elements by role and/or title within a window.",
            empty_schema.clone(),
        ),
        Tool::new(
            "get_element_value",
            "Get the value of a specific UI element by its path ID.",
            empty_schema.clone(),
        ),
        Tool::new(
            "record_start",
            "Start recording user actions.",
            empty_schema.clone(),
        ),
        Tool::new(
            "record_stop",
            "Stop recording and return recorded actions.",
            empty_schema.clone(),
        ),
        Tool::new(
            "record_action",
            "Record a single action manually.",
            empty_schema.clone(),
        ),
        Tool::new(
            "play_actions",
            "Play back a list of recorded actions.",
            empty_schema.clone(),
        ),
        Tool::new(
            "run_scenario",
            "Run a YAML test scenario and return a report.",
            empty_schema.clone(),
        ),
        Tool::new(
            "diff_screenshot",
            "Compare two base64-encoded screenshots and return diff results.",
            empty_schema.clone(),
        ),
    ]
}

// ── ServerHandler implementation ─────────────────────────

impl ServerHandler for SandboxMcpServer {
    fn get_info(&self) -> InitializeResult {
        InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                "system-test-sandbox",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(
                "macOS Desktop Automation Sandbox MCP Server.\n\
                 Tools: screenshot, click, type_text, press_key, spawn_cli, \
                 kill_process, list_processes, list_windows, double_click.",
            )
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.dispatch_tool(&request.name, request.arguments)?.await
    }

    async fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, rmcp::ErrorData> {
        Ok(ListToolsResult::with_all_items(tool_definitions()))
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        tool_definitions().into_iter().find(|t| t.name == name)
    }
}

// ── MCP stdio server runner ──────────────────────────────

pub async fn run_stdio_server() -> Result<(), anyhow::Error> {
    use rmcp::ServiceExt;

    tracing::info!("Starting MCP server on stdio transport");

    let (stdin, stdout) = rmcp::transport::io::stdio();

    let service = SandboxMcpServer::new()
        .serve((stdin, stdout))
        .await
        .inspect_err(|e| tracing::error!("MCP server init error: {e:?}"))
        .map_err(|e| anyhow::anyhow!("MCP serve error: {e}"))?;

    tracing::info!("MCP server running, waiting for requests...");
    service
        .waiting()
        .await
        .map_err(|e| anyhow::anyhow!("MCP runtime error: {e}"))?;
    Ok(())
}

fn base64_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}
