//! The MCP server (rmcp). Tool names are `snake_case`; each tool maps to one control-channel
//! method (see [`crate::rpc`]), answered in process (headless) or by the running app (bridge).

use std::sync::{Arc, Mutex, PoisonError};

use base64::Engine as _;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock as Content};
use rmcp::{ErrorData as McpError, ServerHandler, ServiceExt, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::budgets::{MAX_PNG_SIDE, check_png, fit_png, json_text};
use crate::headless::batch_reply;
use crate::security::MAX_BATCH_STEPS;
use crate::workspace::authorize_command;
use crate::{AutomationError, BridgeClient, Headless, bad};

/// Where tools run.
pub enum Backend {
    /// An in-process session.
    Headless(Arc<Mutex<Headless>>),
    /// The running desktop app, over its control channel.
    Bridge(Arc<BridgeClient>),
}

#[derive(Clone)]
pub struct MaterialcraftMcp {
    backend: Arc<Backend>,
    tool_router: ToolRouter<Self>,
}

// ---------------------------------------------------------------------------------------------
// Parameters (serialized back to JSON as the control method's params)

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct FilterParams {
    /// Only entries whose id or label contains this text (case-insensitive).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct LibraryParams {
    /// Only nodes whose id or label contains this text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    /// Only nodes in this library category, e.g. "Noises", "Patterns", "Filters/Blurs".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct RunParams {
    /// Command id from `command_list`, e.g. `graph.node.add`, `graph.connect`, `edit.undo`.
    pub id: String,
    /// Command parameters (see the command's `params` doc). Omitted = defaults; never a dialog.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct BatchParams {
    /// Commands run in order; the batch stops at the first error and reports its index.
    pub steps: Vec<RunParams>,
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct NewGraphParams {
    /// Template id: empty, pbr (default: six material outputs), bricks, stone_tiles, wood, metal_plates, rusty_metal, lava, marble, wicker, fabric, ground.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Graph size in pixels, a power of two from 16 to 8192 (default 1024).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct NodeAddParams {
    /// Node definition id from `library_list`, e.g. `perlin_noise`, `blend`, `levels`, `normal`, `output`.
    pub def: String,
    /// Graph position `[x, y]` (default: right of `connectFrom`, else the origin).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<[f32; 2]>,
    /// Parameter values by id (see `library_list`): numbers, `[x, y]`, `"#rrggbb"`, enum indices.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    /// Wire `{node, port?}`'s output into the new node's first input.
    #[serde(default, rename = "connectFrom", skip_serializing_if = "Option::is_none")]
    pub connect_from: Option<Value>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ConnectParams {
    /// Source node id.
    pub from: u64,
    /// Source output port id (default: its first output).
    #[serde(default, rename = "fromPort", skip_serializing_if = "Option::is_none")]
    pub from_port: Option<String>,
    /// Target node id.
    pub to: u64,
    /// Target input port id (default: its first input).
    #[serde(default, rename = "toPort", skip_serializing_if = "Option::is_none")]
    pub to_port: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ParamSetParams {
    /// Node id.
    pub id: u64,
    /// Parameter id (see `library_list`).
    pub param: String,
    /// The value: a number, `[x, y]`, `"#rrggbb"`, an enum index, a curve `[[x, y], …]` or a gradient `[[pos, color], …]`.
    pub value: Value,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct PathParams {
    /// Forward-slash path relative to the workspace root.
    pub path: String,
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct SaveParams {
    /// Forward-slash path relative to the write root (`.sbs` is appended unless it ends in `.sbs`
    /// or `.mcraft`). Omit to save to the package's own file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct BitmapParams {
    /// Image file relative to the read root (png, jpg, tga, tif, bmp, webp, exr, hdr).
    pub path: String,
    /// Resource name (default: the file name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ExportParams {
    /// Output folder relative to the write root (created if missing; `.` = the root).
    pub dir: String,
    /// png (default), jpg, tga, tif, bmp, webp, exr, hdr.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// File name pattern with $(graph), $(identifier) and $(usage) (default `$(graph)_$(identifier)`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    /// 8, 16 or 32 bits (default: the graph's output format).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<u32>,
    /// Pixels of the larger side, rounded down to a power of two, 16 to 8192 (default: the graph's size).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u32>,
    /// Graph index (default: the open graph).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct MaterialExportParams {
    /// Forward-slash path relative to the write root (`.mcmat` is appended if missing). The
    /// parent folder must exist.
    pub path: String,
    /// Graph index (default: the open graph).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph: Option<u32>,
    /// Square size in pixels, rounded to a power of two from 16 to 8192 (default: the graph's size).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u32>,
    /// Exposed input values to cook with and embed, by input id (see `session_inspect`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<Value>,
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct RenderParams {
    /// `3d` (default: the material on the 3D view's scene) or `2d` (a node's output).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<String>,
    /// Pixels, 16..2048.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    /// Camera yaw/pitch in degrees for this render only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yaw: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pitch: Option<f32>,
    /// 3D scene: rounded_cube, cube, sphere, cylinder, plane, torus, material_ball.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene: Option<String>,
    /// 2D: the node to show (default: the 2D view's node, else the first output).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<u64>,
    /// 2D: the output port index.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u32>,
    /// Cook at the full graph size instead of the preview size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full: Option<bool>,
    /// 3D: UV repeats of the material on the scene for this render only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tiling: Option<f32>,
    /// 3D beauty render: a finer displaced mesh, ambient occlusion and a shadowed key light (slower).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hq: Option<bool>,
    /// With `hq`: displacement depth in scene radii (default: the view's height depth).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub displacement: Option<f32>,
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct ScreenshotParams {
    /// `3d` (default) or `2d`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct MenuParams {
    /// Menu item id from the app's `ui.menu.list`, e.g. `file.save`, `ui.newGraph`, `ui.view.library`.
    pub id: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct InputParams {
    /// Input events in window points, applied in order on the next frames. Each is one of:
    /// {"type":"move","pos":[x,y]}, {"type":"down"|"up","pos":[x,y],"button":"left"|"right"|"middle"},
    /// {"type":"click"|"doubleClick","pos":[x,y],"button"?}, {"type":"drag","from":[x,y],"to":[x,y],"button"?,"steps"?},
    /// {"type":"scroll","pos":[x,y],"delta":[dx,dy]}, {"type":"key","key":"A"|"Enter"|"Space"|…,"modifiers"?:["ctrl","shift","alt"]},
    /// {"type":"text","text":"…"}.
    pub events: Vec<Value>,
}

// ---------------------------------------------------------------------------------------------
// Results

fn ok_json(v: &Value) -> CallToolResult {
    match json_text(v) {
        Ok(text) => CallToolResult::success(vec![Content::text(text)]),
        Err(e) => fail(e),
    }
}

fn fail(e: impl std::fmt::Display) -> CallToolResult {
    CallToolResult::error(vec![Content::text(e.to_string())])
}

fn to_result(r: Result<Value, AutomationError>) -> Result<CallToolResult, McpError> {
    Ok(match r {
        Ok(v) => ok_json(&v),
        Err(e) => fail(e),
    })
}

/// A `{width, height, png: base64}` reply (render or app screenshot) as image content.
fn image_result(r: Result<Value, AutomationError>, note: &str, refit: bool) -> Result<CallToolResult, McpError> {
    let png = r.and_then(|v| {
        let b64 = v.get("png").and_then(Value::as_str).ok_or_else(|| AutomationError::Other("the reply has no PNG".into()))?;
        if !refit {
            check_png(b64.len() / 4 * 3)?;
            return Ok(b64.to_string());
        }
        let bytes = base64::engine::general_purpose::STANDARD.decode(b64).map_err(|e| AutomationError::Other(format!("bad PNG data: {e}")))?;
        Ok(base64::engine::general_purpose::STANDARD.encode(fit_png(bytes, MAX_PNG_SIDE)?))
    });
    Ok(match png {
        Ok(b64) => CallToolResult::success(vec![Content::image(b64, "image/png"), Content::text(note.to_string())]),
        Err(e) => fail(e),
    })
}

fn params<T: Serialize>(p: &T) -> Value {
    serde_json::to_value(p).unwrap_or(Value::Null)
}

impl MaterialcraftMcp {
    pub fn headless(h: Headless) -> Self {
        Self::with_backend(Backend::Headless(Arc::new(Mutex::new(h))))
    }

    pub fn bridge(client: BridgeClient) -> Self {
        Self::with_backend(Backend::Bridge(Arc::new(client)))
    }

    pub fn with_backend(backend: Backend) -> Self {
        MaterialcraftMcp { backend: Arc::new(backend), tool_router: Self::tool_router() }
    }

    /// Serve MCP on stdin/stdout until the client disconnects.
    pub async fn serve_stdio(self) -> Result<(), AutomationError> {
        let running = self.serve(rmcp::transport::stdio()).await.map_err(|e| AutomationError::Other(format!("MCP start: {e}")))?;
        running.waiting().await.map_err(|e| AutomationError::Other(e.to_string()))?;
        Ok(())
    }

    fn is_bridge(&self) -> bool {
        matches!(*self.backend, Backend::Bridge(_))
    }

    /// One control-channel method on the backend.
    async fn call(&self, method: &str, p: Value) -> Result<Value, AutomationError> {
        match &*self.backend {
            Backend::Headless(h) => {
                let (h, m) = (h.clone(), method.to_string());
                // Edits run on snapshots (`Session::edit`), so a panicking command leaves the
                // session consistent: keep serving after a poisoned lock.
                tokio::task::spawn_blocking(move || h.lock().unwrap_or_else(PoisonError::into_inner).handle(&m, &p))
                    .await
                    .unwrap_or_else(|_| Err(AutomationError::Other(format!("internal error in `{method}`"))))
            }
            Backend::Bridge(b) => match method {
                "command.run" => {
                    let id = p.get("id").and_then(Value::as_str).unwrap_or_default();
                    authorize_command(id, p.get("params").unwrap_or(&Value::Null))?;
                    b.call(method, p).await
                }
                "package.save" if p.get("path").is_none() => b.call("ui.menu.invoke", json!({"id": "file.save"})).await,
                "package.save" | "package.open" | "bitmap.import" | "outputs.export" | "material.export" | "command.batch" | "render" => {
                    Err(bad(format!("`{method}` with paths works in headless mode only: the running app reads and writes its own files (use ui_menu_invoke)")))
                }
                _ => b.call(method, p).await,
            },
        }
    }

    async fn run(&self, id: &str, p: Value) -> Result<Value, AutomationError> {
        self.call("command.run", json!({"id": id, "params": p})).await
    }

    async fn bridge_only(&self, method: &str, p: Value) -> Result<Value, AutomationError> {
        if self.is_bridge() {
            self.call(method, p).await
        } else {
            Err(bad(format!(
                "`{method}` needs the running app: start `materialcraft --control <port>` and run `materialcraft-cli mcp --bridge <port>` (headless renders: view_render)"
            )))
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Tools

#[tool_router]
impl MaterialcraftMcp {
    #[tool(description = "The whole session as JSON: package, graphs, the open graph's nodes (definition, parameters, base \
        parameters, ports, cook errors, timings, mean values), connections, frames, comments, pins, exposed inputs, views and history.")]
    async fn session_inspect(&self) -> Result<CallToolResult, McpError> {
        to_result(self.call("session.inspect", json!({})).await)
    }

    #[tool(description = "List engine commands: id, label, menu path, shortcut, params doc and whether it can run now.")]
    async fn command_list(&self, Parameters(p): Parameters<FilterParams>) -> Result<CallToolResult, McpError> {
        to_result(self.call("command.list", params(&p)).await)
    }

    #[tool(description = "Run an engine command by id with JSON params (see command_list). Runs with defaults, never opens a \
        dialog. Commands given host paths are disabled: use package_open, package_save, bitmap_import and outputs_export.")]
    async fn command_run(&self, Parameters(p): Parameters<RunParams>) -> Result<CallToolResult, McpError> {
        to_result(self.call("command.run", params(&p)).await)
    }

    #[tool(description = "Run several commands in order: steps [{id, params}]. Stops at the first error. Returns {completed, \
        results, failed: {index, id, error} | null}; a failed batch is a tool error.")]
    async fn command_batch(&self, Parameters(p): Parameters<BatchParams>) -> Result<CallToolResult, McpError> {
        if p.steps.len() > MAX_BATCH_STEPS {
            return Ok(fail(format!("at most {MAX_BATCH_STEPS} steps per batch")));
        }
        let reply = if self.is_bridge() {
            let mut results = Vec::new();
            let mut failed = None;
            for (i, s) in p.steps.iter().enumerate() {
                match self.call("command.run", params(s)).await {
                    Ok(v) => results.push(v),
                    Err(e) => {
                        failed = Some((i, s.id.as_str(), e.to_string()));
                        break;
                    }
                }
            }
            batch_reply(results, failed)
        } else {
            match self.call("command.batch", params(&p)).await {
                Ok(v) => v,
                Err(e) => return Ok(fail(e)),
            }
        };
        Ok(if reply["failed"].is_null() { ok_json(&reply) } else { fail(json_text(&reply).unwrap_or_else(|e| e.to_string())) })
    }

    #[tool(description = "The node library: every node's id, label, category, input and output ports and parameters (id, \
        default, range, enum options). Filter by text or category.")]
    async fn library_list(&self, Parameters(p): Parameters<LibraryParams>) -> Result<CallToolResult, McpError> {
        to_result(self.run("library.list", params(&p)).await)
    }

    #[tool(description = "Add a new graph to the package from a template and open it. Returns {graph, name}.")]
    async fn graph_new(&self, Parameters(p): Parameters<NewGraphParams>) -> Result<CallToolResult, McpError> {
        to_result(self.run("file.new", params(&p)).await)
    }

    #[tool(description = "Add a node to the open graph (optionally wired from another node's output). Returns {id}.")]
    async fn node_add(&self, Parameters(p): Parameters<NodeAddParams>) -> Result<CallToolResult, McpError> {
        to_result(self.run("graph.node.add", params(&p)).await)
    }

    #[tool(description = "Wire an output port of one node into an input port of another (replacing what fed that input).")]
    async fn node_connect(&self, Parameters(p): Parameters<ConnectParams>) -> Result<CallToolResult, McpError> {
        to_result(self.run("graph.connect", params(&p)).await)
    }

    #[tool(description = "Set one parameter of a node.")]
    async fn node_param(&self, Parameters(p): Parameters<ParamSetParams>) -> Result<CallToolResult, McpError> {
        to_result(self.run("graph.node.param", params(&p)).await)
    }

    #[tool(
        description = "Open an .sbs or .mcraft package (path relative to the read root). Returns the session; `package.warnings` lists what an SBS file holds that MaterialCraft can't read yet (missing nodes are kept as placeholders)."
    )]
    async fn package_open(&self, Parameters(p): Parameters<PathParams>) -> Result<CallToolResult, McpError> {
        to_result(self.call("package.open", params(&p)).await)
    }

    #[tool(
        description = "Save the package (path relative to the write root; omit to save to its own file). Saves SBS (.sbs, the main format) unless the path ends in .mcraft."
    )]
    async fn package_save(&self, Parameters(p): Parameters<SaveParams>) -> Result<CallToolResult, McpError> {
        to_result(self.call("package.save", params(&p)).await)
    }

    #[tool(description = "Embed an image (path relative to the read root) as a bitmap resource. Returns its name, for a \
        `bitmap` node's `resource` parameter.")]
    async fn bitmap_import(&self, Parameters(p): Parameters<BitmapParams>) -> Result<CallToolResult, McpError> {
        to_result(self.call("bitmap.import", params(&p)).await)
    }

    #[tool(description = "Cook a graph at full size and write every output (base color, normal, roughness, …) into a folder \
        relative to the write root. Returns the written files.")]
    async fn outputs_export(&self, Parameters(p): Parameters<ExportParams>) -> Result<CallToolResult, McpError> {
        to_result(self.call("outputs.export", params(&p)).await)
    }

    #[tool(description = "Cook a graph at full size and write it as a portable .mcmat material (cooked maps, exposed inputs, \
        source package) to a path relative to the write root, for TextureCraft. Optional graph, size and exposed input values. \
        Returns {path, bytes}.")]
    async fn material_export(&self, Parameters(p): Parameters<MaterialExportParams>) -> Result<CallToolResult, McpError> {
        to_result(self.call("material.export", params(&p)).await)
    }

    #[tool(description = "Headless: render the material on the 3D view's scene, or a node's output (view 2d), as a PNG up to \
        2048 px a side. In bridge mode use view_screenshot.")]
    async fn view_render(&self, Parameters(p): Parameters<RenderParams>) -> Result<CallToolResult, McpError> {
        let note = format!("render {}", params(&p));
        image_result(self.call("render", params(&p)).await, &note, false)
    }

    #[tool(description = "Bridge mode: PNG of the live app's 3D or 2D view.")]
    async fn view_screenshot(&self, Parameters(p): Parameters<ScreenshotParams>) -> Result<CallToolResult, McpError> {
        image_result(self.bridge_only("view.screenshot", params(&p)).await, "screenshot of an app view", true)
    }

    #[tool(description = "Bridge mode: PNG of the whole app window (downscaled to 2048 px a side at most).")]
    async fn ui_screenshot(&self) -> Result<CallToolResult, McpError> {
        image_result(self.bridge_only("ui.screenshot", json!({})).await, "screenshot of the app window", true)
    }

    #[tool(description = "Bridge mode: the app's UI state (panels, windows, graph view camera, node rectangles on screen, toasts).")]
    async fn ui_inspect(&self) -> Result<CallToolResult, McpError> {
        to_result(self.bridge_only("ui.inspect", json!({})).await)
    }

    #[tool(description = "Bridge mode: activate a menu item by id like a click (may open a window). ui.menu.list lists ids.")]
    async fn ui_menu_invoke(&self, Parameters(p): Parameters<MenuParams>) -> Result<CallToolResult, McpError> {
        to_result(self.bridge_only("ui.menu.invoke", params(&p)).await)
    }

    #[tool(description = "Bridge mode: synthesize mouse, keyboard and text input in the app window (clicks, drags between \
        node ports, scrolling, shortcuts, typing). Positions are window points (see ui_inspect for node rectangles).")]
    async fn ui_input(&self, Parameters(p): Parameters<InputParams>) -> Result<CallToolResult, McpError> {
        to_result(self.bridge_only("ui.input", params(&p)).await)
    }
}

#[tool_handler(
    router = self.tool_router,
    name = "materialcraft",
    instructions = "MaterialCraft procedural material designer. A package holds graphs of nodes; each graph's output nodes \
        (usages baseColor, normal, roughness, metallic, height, ambientOcclusion) make a PBR material. Start with graph_new \
        (templates: pbr, bricks, wood, …) or package_open, browse nodes with library_list, build with node_add / node_connect \
        / node_param (or command_run/command_batch for anything else: command_list), check your work with session_inspect and \
        view_render (3d, or 2d for one node), then package_save, outputs_export or material_export (.mcmat for TextureCraft). Undo: command_run edit.undo."
)]
impl ServerHandler for MaterialcraftMcp {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::headless::Filesystem;

    #[tokio::test(flavor = "multi_thread")]
    async fn a_panic_does_not_wedge_the_session() {
        let mcp = MaterialcraftMcp::headless(Headless::new(materialcraft_engine::sample_session(), Filesystem::Denied));
        let Backend::Headless(h) = &*mcp.backend else { panic!("headless") };
        let h2 = h.clone();
        let _ = tokio::task::spawn_blocking(move || {
            let _g = h2.lock().unwrap();
            panic!("boom")
        })
        .await;
        assert!(mcp.call("session.inspect", json!({})).await.is_ok());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn material_export_tool_writes_through_the_workspace_only() {
        let base = std::env::temp_dir().join(format!("mc-mcp-{}-material", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let args = || Parameters(MaterialExportParams { path: "m".into(), graph: None, size: Some(16), values: None });

        let denied = MaterialcraftMcp::headless(Headless::new(materialcraft_engine::sample_session(), Filesystem::Denied));
        assert_eq!(denied.material_export(args()).await.unwrap().is_error, Some(true));

        let ws = crate::AuthorizedWorkspace::new(None, Some(&base)).unwrap();
        let mcp = MaterialcraftMcp::headless(Headless::new(materialcraft_engine::sample_session(), Filesystem::Workspace(ws)));
        assert_eq!(mcp.material_export(args()).await.unwrap().is_error, Some(false));
        assert!(base.join("m.mcmat").is_file());
        let escape = Parameters(MaterialExportParams { path: "../m.mcmat".into(), graph: None, size: Some(16), values: None });
        assert_eq!(mcp.material_export(escape).await.unwrap().is_error, Some(true));
        assert!(!base.parent().unwrap().join("m.mcmat").exists());
    }

    #[test]
    fn oversized_replies_become_errors() {
        let r = ok_json(&json!("x".repeat(crate::budgets::MAX_RESPONSE_BYTES)));
        assert_eq!(r.is_error, Some(true));
    }
}
