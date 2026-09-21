mod support;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use support::{Blender, Mcp, text};

#[tokio::test]
async fn real_stdio_schemas_errors_and_image() {
    let Some(b) = Blender::start() else { return };
    let mut mcp = Mcp::start(&b.connection).await;
    let tools = mcp.request("tools/list", json!({})).await;
    let tools = tools["result"]["tools"].as_array().unwrap();
    assert_eq!(
        tools
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["inspect", "discover", "execute", "capture"]
    );
    assert!(tools.iter().all(|t| t.get("outputSchema").is_none()));
    let execute = tools.iter().find(|t| t["name"] == "execute").unwrap();
    for key in [
        "code",
        "script",
        "steps",
        "params",
        "max_output",
        "dry_run",
        "timeout",
    ] {
        assert!(execute["inputSchema"]["properties"].get(key).is_some());
    }
    for (name, args) in [
        ("discover", json!({"operation":["primitive","python"]})),
        (
            "execute",
            json!({"steps":[{"op":"primitive","kind":"cube","name":"Mcp"}]}),
        ),
        ("inspect", json!({"names":["Mcp"],"fields":["location"]})),
    ] {
        let result = mcp.call(name, args).await;
        assert_ne!(result["isError"], true, "{result}");
        assert!(result.get("structuredContent").is_none());
        assert_eq!(result["content"].as_array().unwrap().len(), 1);
        assert!(text(&result).len() <= 8192);
    }
    let empty = mcp
        .call(
            "execute",
            json!({"steps":[{"op":"transform","name":"Mcp","location":[0,0,0]}]}),
        )
        .await;
    assert_eq!(text(&empty), "ok");
    let script = mcp
        .call(
            "execute",
            json!({"code":"result=params['value']","params":{"value":null}}),
        )
        .await;
    assert_ne!(script["isError"], true);
    assert!(text(&script).ends_with("\nresult: null"));
    assert!(!text(&script).contains("step="));
    let handle = text(&script)
        .split_whitespace()
        .find_map(|word| word.strip_prefix("script="))
        .unwrap()
        .to_owned();
    let reused = mcp
        .call("execute", json!({"script":handle,"params":{"value":false}}))
        .await;
    assert_eq!(text(&reused), "ok\nresult: false");
    let huge = "100000000000000000000000000000000000000000000000001";
    let args: Value = serde_json::from_str(&format!(
        "{{\"code\":\"result=params['number']\",\"params\":{{\"number\":{huge}}}}}"
    ))
    .unwrap();
    let precise = mcp.call("execute", args).await;
    assert_ne!(precise["isError"], true, "{precise}");
    assert!(text(&precise).ends_with(&format!("result: {huge}")));
    let dry = mcp
        .call("execute", json!({"code":"result=1","dry_run":true}))
        .await;
    assert_eq!(
        text(&dry),
        "validated=1 executed=0 scope=syntax/permissions"
    );
    for args in [
        json!({"code":"pass","steps":[]}),
        json!({"code":"return 42"}),
        json!({"script":"expired"}),
    ] {
        let result = mcp.call("execute", args).await;
        assert_eq!(result["isError"], true);
        assert!(text(&result).starts_with("error:"));
    }
    for context in [false, true] {
        let result = mcp
            .call(
                "inspect",
                json!({"names":["Mcp"],"fields":["location"],"context":context}),
            )
            .await;
        assert_eq!(text(&result).contains("permissions="), context);
        assert_eq!(text(&result).contains("blender="), context);
    }
    for steps in [
        json!([{"op":"unknown"}]),
        json!([{"op":"transform","name":"Mcp","location":[4,5,6]},{"op":"transform","name":"Missing"}]),
    ] {
        let result = mcp.call("execute", json!({"steps":steps})).await;
        assert_eq!(result["isError"], true, "{result}");
        assert!(result.get("structuredContent").is_none());
        assert!(text(&result).contains("error"));
    }
    let partial = mcp.call("execute", json!({"steps":[{"op":"python","code":"result=42"},{"op":"save","filename":"before-failure.blend"},{"op":"transform","name":"Missing"}]})).await;
    assert_eq!(partial["isError"], true);
    for expected in [
        "failed_index=2 completed=2 partial=true",
        "step=0 script=",
        "file=before-failure.blend",
        "step=0 result: 42",
    ] {
        assert!(text(&partial).contains(expected), "{partial}");
    }
    assert_eq!(
        b.call("inspect", json!({"names":["Mcp"]})).await["objects"][0]["location"],
        json!([4.0, 5.0, 6.0])
    );
    b.execute(json!([{"op":"camera","name":"Cam","location":[8,-8,8],"target":[0,0,0]}]))
        .await;
    let image = mcp.call("capture", json!({"size":64})).await;
    assert_eq!(image["content"].as_array().unwrap().len(), 1);
    assert_eq!(image["content"][0]["type"], "image");
    assert_eq!(image["content"][0]["mimeType"], "image/png");
    assert!(
        STANDARD
            .decode(image["content"][0]["data"].as_str().unwrap())
            .unwrap()
            .starts_with(b"\x89PNG\r\n\x1a\n")
    );
    mcp.close().await;
    let mut denied = Mcp::start(&b.scratch.path().join("readonly.json")).await;
    assert_eq!(
        denied.call("execute", json!({"code":"pass"})).await["isError"],
        true
    );
    denied.close().await;
}

#[tokio::test]
async fn cli_keeps_json_stdin_and_failure_exit_status() {
    let Some(b) = Blender::start() else { return };
    let run = |method: &str, params: &Value| {
        std::process::Command::new(env!("CARGO_BIN_EXE_blender-compact"))
            .args([method, "--params", &params.to_string(), "--connection"])
            .arg(&b.connection)
            .output()
            .unwrap()
    };
    let good = run("inspect", &json!({"limit":1}));
    assert!(good.status.success());
    assert!(serde_json::from_slice::<Value>(&good.stdout).unwrap()["objects"].is_array());
    let code = run(
        "execute",
        &json!({"code":"result=params['value']","params":{"value":42}}),
    );
    assert!(code.status.success());
    let code: Value = serde_json::from_slice(&code.stdout).unwrap();
    assert_eq!(code["completed"], 1);
    assert!(code["changed"].is_null());
    assert_eq!(code["results"][0]["result"]["value"], 42);
    let handle = &code["results"][0]["result"]["script"];
    let reused = run(
        "execute",
        &json!({"script":handle,"params":{"value":false}}),
    );
    assert!(reused.status.success());
    let reused: Value = serde_json::from_slice(&reused.stdout).unwrap();
    assert_eq!(&reused["results"][0]["result"]["script"], handle);
    assert_eq!(reused["results"][0]["result"]["value"], false);
    let partial = run(
        "execute",
        &json!({"steps":[{"op":"transform","name":"Missing"}]}),
    );
    assert!(!partial.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&partial.stdout).unwrap()["ok"],
        false
    );
    let rejected = run("execute", &json!({"steps":[{"op":"unknown"}]}));
    assert!(!rejected.status.success());
    assert!(serde_json::from_slice::<Value>(&rejected.stderr).unwrap()["error"].is_string());
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_blender-compact"))
        .args(["discover", "--params", "-", "--connection"])
        .arg(&b.connection)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"{\"operation\":\"array\"}")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(serde_json::from_slice::<Value>(&output.stdout).unwrap()["args"]["count"].is_string());
}
