#![allow(dead_code)]
use compact_mcp::client::Client;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

pub struct Blender {
    pub scratch: TempDir,
    pub connection: PathBuf,
    pub client: Client,
    pub child: Child,
}

impl Blender {
    pub fn start() -> Option<Self> {
        let Some(exe) = std::env::var_os("BLENDER_EXE") else {
            eprintln!("Blender test skipped: set BLENDER_EXE");
            return None;
        };
        let scratch = tempfile::tempdir().unwrap();
        let connection = scratch.path().join("connection.json");
        let log = std::fs::File::create(scratch.path().join("blender.log")).unwrap();
        let mut child = Command::new(exe)
            .args([
                "--background",
                "--factory-startup",
                "--threads",
                "1",
                "--python-exit-code",
                "1",
                "--python",
            ])
            .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/blender_host.py"))
            .arg("--")
            .arg("--connection")
            .arg(&connection)
            .arg("--output")
            .arg(scratch.path().join("exports"))
            .arg("--stop")
            .arg(scratch.path().join("stop"))
            .env("BLENDER_USER_CONFIG", scratch.path().join("config"))
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(45);
        while !connection.exists() {
            if child.try_wait().unwrap().is_some() || Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!(
                    "Blender startup failed: {}",
                    std::fs::read_to_string(scratch.path().join("blender.log")).unwrap()
                );
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let client = Client::from_path(&connection).unwrap();
        Some(Self {
            scratch,
            connection,
            client,
            child,
        })
    }

    pub async fn call(&self, method: &str, params: Value) -> Value {
        self.client
            .call(method, params, Duration::from_secs(60))
            .await
            .unwrap_or_else(|error| {
                panic!(
                    "Blender {method} failed: {error:#}\n{}",
                    std::fs::read_to_string(self.scratch.path().join("blender.log"))
                        .unwrap_or_default()
                )
            })
    }

    pub async fn execute(&self, steps: Value) -> Value {
        self.call("execute", json!({"steps":steps})).await
    }

    pub async fn rejects(&self, steps: Value, message: &str) {
        let error = self
            .client
            .call("execute", json!({"steps":steps}), Duration::from_secs(10))
            .await
            .unwrap_err();
        assert!(error.to_string().contains(message), "{error:#}");
    }
}

impl Drop for Blender {
    fn drop(&mut self) {
        let _ = std::fs::write(self.scratch.path().join("stop"), "");
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.child.try_wait().ok().flatten().is_none() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

pub struct Mcp {
    child: tokio::process::Child,
    input: Option<tokio::process::ChildStdin>,
    output: tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
    id: u64,
    pub initialized: Value,
}

impl Mcp {
    pub async fn start(connection: &std::path::Path) -> Self {
        let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_blender-compact-mcp"))
            .env("BLENDER_COMPACT_CONNECTION", connection)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let input = child.stdin.take();
        let output = BufReader::new(child.stdout.take().unwrap()).lines();
        let mut this = Self {
            child,
            input,
            output,
            id: 0,
            initialized: Value::Null,
        };
        this.initialized = this.request("initialize", json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"integration","version":"1"}})).await;
        this.input
            .as_mut()
            .unwrap()
            .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n")
            .await
            .unwrap();
        this
    }

    pub async fn request(&mut self, method: &str, params: Value) -> Value {
        self.id += 1;
        let mut line =
            json!({"jsonrpc":"2.0","id":self.id,"method":method,"params":params}).to_string();
        line.push('\n');
        self.input
            .as_mut()
            .unwrap()
            .write_all(line.as_bytes())
            .await
            .unwrap();
        loop {
            let line = tokio::time::timeout(Duration::from_secs(60), self.output.next_line())
                .await
                .unwrap()
                .unwrap()
                .expect("MCP stdout closed");
            let response: Value =
                serde_json::from_str(&line).expect("Only JSON-RPC may appear on stdout");
            if response["id"] == self.id {
                return response;
            }
        }
    }

    pub async fn call(&mut self, name: &str, args: Value) -> Value {
        let reply = self
            .request("tools/call", json!({"name":name,"arguments":args}))
            .await;
        assert!(reply.get("error").is_none(), "{reply}");
        reply["result"].clone()
    }

    pub async fn close(mut self) {
        self.input.take();
        let status = tokio::time::timeout(Duration::from_secs(5), self.child.wait())
            .await
            .unwrap()
            .unwrap();
        assert!(status.success(), "MCP server exited with {status}");
    }
}

pub fn text(result: &Value) -> &str {
    result["content"][0]["text"].as_str().unwrap()
}
