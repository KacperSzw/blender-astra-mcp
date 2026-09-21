use compact_mcp::client::{Client, Descriptor, MAX_REQUEST, MAX_RESPONSE};
use serde_json::json;
use std::time::Duration;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

fn descriptor(port: u16) -> Client {
    Client {
        info: Descriptor {
            protocol: 1,
            host: "127.0.0.1".into(),
            port,
            token: "test-secret".into(),
            output_dir: std::env::temp_dir(),
        },
    }
}

#[test]
fn descriptor_discovery_is_explicit_and_loopback_only() {
    let dir = tempfile::tempdir().unwrap();
    assert!(Client::from_state_dir(dir.path()).is_err());
    let info =
        json!({"protocol":1,"host":"127.0.0.1","port":1,"token":"secret","output_dir":dir.path()});
    std::fs::write(dir.path().join("connection-1.json"), info.to_string()).unwrap();
    assert!(Client::from_state_dir(dir.path()).is_ok());
    std::fs::write(dir.path().join("connection-2.json"), info.to_string()).unwrap();
    assert!(Client::from_state_dir(dir.path()).is_err());
    let mut info = info;
    info["host"] = json!("example.com");
    std::fs::write(dir.path().join("remote.json"), info.to_string()).unwrap();
    assert!(
        Client::from_path(&dir.path().join("remote.json"))
            .err()
            .unwrap()
            .to_string()
            .contains("loopback")
    );
}

#[tokio::test]
async fn frames_partial_socket_reads_and_authenticates() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client = descriptor(listener.local_addr().unwrap().port());
    let task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        loop {
            let b = stream.read_u8().await.unwrap();
            if b == b'\n' {
                break;
            }
            request.push(b);
        }
        let request: serde_json::Value = serde_json::from_slice(&request).unwrap();
        assert_eq!(request["token"], "test-secret");
        assert_eq!(request["id"], "fixed");
        stream.write_all(b"{\"ok\":true,").await.unwrap();
        stream
            .write_all(b"\"result\":{\"value\":42}}\n")
            .await
            .unwrap();
    });
    assert_eq!(
        client
            .call_with_id("inspect", json!({}), Duration::from_secs(2), "fixed")
            .await
            .unwrap(),
        json!({"value":42})
    );
    task.await.unwrap();
    assert!(
        client
            .call(
                "execute",
                json!({"data":"a".repeat(MAX_REQUEST)}),
                Duration::from_secs(1)
            )
            .await
            .unwrap_err()
            .to_string()
            .contains("256 KiB")
    );
}

#[tokio::test]
async fn timeout_never_replays_and_reports_uncertainty() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client = descriptor(listener.local_addr().unwrap().port());
    let task = tokio::spawn(async move {
        let (_stream, _) = listener.accept().await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(100), listener.accept())
                .await
                .is_err()
        );
    });
    let error = client
        .call("execute", json!({}), Duration::from_millis(25))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("outcome may be unknown"));
    assert!(!format!("{error:#}").contains("test-secret"));
    task.await.unwrap();
}

#[tokio::test]
async fn oversized_or_incomplete_responses_report_unknown_outcome() {
    for response in [b"{\"ok\":".to_vec(), vec![b'x'; MAX_RESPONSE + 1]] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let client = descriptor(listener.local_addr().unwrap().port());
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut data = [0; 1024];
            let _ = stream.read(&mut data).await;
            let _ = stream.write_all(&response).await;
        });
        assert!(
            client
                .call("inspect", json!({}), Duration::from_secs(2))
                .await
                .unwrap_err()
                .to_string()
                .contains("unknown")
        );
        task.await.unwrap();
    }
}

#[tokio::test]
async fn png_paths_reject_traversal_and_symlinks() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = descriptor(1);
    client.info.output_dir = dir.path().to_owned();
    std::fs::write(dir.path().join("image.png"), b"\x89PNG\r\n\x1a\n").unwrap();
    assert!(client.image("image.png").await.is_ok());
    assert!(client.image("../image.png").await.is_err());
    assert!(client.image("/image.png").await.is_err());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(dir.path().join("image.png"), dir.path().join("link.png"))
            .unwrap();
        assert!(client.image("link.png").await.is_err());
    }
}
