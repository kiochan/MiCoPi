//! MCP 2025-11-25 stdio: one JSON-RPC value per line, stdout is protocol-only.
use super::batch::{line, Backend};
use crate::application::{Action, ActionError};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
pub struct Server<B> {
    pub backend: B,
    initialized: bool,
    ready: bool,
}
impl<B: Backend> Server<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            initialized: false,
            ready: false,
        }
    }
    pub fn message(&mut self, value: Value) -> Option<Value> {
        let id = value.get("id").cloned();
        let method = value["method"].as_str().unwrap_or("");
        let error = |code, message: &str| json!({"jsonrpc":"2.0","id":id.clone().unwrap_or(Value::Null),"error":{"code":code,"message":message}});
        if value["jsonrpc"] != "2.0" || method.is_empty() || !value.is_object() {
            return Some(error(-32600, "Invalid Request"));
        }
        if method == "notifications/initialized" {
            if self.initialized {
                self.ready = true;
            }
            return None;
        }
        if id.is_none() {
            return None;
        }
        if method == "ping" {
            return Some(json!({"jsonrpc":"2.0","id":id,"result":{}}));
        }
        let params = value.get("params").cloned().unwrap_or(json!({}));
        let result=match method {
            "initialize"=>{
                if self.initialized{return Some(error(-32600,"Already initialized"));}
                if !params["protocolVersion"].is_string(){return Some(error(-32602,"Missing protocolVersion"));}
                let protocol=match params["protocolVersion"].as_str().unwrap(){"2024-11-05"=>"2024-11-05","2025-03-26"=>"2025-03-26","2025-06-18"=>"2025-06-18",_=>"2025-11-25"};
                self.initialized=true;Ok(json!({"protocolVersion":protocol,"capabilities":{"tools":{"listChanged":false},"resources":{"listChanged":false}},"serverInfo":{"name":"micopi","version":env!("CARGO_PKG_VERSION")},"instructions":"Start with app.describe to discover actions, schemas and public plugin state. Read context.read or micopi://state resources for the active document, opened file, tools and input. pipeline.run executes sequential steps with JSON Pointer result references, dry_run and expected_revision; it stops on error without rolling back prior steps. Inspect its result.ok and completed fields. All tools share one document Session. Coordinates are document pixels; colors are #RRGGBB. canvas.snapshot returns PNG. GUI attach edits the open document; default mode is headless."}))
            }
            _ if !self.ready=>return Some(error(-32002,"Initialize and send notifications/initialized first")),
            "resources/list"=>self.backend.dispatch(Action::new("app.describe",json!({}))).map(|d|json!({"resources":d["result"]["resources"].as_array().unwrap_or(&vec![]).iter().map(|r|json!({"uri":r["uri"],"name":r["id"],"title":r["title"],"mimeType":"application/json"})).collect::<Vec<_>>()})),
            "resources/read"=>{
                let Some(uri)=params["uri"].as_str() else { return Some(error(-32602,"Missing resource URI")); };
                let Some(resource)=uri.strip_prefix("micopi://state/") else { return Some(error(-32602,"Unknown resource URI")); };
                self.backend.dispatch(Action::new("context.read",json!({"resources":[resource]}))).and_then(|d|{
                    let data=&d["result"]["resources"][resource];
                    if data["ok"]!=true { return Err(ActionError::new("resource_error",data["error"].to_string())); }
                    Ok(json!({"contents":[{"uri":uri,"mimeType":"application/json","text":data["value"].to_string()}]}))
                })
            }
            "tools/list"=>self.backend.actions().map(|actions|json!({"tools":actions.as_array().unwrap_or(&vec![]).iter().map(|a|json!({"name":a["id"],"title":a["title"],"description":format!("MiCoPi action {}",a["id"].as_str().unwrap_or("")),"inputSchema":a["inputSchema"],"annotations":{"readOnlyHint":a["readOnly"],"openWorldHint":false}})).collect::<Vec<_>>()})),
            "tools/call"=>{
                let Some(name)=params["name"].as_str()else{return Some(error(-32602,"Missing tool name"));};
                let args=params.get("arguments").cloned().unwrap_or(json!({}));
                match self.backend.dispatch(Action::new(name,args)){
                    Ok(data)=>{
                        let content=if data["result"]["mimeType"]=="image/png"{json!([{"type":"image","mimeType":"image/png","data":data["result"]["data"]}])}else{json!([{"type":"text","text":data.to_string()}])};
                        let failed=super::batch::failed(&data);
                        Ok(json!({"content":content,"structuredContent":data,"isError":failed}))
                    }
                    Err(e)=>Ok(json!({"content":[{"type":"text","text":e.to_string()}],"structuredContent":{"error":e},"isError":true})),
                }
            }
            _=>return Some(error(-32601,"Method not found")),
        };
        Some(match result {
            Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
            Err(e) => {
                json!({"jsonrpc":"2.0","id":id,"error":{"code":-32603,"message":e.to_string()}})
            }
        })
    }
    pub fn serve(
        &mut self,
        reader: &mut impl BufRead,
        writer: &mut impl Write,
    ) -> Result<(), ActionError> {
        while let Some(text) = line(reader)? {
            let response = match serde_json::from_str(&text) {
                Ok(value) => self.message(value),
                Err(_) => Some(
                    json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Parse error"}}),
                ),
            };
            if let Some(response) = response {
                serde_json::to_writer(&mut *writer, &response)?;
                writer.write_all(b"\n")?;
                writer.flush()?;
            }
        }
        Ok(())
    }
}
