use compact_mcp::format::{self, TEXT_BUDGET};
use serde_json::json;

#[test]
fn pagination_stops_at_whole_rows_and_preserves_identifiers() {
    let rows:Vec<_>=(0..100).map(|i|json!({"name":format!("{i:03}-{}","ż".repeat(100)),"type":"MESH","materials":["a".repeat(250),"b".repeat(250),"c".repeat(250),"d".repeat(250)]})).collect();
    let value = json!({"scene":"Scene\nwith tabs\t","total":100,"blender":"5.1.1","permissions":["write"],"objects":rows});
    let mut offset = 0;
    let mut names = Vec::new();
    while offset < 100 {
        let mut page = value.clone();
        page["objects"] = json!(&rows[offset..]);
        let text = format::result("inspect", &json!({"offset":offset}), &page);
        assert!(text.len() <= TEXT_BUDGET);
        for line in text.lines().skip(2).filter(|line| line.starts_with('"')) {
            let name: String = serde_json::from_str(line.split('\t').next().unwrap()).unwrap();
            names.push(name);
        }
        let next: Option<usize> = text.split_whitespace().find_map(|word| {
            word.strip_prefix("next_offset=")
                .map(|value| value.parse().unwrap())
        });
        if let Some(next) = next {
            assert!(next > offset);
            offset = next;
        } else {
            assert!(!text.contains("returned="));
            break;
        }
    }
    assert_eq!(names.len(), 100);
    assert_eq!(names[99], rows[99]["name"].as_str().unwrap());
}

#[test]
fn truncation_is_utf8_safe_and_keeps_failure_and_handles() {
    let text = format::result(
        "execute",
        &json!({}),
        &json!({"ok":false,"completed":1,"changed":null,"failed_index":1,"error":"failed","files":[],"results":[{"index":0,"result":{"script":"session:1","stdout":"🦀".repeat(10000)}}]}),
    );
    assert!(text.len() <= TEXT_BUDGET);
    for expected in [
        "failed_index=1",
        "completed=1",
        "partial=true",
        "script=session:1",
        "output truncated",
    ] {
        assert!(text.contains(expected), "{expected}");
    }
}

#[test]
fn receipts_keep_requested_values_and_only_new_handles() {
    let base = json!({"ok":true,"completed":1,"changed":null,"files":[]});
    assert_eq!(
        format::result("execute", &json!({"steps":[{}]}), &base),
        "ok"
    );
    let mut response = base;
    for value in [
        json!(null),
        json!(false),
        json!(0),
        json!(""),
        json!([]),
        json!({}),
    ] {
        response["results"] = json!([{"index":0,"result":{"script":"session:1","value":value}}]);
        assert_eq!(
            format::result("execute", &json!({"code":"source"}), &response),
            format!("ok\nscript=session:1\nresult: {value}")
        );
        for args in [
            json!({"script":"session:1"}),
            json!({"steps":[{"op":"python","script":"session:1"}]}),
        ] {
            assert_eq!(
                format::result("execute", &args, &response),
                format!("ok\nresult: {value}")
            );
        }
    }
    response["results"] = json!([{"index":0,"result":{"script":"session:1"}}]);
    assert_eq!(
        format::result("execute", &json!({"script":"session:1"}), &response),
        "ok"
    );
    response["results"][0]["result"]["stdout"] = json!("hello\n");
    response["results"][0]["result"]["stdout_truncated"] = json!(false);
    assert_eq!(
        format::result("execute", &json!({"script":"session:1"}), &response),
        "ok\nstdout: \"hello\\n\""
    );
    response["results"][0]["result"]["stdout_truncated"] = json!(true);
    response["results"][0]["result"]["result_preview"] = json!("");
    response["results"][0]["result"]["result_chars"] = json!(20);
    let text = format::result("execute", &json!({"script":"session:1"}), &response);
    assert!(text.contains("stdout truncated:"));
    assert!(text.contains("result truncated chars=20: \"\""));
}

#[test]
fn inspection_context_and_escaping_are_explicit() {
    let value = json!({"scene":"Scene","blender":"5.1.1","permissions":["write","python"],"total":3,"missing":["Missing"],"objects":[{"name":"Cube","location":[1.0,2.0,3.0]},{"name":"001","location":[0,0,0]},{"name":"a\tb\n\"c"}]});
    let text = format::result("inspect", &json!({}), &value);
    assert_eq!(
        text,
        "scene=Scene total=3\nmissing=1 [\"Missing\"]\nname\tlocation\nCube\t[1,2,3]\n\"001\"\t[0,0,0]\n\"a\\tb\\n\\\"c\"\t-"
    );
    let context = format::result("inspect", &json!({"context":true}), &value);
    assert!(context.starts_with("scene=Scene total=3 blender=5.1.1 permissions=write,python\n"));
}

fn script_result(value: serde_json::Value) -> String {
    format::result(
        "execute",
        &json!({"script":"session:1"}),
        &json!({"ok":true,"results":[{"index":0,"result":{"script":"session:1","value":value}}]}),
    )
}

#[test]
fn result_tables_preserve_types_keys_and_integer_precision() {
    let value = json!([
        {"name":"Cube","count":9007199254740993_u64,"value":false},
        {"value":null,"count":u64::MAX,"name":"true"},
        {"name":"null","count":-9007199254740993_i64,"value":0},
        {"name":"001","count":1.0,"value":"0"},
        {"name":"Żółw\t\n\"\\","count":1.2345678901234567,"value":""}
    ]);
    assert_eq!(
        script_result(value),
        "ok\nresult rows=5:\nname\tcount\tvalue\nCube\t9007199254740993\tfalse\n\"true\"\t18446744073709551615\tnull\n\"null\"\t-9007199254740993\t0\n\"001\"\t1.0\t\"0\"\n\"Żółw\\t\\n\\\"\\\\\"\t1.2345678901234567\t\"\""
    );
    let quoted_keys = script_result(json!([{"x\ty":1},{"x\ty":2}]));
    assert_eq!(quoted_keys, "ok\nresult rows=2:\n\"x\\ty\"\n1\n2");
    let huge = "100000000000000000000000000000000000000000000000001";
    let value = serde_json::from_str(&format!("[{{\"n\":{huge}}},{{\"n\":-{huge}}}]")).unwrap();
    assert_eq!(
        script_result(value),
        format!("ok\nresult rows=2:\nn\n{huge}\n-{huge}")
    );
    for value in [
        json!([{"x":1},{"x":2,"y":null}]),
        json!([{"x":[1,2]},{"x":[3,4]}]),
        json!([{"x":{"y":1}},{"x":{"y":2}}]),
        json!([{}, {}]),
        json!([{"x":1}]),
    ] {
        assert_eq!(script_result(value.clone()), format!("ok\nresult: {value}"));
    }
}

#[test]
fn bounded_tables_keep_whole_rows_and_metadata_precedes_payloads() {
    let rows: Vec<_> = (0..100)
        .map(|i| json!({"name":format!("{i}-{}","ż".repeat(60)),"value":"\n".repeat(100)}))
        .collect();
    let text = script_result(json!(rows));
    assert!(text.len() <= TEXT_BUDGET);
    assert!(text.ends_with("[output truncated; changes are not cancelled]"));
    let mut returned = 0;
    for line in text
        .lines()
        .skip(3)
        .take_while(|line| !line.starts_with('['))
    {
        let cells: Vec<_> = line.split('\t').collect();
        assert_eq!(cells.len(), 2);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(cells[0]).unwrap(),
            rows[returned]["name"]
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(cells[1]).unwrap(),
            rows[returned]["value"]
        );
        returned += 1;
    }
    assert!(returned > 0 && returned < 100);
    let value = json!({"ok":false,"completed":2,"failed_index":2,"error":"Missing","files":[{"file":"earlier.blend"}],"results":[{"index":0,"result":{"script":"session:1","stdout":"🦀".repeat(10000)}},{"index":1,"result":{"script":"session:2","value":42}}]});
    let text = format::result("execute", &json!({}), &value);
    assert!(text.len() <= TEXT_BUDGET);
    for expected in [
        "error failed_index=2 completed=2 partial=true\nMissing",
        "step=0 script=session:1",
        "step=1 script=session:2",
        "file=earlier.blend",
        "output truncated",
    ] {
        assert!(text.contains(expected), "{expected}");
    }
}

#[test]
fn discovery_returns_complete_contracts_and_remaining_names() {
    let legacy = format::result(
        "discover",
        &json!({"operation":"transform"}),
        &json!({"summary":"Transform object","args":{"name":"string"}}),
    );
    assert!(legacy.starts_with("transform [unknown]:"));
    let read = format::result(
        "discover",
        &json!({"operation":"rna"}),
        &json!({"summary":"Read RNA","args":{},"permission":null}),
    );
    assert!(read.starts_with("rna [read]:"));
    let mut contracts = serde_json::Map::new();
    for i in 0..16 {
        contracts.insert(format!("op{i}"),json!({"summary":"description".repeat(100),"args":{"name":"string"},"permission":"write","required":["name"]}));
    }
    let text = format::result("discover", &json!({}), &contracts.into());
    assert!(text.len() <= TEXT_BUDGET);
    assert!(text.contains("remaining:"));
    assert!(text.contains("name: string\nremaining:") || text.contains("name: string\nop"));
    assert!(text.contains("op15"));
}
