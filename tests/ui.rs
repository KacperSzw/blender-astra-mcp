use compact_mcp::client::Client;
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

struct UiHost(tempfile::TempDir);

impl Drop for UiHost {
    fn drop(&mut self) {
        let _ = std::fs::write(self.0.path().join("stop"), "");
        let deadline = Instant::now() + Duration::from_secs(10);
        while !self.0.path().join("stopped").exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

async fn wait_for(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(45);
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "UI fixture timed out waiting for {}",
            path.display()
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
#[ignore = "Opt-in workstation viewport/Undo test: see README"]
async fn real_timer_viewport_and_undo() {
    if std::env::var_os("BLENDER_TEST_UI").is_none() {
        eprintln!("Set BLENDER_TEST_UI=1 and BLENDER_TEST_WORKSPACE for the isolated UI check");
        panic!("BLENDER_TEST_UI=1 is required for the interactive test");
    }
    let exe = std::env::var_os("BLENDER_EXE").expect("BLENDER_EXE required");
    let workspace = std::env::var("BLENDER_TEST_WORKSPACE")
        .expect("Set BLENDER_TEST_WORKSPACE to this terminal's native workspace ID");
    let host = UiHost(tempfile::tempdir().unwrap());
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let log = std::fs::File::create(host.0.path().join("blender-ui.log")).unwrap();
    let status = Command::new("workstation-desktop")
        .args(["launch", "--workspace", &workspace, "--background", "--"])
        .arg(exe)
        .args([
            "--factory-startup",
            "--threads",
            "1",
            "--python-exit-code",
            "1",
            "--python",
        ])
        .arg(root.join("scripts/blender_ui_host.py"))
        .arg("--")
        .arg(host.0.path())
        .env("BLENDER_USER_CONFIG", host.0.path().join("config"))
        .stdin(Stdio::null())
        .stdout(log.try_clone().unwrap())
        .stderr(log)
        .status()
        .unwrap();
    assert!(status.success());
    let connection = host.0.path().join("connection.json");
    wait_for(&connection).await;
    let client = Client::from_path(&connection).unwrap();
    for args in [
        json!({"steps":[{"op":"primitive","name":"UITest","kind":"cube","location":[1,2,3]}]}),
        json!({"code":"bpy.data.objects['UITest'].location=(7,8,9)"}),
    ] {
        assert_eq!(
            client
                .call("execute", args, Duration::from_secs(20))
                .await
                .unwrap()["ok"],
            true
        );
    }
    let capture = client
        .call(
            "capture",
            json!({"filename":"viewport.png","view":"viewport","size":64}),
            Duration::from_secs(30),
        )
        .await
        .unwrap();
    assert!(
        client
            .image(capture["file"].as_str().unwrap())
            .await
            .is_ok()
    );
    std::fs::write(host.0.path().join("undo"), "").unwrap();
    let result = host.0.path().join("ui-result.json");
    wait_for(&result).await;
    let result: Value = serde_json::from_slice(&std::fs::read(result).unwrap()).unwrap();
    assert_eq!(
        result,
        json!({"poll":true,"location":[1.0,2.0,3.0],"timer_removed":true,"descriptor_removed":true})
    );
}
