use crate::{client::Client, format};
use anyhow::Result;
use base64::{Engine, engine::general_purpose::STANDARD};
use rmcp::{ErrorData, RoleServer, ServerHandler, model::*, service::RequestContext};
use serde_json::{Value, json};
use std::{sync::OnceLock, time::Duration};

pub const INSTRUCTIONS: &str = "Use loops and return only needed data. Discover unfamiliar operations. Python is unrestricted. Failed steps may leave changes; timeouts do not cancel work. Inspect before retrying.";

/// Explicit schemas keep discovery small; Blender validates the operation vocabulary.
pub fn tools() -> &'static [Tool] {
    static TOOLS: OnceLock<Vec<Tool>> = OnceLock::new();
    TOOLS.get_or_init(|| {
    let definitions = json!([
        {"name":"inspect","description":"names or case-sensitive match glob; fields selects properties. Follow next_offset until absent; keep scene unchanged.",
         "inputSchema":{"type":"object","properties":{
            "names":{"type":"array","items":{"type":"string"},"maxItems":100},
            "match":{"type":"string"},
            "fields":{"type":"array","items":{"type":"string"},"description":"name,type,managed,location,rotation,scale,vertices,polygons,materials"},
            "limit":{"type":"integer","default":20,"minimum":1,"maximum":100},
            "offset":{"type":"integer","default":0,"minimum":0},
            "context":{"type":"boolean","description":"Include Blender version and permissions."}},"additionalProperties":false},
         "annotations":{"readOnlyHint":true,"openWorldHint":false}},
        {"name":"discover","description":"List operations or get contracts by name/list; ? means optional.",
         "inputSchema":{"type":"object","properties":{"operation":{"anyOf":[{"type":"string"},{"type":"array","items":{"type":"string"},"minItems":1,"maxItems":16}]}},"additionalProperties":false},
         "annotations":{"readOnlyHint":true,"openWorldHint":false}},
        {"name":"execute","description":"Exactly one of code, script, steps. Module-level Python: assign result only if needed; no top-level return. dry_run checks syntax/permissions only.",
         "inputSchema":{"type":"object","properties":{
            "code":{"type":"string","description":"Globals: bpy, params, output_dir."},
            "script":{"type":"string","description":"Reuse a returned session handle."},
            "params":{"type":"object"},
            "max_output":{"type":"integer","default":2000,"minimum":0,"maximum":32000},
            "steps":{"type":"array","description":"Batch {op,...args}.","items":{"type":"object"},"minItems":1,"maxItems":100},
            "dry_run":{"type":"boolean","default":false},
            "timeout":{"type":"integer","default":180,"minimum":1,"maximum":86400}},"additionalProperties":false}},
        {"name":"capture","description":"Camera render or interactive viewport PNG.",
         "inputSchema":{"type":"object","properties":{
            "size":{"type":"integer","default":512,"minimum":64,"maximum":1024},
            "view":{"type":"string","enum":["camera","viewport"],"default":"camera"}},"additionalProperties":false},
         "annotations":{"destructiveHint":false,"openWorldHint":false}}
    ]);
    serde_json::from_value(definitions).expect("static tool definitions")
    })
}

#[derive(Clone)]
pub struct Bridge;

impl Bridge {
    pub async fn invoke(&self, method: &str, mut args: Value) -> Result<CallToolResult> {
        let tool = tools()
            .iter()
            .find(|t| t.name == method)
            .ok_or_else(|| anyhow::anyhow!("Unknown tool"))?;
        let object = args
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("Arguments must be an object"))?;
        let properties = tool.input_schema["properties"]
            .as_object()
            .expect("schema properties");
        if let Some(key) = object.keys().find(|key| !properties.contains_key(*key)) {
            anyhow::bail!("Unknown {method} argument: {key}");
        }
        let timeout = if method == "execute" {
            match object.remove("timeout") {
                Some(value) => value
                    .as_u64()
                    .filter(|s| (1..=86400).contains(s))
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "timeout must be 1..86400 seconds; timeout does not cancel Blender work"
                        )
                    })?,
                None => 180,
            }
        } else {
            180
        };
        let client = Client::discover(None)?;
        if method == "capture" {
            object.insert(
                "filename".into(),
                json!(format!("preview-{}.png", uuid::Uuid::new_v4().simple())),
            );
        }
        let result = client
            .call(method, args.clone(), Duration::from_secs(timeout))
            .await?;
        if method == "capture" {
            let bytes = client
                .image(
                    result["file"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("Capture lacks filename"))?,
                )
                .await?;
            return Ok(CallToolResult::success(vec![ContentBlock::image(
                STANDARD.encode(bytes),
                "image/png",
            )]));
        }
        let content = vec![ContentBlock::text(format::result(method, &args, &result))];
        Ok(if result["ok"] == false {
            CallToolResult::error(content)
        } else {
            CallToolResult::success(content)
        })
    }
}

impl ServerHandler for Bridge {
    fn get_info(&self) -> ServerConfig {
        let mut config = ServerConfig::default();
        config.capabilities = ServerCapabilities::builder().enable_tools().build();
        config.server_info = Implementation::new("blender-compact", env!("CARGO_PKG_VERSION"));
        config.instructions = Some(INSTRUCTIONS.into());
        config
    }

    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult {
            tools: tools().to_vec(),
            ..Default::default()
        })
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        tools().iter().find(|t| t.name == name).cloned()
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let args = Value::Object(request.arguments.unwrap_or_default());
        let result = self
            .invoke(&request.name, args)
            .await
            .unwrap_or_else(|error| {
                CallToolResult::error(vec![ContentBlock::text(format::bounded(&format!(
                    "error: {error:#}"
                )))])
            });
        Ok(result.into())
    }
}
