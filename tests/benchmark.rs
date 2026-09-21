mod support;
use compact_mcp::server;
use serde_json::{Value, json};
use support::{Blender, Mcp};
use tiktoken_rs::CoreBPE;

fn count(enc: &CoreBPE, text: &str) -> usize {
    enc.encode_ordinary(text).len()
}

fn measure(enc: &CoreBPE, instructions: &str, tools: &Value, calls: &[Value]) -> Value {
    let schemas = count(enc, &tools.to_string());
    let instructions = count(enc, instructions);
    let mut arguments = 0;
    let mut results = 0;
    let mut wire = 0;
    let mut discovery_arguments = 0;
    let mut discovery_results = 0;
    for call in calls {
        let input = count(enc, &call["arguments"].to_string());
        arguments += input;
        let mut output = 0;
        for content in call["result"]["content"].as_array().unwrap() {
            output += count(enc, content["text"].as_str().expect("text-only benchmark"));
        }
        results += output;
        if call["name"] == "discover" {
            discovery_arguments += input;
            discovery_results += output;
        }
        wire += count(enc, &json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":call["name"],"arguments":call["arguments"]}}).to_string());
        wire += count(
            enc,
            &json!({"jsonrpc":"2.0","id":1,"result":call["result"]}).to_string(),
        );
    }
    json!({"calls":calls.len(),"schemas":schemas,"instructions":instructions,
        "arguments":arguments,"text_results":results,
        "discovery_arguments":discovery_arguments,"discovery_results":discovery_results,
        "non_discovery_arguments":arguments-discovery_arguments,"non_discovery_results":results-discovery_results,
        "images":0,
        "warm_text_payload":arguments+results,"cold_text_payload":schemas+instructions+arguments+results,
        "normalized_mcp_wire":wire,"envelopes_and_structured_content":wire-arguments-results})
}

#[test]
fn advertised_schema_budget() {
    let current = serde_json::to_value(server::tools()).unwrap();
    for (enc, ceiling) in [
        (tiktoken_rs::cl100k_base().unwrap(), 525),
        (tiktoken_rs::o200k_base().unwrap(), 540),
    ] {
        let new = count(&enc, &current.to_string()) + count(&enc, server::INSTRUCTIONS);
        assert!(
            new <= ceiling,
            "Schema/instruction budget exceeded: {new} > {ceiling}"
        );
    }
}

#[tokio::test]
async fn matched_cold_and_warm_transcripts() {
    let Some(b) = Blender::start() else { return };
    let baseline: Value = serde_json::from_str(include_str!("fixtures/v0.3-mcp.json")).unwrap();
    let transcripts: Value =
        serde_json::from_str(include_str!("fixtures/v0.3-transcripts.json")).unwrap();
    assert_eq!(b.execute(transcripts["setup"].clone()).await["ok"], true);
    let mut mcp = Mcp::start(&b.connection).await;
    let schemas = mcp.request("tools/list", json!({})).await["result"]["tools"].clone();
    let mut report = json!({
        "baseline":"Recorded v0.3 working-tree schemas and MCP transcripts in tests/fixtures/v0.3-*.json","version":env!("CARGO_PKG_VERSION"),
        "blender":b.call("inspect",json!({"limit":1})).await["blender"],
        "method":"Recorded v0.3 MCP transcripts versus actual v0.4 stdio calls; same isolated factory scene and verified task outcomes",
        "definitions":{"cold_text_payload":"instructions + tools/list schemas + arguments + content text; each counted separately",
            "warm_text_payload":"arguments + content text; excludes startup context",
            "normalized_mcp_wire":"JSON-RPC calls/results normalized with serde_json and fixed request ID; includes structuredContent and envelopes"},
        "excluded":["provider-specific tool loading","conversation/reasoning","cached-token billing","images","scene setup"],
        "limitation":"Offline payload measurements, not provider usage or billed savings. Cold code assumes discovery is needed in v0.3; known_code measures an agent already knowing the old Python contract. Random session handles affect counts slightly.",
        "equivalent_task_outcomes":true,"encodings":{}
    });
    let mut current_cases = serde_json::Map::new();
    for (scenario, legacy) in transcripts["scenarios"].as_object().unwrap() {
        let mut calls = Vec::new();
        let mut handle = None;
        let legacy = legacy.as_array().unwrap();
        let code_first = matches!(
            scenario.as_str(),
            "cold_code" | "known_code" | "snippets" | "uniform_records"
        );
        for old in legacy {
            let name = old["name"].as_str().unwrap();
            if code_first && name == "discover" {
                continue;
            }
            let mut args = old["arguments"].clone();
            if code_first && name == "execute" {
                args = args["steps"][0].clone();
                args.as_object_mut().unwrap().remove("op");
                if args.get("script").is_some() {
                    args["script"] = json!(handle.as_ref().expect("earlier source receipt"));
                }
            }
            let result = mcp.call(name, args.clone()).await;
            if code_first && name == "execute" {
                assert_ne!(result["isError"], true, "{result}");
                let text = support::text(&result);
                let parsed = text
                    .split_whitespace()
                    .find_map(|word| word.strip_prefix("script="));
                if args.get("code").is_some() {
                    handle = Some(parsed.expect("new source receipt").to_owned());
                } else {
                    assert!(parsed.is_none(), "reused handle must not be echoed");
                }
                if scenario != "uniform_records" {
                    assert!(text.contains("\"updated\":2"), "{text}");
                    let actual = b
                        .call(
                            "inspect",
                            json!({"names":["BenchSeed","Bench_049"],"fields":["location"]}),
                        )
                        .await;
                    for row in actual["objects"].as_array().unwrap() {
                        assert_eq!(row["location"][2].as_f64(), args["params"]["z"].as_f64());
                    }
                } else {
                    let original = support::text(&old["result"])
                        .lines()
                        .find_map(|line| line.strip_prefix("step=0 {\"value\":"))
                        .unwrap();
                    let original: Value =
                        serde_json::from_str(original.strip_suffix('}').unwrap()).unwrap();
                    let current = text
                        .split_once("result rows=20:\nname\tvertices\tvisible\n")
                        .unwrap()
                        .1;
                    let expected = original
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|row| {
                            format!(
                                "{}\t{}\t{}",
                                row["name"].as_str().unwrap(),
                                row["vertices"],
                                row["visible"]
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    assert_eq!(current, expected);
                }
            }
            calls.push(json!({"name":name,"arguments":args,"result":result}));
        }
        match scenario.as_str() {
            "cold_edit" | "recovery" => {
                assert_eq!(
                    b.call("inspect", json!({"names":["BenchSeed"]})).await["objects"][0]["location"],
                    json!([1.0, 2.0, 3.0])
                );
                if scenario == "recovery" {
                    assert_eq!(calls[0]["result"]["isError"], true);
                }
            }
            "transforms" => {
                let names: Vec<_> = legacy[0]["arguments"]["steps"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|s| s["name"].clone())
                    .collect();
                let actual = b.call("inspect", json!({"names":names,"limit":100})).await;
                for step in legacy[0]["arguments"]["steps"].as_array().unwrap() {
                    let row = actual["objects"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|r| r["name"] == step["name"])
                        .unwrap();
                    let positions: Vec<_> = step["location"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_f64().unwrap())
                        .collect();
                    assert_eq!(row["location"], json!(positions));
                }
            }
            "inspection" => {
                let actual = b.call("inspect", legacy[0]["arguments"].clone()).await;
                assert_eq!(actual["total"], 2);
                assert_eq!(actual["objects"][0]["location"], json!([0.0, 3.0, 1.0]));
                assert_eq!(actual["objects"][1]["location"], json!([49.0, 3.0, 1.0]));
            }
            _ => {}
        }
        current_cases.insert(scenario.clone(), json!(calls));
    }
    for (encoding, enc) in [
        ("cl100k_base", tiktoken_rs::cl100k_base().unwrap()),
        ("o200k_base", tiktoken_rs::o200k_base().unwrap()),
    ] {
        for (scenario, current) in &current_cases {
            let old = measure(
                &enc,
                baseline["instructions"].as_str().unwrap(),
                &baseline["tools"],
                transcripts["scenarios"][scenario].as_array().unwrap(),
            );
            let new = measure(
                &enc,
                server::INSTRUCTIONS,
                &schemas,
                current.as_array().unwrap(),
            );
            eprintln!(
                "{encoding} {scenario}: cold {} -> {}, warm {} -> {}",
                old["cold_text_payload"],
                new["cold_text_payload"],
                old["warm_text_payload"],
                new["warm_text_payload"]
            );
            if scenario == "cold_code" {
                assert!(
                    new["cold_text_payload"].as_u64() <= old["cold_text_payload"].as_u64(),
                    "Cold code regression: {old} -> {new}"
                );
            }
            if scenario == "snippets" {
                assert!(
                    new["warm_text_payload"].as_u64().unwrap() * 100
                        <= old["warm_text_payload"].as_u64().unwrap() * 75,
                    "{scenario}: {old} -> {new}"
                );
            }
            if scenario == "uniform_records" {
                assert!(
                    new["text_results"].as_u64().unwrap() * 100
                        <= old["text_results"].as_u64().unwrap() * 65,
                    "record output regression: {old} -> {new}"
                );
            }
            report["encodings"][encoding][scenario] = json!({"v0.3":old,"v0.4":new});
        }
    }
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("artifacts");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("benchmark-v0.4.json"),
        serde_json::to_string_pretty(&report).unwrap() + "\n",
    )
    .unwrap();
    std::fs::write(
        directory.join("transcripts-v0.4.json"),
        serde_json::to_string_pretty(&current_cases).unwrap() + "\n",
    )
    .unwrap();
    std::fs::write(
        directory.join("schemas-v0.4.json"),
        serde_json::to_string_pretty(&json!({"instructions":server::INSTRUCTIONS,"tools":schemas}))
            .unwrap()
            + "\n",
    )
    .unwrap();
    mcp.close().await;
}
