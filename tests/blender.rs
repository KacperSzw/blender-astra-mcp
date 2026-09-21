mod support;
use compact_mcp::client::{Client, MAX_REQUEST};
use serde_json::{Value, json};
use std::time::Duration;
use support::Blender;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpStream,
};

#[tokio::test]
async fn all_operations_and_existing_scene_editing() {
    let Some(b) = Blender::start() else { return };
    let catalog = b.call("discover", json!({})).await;
    assert_eq!(catalog.as_object().unwrap().len(), 16);
    let contracts = b
        .call(
            "discover",
            json!({"operation":["primitive","python","rna"]}),
        )
        .await;
    assert_eq!(contracts["primitive"]["required"], json!(["name", "kind"]));
    assert_eq!(contracts["python"]["permission"], "python");
    assert!(contracts["rna"]["permission"].is_null());
    let result = b.execute(json!([
        {"op":"transform","name":"UnmanagedSentinel","location":[3,2,1]},
        {"op":"primitive","name":"Seed","kind":"cube"},
        {"op":"material","name":"Red","color":[1,0,0,1]},
        {"op":"material","name":"Blue","color":[0,0,1,1]},
        {"op":"assign_material","name":"Seed","material":"Red"},
        {"op":"array","name":"Seed","count":2,"offset":[2,0,0],"prefix":"Copy"},
        {"op":"assign_material","name":"Copy_001","material":"Blue"},
        {"op":"modifier","name":"Seed","modifier":"Bevel","type":"BEVEL","properties":{"width":0.2,"segments":2}},
        {"op":"properties","name":"Seed","values":{"hide_render":false}},
        {"op":"keyframes","name":"Seed","data_path":"location","keys":[[1,[0,0,0]],[10,[0,0,2]]]},
        {"op":"frame","frame":10,"start":1,"end":10},
        {"op":"rna","type":"BevelModifier","contains":"width"},
        {"op":"camera","name":"TestCamera","location":[8,-8,8],"target":[0,0,0]},
        {"op":"light","name":"Key","location":[0,-5,8],"energy":500},
        {"op":"save","filename":"scene.blend"}
    ])).await;
    assert_eq!(result["ok"], true, "{result}");
    assert!(b.scratch.path().join("exports/scene.blend").is_file());
    let inspect = b
        .call(
            "inspect",
            json!({"names":["Seed","Copy_001","Copy_002","UnmanagedSentinel"]}),
        )
        .await;
    let rows = inspect["objects"].as_array().unwrap();
    let row = |name| rows.iter().find(|v| v["name"] == name).unwrap();
    assert_eq!(row("Seed")["materials"], json!(["Red"]));
    assert_eq!(row("Seed")["location"], json!([0.0, 0.0, 2.0]));
    assert_eq!(row("Copy_001")["materials"], json!(["Blue"]));
    assert_eq!(row("Copy_002")["location"], json!([4.0, 0.0, 0.0]));
    assert_eq!(row("UnmanagedSentinel")["location"], json!([3.0, 2.0, 1.0]));
    let script = b.execute(json!([{"op":"python","code":"result=bpy.data.objects['Seed'].modifiers['Bevel'].width"}])).await;
    assert!(script["changed"].is_null());
    assert!((script["results"][0]["result"]["value"].as_f64().unwrap() - 0.2).abs() < 0.0001);
    let png = b
        .call("capture", json!({"size":64,"filename":"test.png"}))
        .await;
    assert!(
        b.client
            .image(png["file"].as_str().unwrap())
            .await
            .unwrap()
            .starts_with(b"\x89PNG")
    );
    let render = b.execute(json!([
        {"op":"properties","target":"render","values":{"resolution_x":16,"resolution_y":16,"resolution_percentage":100,"engine":"CYCLES"}},
        {"op":"render","filename":"small"}
    ])).await;
    assert_eq!(render["ok"], true, "{render}");
    assert!(render["results"][0]["result"]["total"].as_u64().unwrap() > 0);
    assert_eq!(
        b.execute(json!([{"op":"delete","names":["Copy_001","Copy_002"],"confirm":true}]))
            .await["changed"],
        2
    );
}

#[tokio::test]
async fn prevalidation_permissions_dry_run_and_partial_failures() {
    let Some(b) = Blender::start() else { return };
    let create = json!({"op":"primitive","name":"NeverCreated","kind":"cube"});
    let invalid = [
        json!({"op":"unknown"}),
        json!({"op":"modifier","name":"X"}),
        json!({"op":"keyframes","name":"X","data_path":"location","keys":[]}),
        json!({"op":"frame","frame":true}),
        json!({"op":"rna","type":"Object","limit":0}),
        json!({"op":"properties","values":[]}),
        json!({"op":"python","code":"if invalid:"}),
        json!({"op":"python","code":"pass","script":"no"}),
        json!({"op":"python","script":"missing"}),
        json!({"op":"array","name":"X","count":201,"prefix":"X","offset":[1,0,0]}),
        json!({"op":"delete","names":["X"],"confirm":false}),
        json!({"op":"render","filename":"render","animation":"yes"}),
    ];
    for step in invalid {
        assert!(
            b.client
                .call(
                    "execute",
                    json!({"steps":[create.clone(),step]}),
                    Duration::from_secs(10)
                )
                .await
                .is_err()
        );
    }
    assert_eq!(
        b.call("inspect", json!({"names":["NeverCreated"]})).await["total"],
        0
    );
    let dry = b
        .call(
            "execute",
            json!({"steps":[create,{"op":"python","code":"result=1"}],"dry_run":true}),
        )
        .await;
    assert_eq!(dry["executed"], 0);
    assert_eq!(
        b.call("inspect", json!({"names":["NeverCreated"]})).await["total"],
        0
    );
    let status = b.execute(json!([{"op":"python","code":"import compact_blender\nresult=len(compact_blender._server.engine.snippets.entries)"}])).await;
    assert_eq!(status["results"][0]["result"]["value"], 1); // Only this execution entered the cache.
    let readonly = Client::from_path(&b.scratch.path().join("readonly.json")).unwrap();
    for step in [
        json!({"op":"python","code":"pass"}),
        json!({"op":"save","filename":"denied.blend"}),
        json!({"op":"primitive","kind":"cube","name":"Denied"}),
        json!({"op":"render","filename":"denied"}),
        json!({"op":"delete","names":["X"],"confirm":true}),
    ] {
        assert!(
            readonly
                .call("execute", json!({"steps":[step]}), Duration::from_secs(5))
                .await
                .unwrap_err()
                .to_string()
                .contains("Disabled permission")
        );
    }
    assert!(
        readonly
            .call("capture", json!({}), Duration::from_secs(5))
            .await
            .is_err()
    );
    assert!(
        readonly
            .call(
                "execute",
                json!({"steps":[{"op":"rna","type":"Object","limit":1}]}),
                Duration::from_secs(5)
            )
            .await
            .is_ok()
    );
    let failed = b
        .execute(json!([
            {"op":"primitive","name":"Partial","kind":"cube"},
            {"op":"frame","frame":3},
            {"op":"save","filename":"partial.blend"},
            {"op":"python","code":"result=42"},
            {"op":"transform","name":"Missing","location":[0,0,0]}
        ]))
        .await;
    assert_eq!(failed["completed"], 4);
    assert_eq!(failed["failed_index"], 4);
    assert_eq!(failed["atomic"], false);
    assert!(failed["changed"].is_null());
    assert_eq!(failed["results"][1]["result"]["value"], 42);
    assert_eq!(failed["files"][0]["file"], "partial.blend");
    assert_eq!(
        b.call("inspect", json!({"names":["Partial"]})).await["total"],
        1
    );
}

#[tokio::test]
async fn inspection_filters_projection_and_paging() {
    let Some(b) = Blender::start() else { return };
    b.execute(json!([{"op":"primitive","name":"Seed","kind":"cube"},{"op":"array","name":"Seed","count":7,"prefix":"Page","offset":[1,0,0]}])).await;
    let mut names = Vec::new();
    let mut offset = 0;
    loop {
        let page = b
            .call(
                "inspect",
                json!({"match":"Page_*","fields":["location"],"limit":2,"offset":offset}),
            )
            .await;
        assert_eq!(page["total"], 7);
        for row in page["objects"].as_array().unwrap() {
            assert_eq!(row.as_object().unwrap().len(), 2);
            names.push(row["name"].clone());
        }
        match page["next_offset"].as_u64() {
            Some(next) => offset = next,
            None => break,
        }
    }
    assert_eq!(names.len(), 7);
    names.sort_by_key(Value::to_string);
    names.dedup();
    assert_eq!(names.len(), 7);
    assert_eq!(
        b.call("inspect", json!({"match":"page_*"})).await["total"],
        0
    );
    assert_eq!(
        b.call("inspect", json!({"names":["Missing","Seed"]})).await["missing"],
        json!(["Missing"])
    );
    for args in [
        json!({"names":["Seed"],"match":"*"}),
        json!({"fields":["unknown"]}),
        json!({"offset":-1}),
        json!({"limit":101}),
        json!({"context":"yes"}),
    ] {
        assert!(
            b.client
                .call("inspect", args, Duration::from_secs(5))
                .await
                .is_err()
        );
    }
}

#[tokio::test]
async fn direct_code_validates_before_writes_and_shares_batch_semantics() {
    let Some(b) = Blender::start() else { return };
    let source = "bpy.context.scene.frame_set(77)\nresult=params['value']";
    let validated = b
        .call(
            "execute",
            json!({"code":source,"params":{"value":42},"dry_run":true}),
        )
        .await;
    assert_eq!(validated["validated"], 1);
    assert_eq!(validated["executed"], 0);
    let state = b.call("execute", json!({"code":"import compact_blender\nresult=[bpy.context.scene.frame_current,len(compact_blender._server.engine.snippets.entries)]"})).await;
    assert_eq!(state["results"][0]["result"]["value"], json!([1, 1]));
    for (args, hint) in [
        (json!({}), "Exactly one"),
        (json!({"code":source,"script":"missing"}), "Exactly one"),
        (json!({"code":source,"steps":[]}), "Exactly one"),
        (json!({"code":null,"script":"missing"}), "Exactly one"),
        (
            json!({"steps":[{"op":"frame","frame":77}],"params":{}}),
            "inside each Python step",
        ),
        (
            json!({"steps":[{"op":"frame","frame":77}],"max_output":0}),
            "inside each Python step",
        ),
        (json!({"code":null}), "must be a string"),
        (
            json!({"code":source,"params":null}),
            "params must be an object",
        ),
        (json!({"code":source,"max_output":true}), "max_output"),
        (json!({"code":source,"max_output":32001}), "max_output"),
        (json!({"code":source,"dry_run":1}), "dry_run"),
        (
            json!({"code":source,"unexpected":1}),
            "Unknown execute arguments",
        ),
        (
            json!({"code":"bpy.context.scene.frame_set(77)\nreturn 42"}),
            "assign result",
        ),
        (json!({"script":"expired"}), "expired"),
    ] {
        let error = b
            .client
            .call("execute", args, Duration::from_secs(5))
            .await
            .unwrap_err();
        assert!(error.to_string().contains(hint), "{error:#}");
    }
    let current = b
        .call(
            "execute",
            json!({"code":"result=bpy.context.scene.frame_current"}),
        )
        .await;
    assert_eq!(current["results"][0]["result"]["value"], 1);
    let result = b
        .call("execute", json!({"code":source,"params":{"value":42}}))
        .await;
    assert_eq!(result["completed"], 1);
    assert!(result["changed"].is_null());
    assert_eq!(result["results"][0]["result"]["value"], 42);
    let handle = &result["results"][0]["result"]["script"];
    let readonly = Client::from_path(&b.scratch.path().join("readonly.json")).unwrap();
    for args in [
        json!({"code":"pass"}),
        json!({"script":handle}),
        json!({"code":"pass","dry_run":true}),
    ] {
        let error = readonly
            .call("execute", args, Duration::from_secs(5))
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("Disabled permission"),
            "{error:#}"
        );
    }
    let reused = b
        .call("execute", json!({"script":handle,"params":{"value":false}}))
        .await;
    assert_eq!(reused["results"][0]["result"]["value"], false);
    assert_eq!(&reused["results"][0]["result"]["script"], handle);
    let failed = b.call("execute", json!({"code":"bpy.context.scene.frame_set(88)\nprint('before')\nraise ValueError('after write')"})).await;
    assert_eq!(failed["ok"], false);
    assert_eq!(failed["completed"], 0);
    assert_eq!(failed["failed_index"], 0);
    assert_eq!(failed["atomic"], false);
    assert_eq!(failed["results"][0]["result"]["stdout"], "before\n");
    let current = b
        .call(
            "execute",
            json!({"code":"result=bpy.context.scene.frame_current"}),
        )
        .await;
    assert_eq!(current["results"][0]["result"]["value"], 88);
    let huge = b.call("execute", json!({"code":"result=10**50+1"})).await;
    assert_eq!(
        huge["results"][0]["result"]["value"].to_string(),
        "100000000000000000000000000000000000000000000000001"
    );
}

#[tokio::test]
async fn snippets_reuse_eviction_restart_and_output_budgets() {
    let Some(b) = Blender::start() else { return };
    let first = b
        .call(
            "execute",
            json!({"code":"result=params['x']*2","params":{"x":3}}),
        )
        .await;
    let handle = first["results"][0]["result"]["script"]
        .as_str()
        .unwrap()
        .to_owned();
    let reuse = b
        .call("execute", json!({"script":handle,"params":{"x":5}}))
        .await;
    assert_eq!(reuse["results"][0]["result"]["value"], 10);
    let readonly = Client::from_path(&b.scratch.path().join("readonly.json")).unwrap();
    assert!(
        readonly
            .call(
                "execute",
                json!({"steps":[{"op":"python","script":handle}]}),
                Duration::from_secs(5)
            )
            .await
            .unwrap_err()
            .to_string()
            .contains("Disabled permission")
    );
    for _ in 0..64 {
        assert_eq!(
            b.execute(json!([{"op":"python","code":"pass"}])).await["ok"],
            true
        );
    }
    b.rejects(
        json!([{"op":"python","script":handle,"params":{"x":2}}]),
        "expired",
    )
    .await;
    let fresh = b
        .execute(json!([{"op":"python","code":"result=params['x']*2","params":{"x":3}}]))
        .await;
    let fresh_handle = fresh["results"][0]["result"]["script"].clone();
    // The byte budget can evict entries before the 64-entry limit is reached.
    let source = format!("pass\n#{}", "x".repeat(190_000));
    let mut large_handle = Value::Null;
    for i in 0..6 {
        let result = b.execute(json!([{"op":"python","code":source}])).await;
        assert_eq!(result["ok"], true);
        if i == 0 {
            large_handle = result["results"][0]["result"]["script"].clone();
        }
    }
    b.rejects(json!([{"op":"python","script":large_handle}]), "expired")
        .await;
    let cache = b.execute(json!([{"op":"python","code":"import compact_blender\nresult=compact_blender._server.engine.snippets.source_bytes"}])).await;
    assert!(cache["results"][0]["result"]["value"].as_u64().unwrap() <= 1024 * 1024);
    let steps: Vec<_> = (0..8).map(|_| json!({"op":"python","code":"print('🦀'*20000)\nresult='ż'*20000","max_output":32000})).collect();
    let large = b.execute(json!(steps)).await;
    assert_eq!(large["completed"], 8);
    let mut bytes = 0;
    for r in large["results"].as_array().unwrap() {
        let value = &r["result"];
        bytes += value["stdout"].as_str().unwrap_or("").len()
            + value["result_preview"].as_str().unwrap_or("").len();
        assert_eq!(value["stdout_truncated"], true);
        assert_eq!(value["result_truncated"], true);
    }
    assert!(bytes <= 32768);
    assert_eq!(
        b.execute(json!([{"op":"python","code":"result=42","max_output":0}]))
            .await["results"][0]["result"]["result_truncated"],
        true
    );
    for code in [
        "result=bpy.context.scene",
        "result=float('nan')",
        "bpy.context.scene.frame_set(23)\nprint('before')\nraise ValueError('failure')",
    ] {
        let failed = b.execute(json!([{"op":"python","code":code}])).await;
        assert_eq!(failed["ok"], false);
        assert_eq!(failed["completed"], 0);
        assert!(failed["changed"].is_null());
        assert!(failed["results"][0]["result"]["script"].is_string());
    }
    drop(b);
    let b = Blender::start().unwrap();
    b.rejects(
        json!([{"op":"python","script":fresh_handle,"params":{"x":3}}]),
        "expired",
    )
    .await;
}

async fn wire(b: &Blender, bytes: &[u8]) -> Value {
    let mut conn = TcpStream::connect(("127.0.0.1", b.client.info.port))
        .await
        .unwrap();
    conn.write_all(bytes).await.unwrap();
    let mut line = String::new();
    tokio::time::timeout(
        Duration::from_secs(10),
        BufReader::new(conn).read_line(&mut line),
    )
    .await
    .unwrap()
    .unwrap();
    serde_json::from_str(&line).unwrap()
}

#[tokio::test]
async fn addon_authentication_replay_limits_and_lifecycle() {
    let Some(b) = Blender::start() else { return };
    assert_eq!(
        wire(&b, b"not JSON\n").await["error"],
        "Invalid JSON request"
    );
    assert_eq!(
        wire(&b, &vec![b'x'; MAX_REQUEST + 1]).await["error"],
        "Request too large"
    );
    assert_eq!(
        wire(
            &b,
            b"{\"id\":\"a\",\"token\":\"wrong\",\"method\":\"execute\"}\n"
        )
        .await["error"],
        "Unauthorized"
    );
    let steps = json!({"steps":[{"op":"primitive","name":"Replay","kind":"cube"}]});
    let result = b
        .client
        .call_with_id("execute", steps.clone(), Duration::from_secs(5), "same")
        .await
        .unwrap();
    assert_eq!(
        b.client
            .call_with_id("execute", steps, Duration::from_secs(5), "same")
            .await
            .unwrap(),
        result
    );
    assert!(
        b.client
            .call_with_id("inspect", json!({}), Duration::from_secs(5), "same")
            .await
            .unwrap_err()
            .to_string()
            .contains("reused")
    );
    for i in 0..140 {
        b.client
            .call_with_id(
                "inspect",
                json!({"limit":1}),
                Duration::from_secs(5),
                &format!("read-{i}"),
            )
            .await
            .unwrap();
    }
    let cache=b.execute(json!([{"op":"python","code":"import compact_blender\nresult=len(compact_blender._server.cache)"}])).await;
    assert_eq!(cache["results"][0]["result"]["value"], 128);
    let connection = b.connection.clone();
    std::fs::write(b.scratch.path().join("stop"), "").unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!connection.exists());
}
