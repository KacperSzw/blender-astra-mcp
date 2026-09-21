//! The private Blender protocol. Never replay a request after an uncertain outcome.
use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use uuid::Uuid;

pub const MAX_REQUEST: usize = 256 * 1024;
pub const MAX_RESPONSE: usize = 2 * 1024 * 1024;

#[derive(Clone, Deserialize)]
pub struct Descriptor {
    pub protocol: u8,
    pub host: String,
    pub port: u16,
    pub token: String,
    pub output_dir: PathBuf,
}

pub struct Client {
    pub info: Descriptor,
}

pub fn state_dir() -> Result<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("XDG_STATE_HOME"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/state")))
        .context("No local application state directory")?;
    Ok(base.join("blender-compact-mcp"))
}

impl Client {
    pub fn discover(explicit: Option<&Path>) -> Result<Self> {
        let env = std::env::var_os("BLENDER_COMPACT_CONNECTION").map(PathBuf::from);
        if let Some(path) = explicit.or(env.as_deref()) {
            return Self::from_path(path);
        }
        Self::from_state_dir(&state_dir()?)
    }

    pub fn from_state_dir(root: &Path) -> Result<Self> {
        let mut paths = Vec::new();
        match std::fs::read_dir(root) {
            Ok(entries) => {
                for entry in entries {
                    let path = entry?.path();
                    if path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("connection-") && n.ends_with(".json"))
                    {
                        paths.push(path);
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        ensure!(
            paths.len() == 1,
            "Start the Blender bridge. For multiple/stale instances, set BLENDER_COMPACT_CONNECTION."
        );
        Self::from_path(&paths[0])
    }

    pub fn from_path(path: &Path) -> Result<Self> {
        let info: Descriptor = serde_json::from_slice(
            &std::fs::read(path).context("Cannot read Blender connection descriptor")?,
        )
        .context("Invalid Blender connection descriptor")?;
        ensure!(
            info.protocol == 1 && info.host == "127.0.0.1",
            "Only protocol 1 loopback connections are supported"
        );
        ensure!(
            info.port != 0 && !info.token.is_empty(),
            "Invalid Blender connection descriptor"
        );
        Ok(Self { info })
    }

    pub async fn call(&self, method: &str, params: Value, timeout: Duration) -> Result<Value> {
        self.call_with_id(method, params, timeout, &Uuid::new_v4().to_string())
            .await
    }

    pub async fn call_with_id(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
        id: &str,
    ) -> Result<Value> {
        ensure!(params.is_object(), "params must be an object");
        let mut data = serde_json::to_vec(
            &json!({"id":id,"token":self.info.token,"method":method,"params":params}),
        )?;
        data.push(b'\n');
        ensure!(data.len() <= MAX_REQUEST, "Request exceeds 256 KiB");
        let exchange = async {
            let mut conn =
                TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, self.info.port)).await?;
            conn.write_all(&data).await?;
            let mut received = Vec::new();
            let mut chunk = [0; 16384];
            loop {
                let count = conn.read(&mut chunk).await?;
                ensure!(count != 0, "Connection closed before a complete result");
                if let Some(end) = chunk[..count].iter().position(|&c| c == b'\n') {
                    ensure!(
                        received.len() + end <= MAX_RESPONSE,
                        "Response exceeds 2 MiB"
                    );
                    received.extend_from_slice(&chunk[..end]);
                    return serde_json::from_slice::<Value>(&received)
                        .context("Invalid bridge response");
                }
                ensure!(
                    received.len() + count <= MAX_RESPONSE,
                    "Response exceeds 2 MiB"
                );
                received.extend_from_slice(&chunk[..count]);
            }
        };
        let response = tokio::time::timeout(timeout, exchange).await
            .context("Transport timed out")
            .and_then(|r| r)
            .with_context(|| format!("Transport failed; outcome may be unknown. Inspect before retrying. Request id: {id}"))?;
        if response["ok"] != true {
            bail!(
                "{}",
                response["error"]
                    .as_str()
                    .unwrap_or("Bridge rejected request")
            );
        }
        response
            .get("result")
            .cloned()
            .context("Bridge response lacks result; outcome may be unknown")
    }

    pub async fn image(&self, filename: &str) -> Result<Vec<u8>> {
        let relative = Path::new(filename);
        ensure!(
            relative.components().count() == 1
                && relative.file_name() == Some(relative.as_os_str())
                && relative
                    .extension()
                    .is_some_and(|s| s.eq_ignore_ascii_case("png")),
            "Invalid image output path"
        );
        let root = self.info.output_dir.canonicalize()?;
        let path = root.join(relative);
        ensure!(
            !std::fs::symlink_metadata(&path)?.file_type().is_symlink(),
            "Invalid image output path"
        );
        ensure!(
            path.canonicalize()?.parent() == Some(root.as_path()),
            "Invalid image output path"
        );
        let bytes = tokio::fs::read(path).await?;
        ensure!(
            bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
            "Capture is not a PNG"
        );
        Ok(bytes)
    }
}
