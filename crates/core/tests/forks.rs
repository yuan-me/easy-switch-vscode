use easy_switch_core::{
    Settings, history,
    journal::{self, Journal},
    sessions,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tokio_util::sync::CancellationToken;

fn crypto() {
    #[cfg(not(windows))]
    easy_switch_core::crypto::set_session_key([42; 32]).unwrap();
}
fn body(bytes: &[u8]) -> &[u8] {
    &bytes[bytes.iter().position(|b| *b == b'\n').unwrap() + 1..]
}
fn rows(depth: usize, modern: bool) -> Vec<Value> {
    let ids: Vec<_> = (0..=depth)
        .map(|_| uuid::Uuid::new_v4().to_string())
        .collect();
    let mut out = vec![];
    for (i, id) in ids.iter().enumerate() {
        let mut p = json!({"id":id,"cwd":"/project","model_provider":"openai","source":"vscode"});
        if i < depth {
            p["forked_from_id"] = ids[i + 1].clone().into();
        }
        if modern {
            p["session_id"] = uuid::Uuid::new_v4().to_string().into();
            p["history_mode"] = "legacy".into();
        }
        out.push(json!({"type":"session_meta","payload":p}));
    }
    out.push(json!({"type":"response_item","payload":{"type":"function_call_output","call_id":"preserve","output":"keep tools and text"}}));
    out.push(json!({"type":"event_msg","payload":{"type":"user_message","message":"fork title"}}));
    out
}
fn write(home: &Path, rows: &[Value]) -> PathBuf {
    fs::create_dir_all(home.join("sessions")).unwrap();
    let p = home.join(format!(
        "sessions/rollout-2026-01-01T00-00-00-{}.jsonl",
        rows[0]["payload"]["id"].as_str().unwrap()
    ));
    fs::write(
        &p,
        rows.iter()
            .map(|r| format!("{r}  \r\n"))
            .collect::<String>(),
    )
    .unwrap();
    p
}
fn scan(s: &Settings) -> Vec<sessions::Session> {
    let result = sessions::Scanner::default()
        .scan(s, &CancellationToken::new())
        .unwrap();
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    result.sessions
}

#[test]
fn ordinary_and_nested_forks_preserve_ancestry_bytes_and_rollback() {
    crypto();
    for modern in [false, true] {
        for depth in [1, 2, 4] {
            let tmp = tempfile::tempdir().unwrap();
            let s = Settings {
                codex_home: tmp.path().join("home"),
                ..Default::default()
            };
            let r = rows(depth, modern);
            let path = write(&s.codex_home, &r);
            let before = fs::read(&path).unwrap();
            let items = scan(&s);
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].id, r[0]["payload"]["id"]);
            assert_eq!(items[0].title, "fork title");
            let legacy = tmp.path().join("legacy.jsonl");
            sessions::patch_metadata(&path, &legacy, "model_provider", "a-longer-api-provider")
                .unwrap();
            assert_eq!(body(&fs::read(&legacy).unwrap()), body(&before));
            assert_eq!(fs::read(&path).unwrap(), before);
            let mut j = Journal::new(&tmp.path().join("store"), "fork migration").unwrap();
            sessions::stage_changes(
                &mut j,
                &s,
                &items,
                "provider",
                Some("a-longer-api-provider"),
                &CancellationToken::new(),
            )
            .unwrap();
            j.commit().unwrap();
            assert_eq!(body(&fs::read(&path).unwrap()), body(&before));
            assert_eq!(scan(&s)[0].provider, "a-longer-api-provider");
            j.rollback().unwrap();
            assert_eq!(fs::read(&path).unwrap(), before);
        }
    }
}

#[test]
fn broken_fork_chains_never_commit() {
    crypto();
    for invalid in [
        "unrelated",
        "cycle",
        "closing_cycle",
        "self_parent",
        "repeated",
        "gap",
        "late",
        "missing_parent",
        "bad_id",
        "paginated",
        "unknown_mode",
        "subagent",
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let s = Settings {
            codex_home: tmp.path().join("home"),
            ..Default::default()
        };
        let mut r = rows(2, true);
        match invalid {
            "unrelated" => r[2]["payload"]["id"] = uuid::Uuid::new_v4().to_string().into(),
            "cycle" => {
                r[1]["payload"]["forked_from_id"] = r[0]["payload"]["id"].clone();
                r[2] = r[0].clone();
            }
            "closing_cycle" => r[2]["payload"]["forked_from_id"] = r[0]["payload"]["id"].clone(),
            "self_parent" => r[2]["payload"]["forked_from_id"] = r[2]["payload"]["id"].clone(),
            "repeated" => r[2] = r[1].clone(),
            "gap" => r.insert(2, json!({"type":"response_item","payload":{}})),
            "late" => {
                let meta = r.remove(2);
                r.push(meta);
            }
            "missing_parent" => {
                r[1]["payload"]
                    .as_object_mut()
                    .unwrap()
                    .remove("forked_from_id");
            }
            "bad_id" => {
                r[1]["payload"]["forked_from_id"] = "not-a-thread-id".into();
                r[2]["payload"]["id"] = "not-a-thread-id".into();
            }
            "paginated" => r[0]["payload"]["history_mode"] = "paginated".into(),
            "unknown_mode" => r[0]["payload"]["history_mode"] = "future-format".into(),
            _ => r[0]["payload"]["subagent_history_start_ordinal"] = 1.into(),
        }
        let path = write(&s.codex_home, &r);
        let before = fs::read(&path).unwrap();
        // Database-backed scanning can identify a thread without parsing its body.
        let item = sessions::Session {
            id: r[0]["payload"]["id"].as_str().unwrap().into(),
            title: "fork".into(),
            cwd: "/project".into(),
            provider: "openai".into(),
            path: path.clone(),
            updated: 0,
            archived: false,
            database: None,
            related_databases: vec![],
            related_paths: vec![],
        };
        assert!(
            sessions::patch_metadata(&path, &tmp.path().join("out"), "model_provider", "api")
                .is_err(),
            "{invalid}"
        );
        let mut j = Journal::new(&tmp.path().join("store"), "reject").unwrap();
        assert!(
            sessions::stage_changes(
                &mut j,
                &s,
                &[item],
                "provider",
                Some("api"),
                &CancellationToken::new()
            )
            .is_err(),
            "{invalid}"
        );
        assert_eq!(fs::read(&path).unwrap(), before);
        if invalid != "late" {
            assert!(
                !sessions::Scanner::default()
                    .scan(&s, &CancellationToken::new())
                    .unwrap()
                    .warnings
                    .is_empty(),
                "{invalid}"
            );
        }
    }
}

#[test]
#[ignore = "requires private sample directory; only writes disposable copies"]
fn private_forks_migrate_copies_without_changing_originals() {
    crypto();
    let source = PathBuf::from(std::env::var_os("EASY_SWITCH_FORK_SAMPLES").unwrap());
    let export = std::env::var_os("EASY_SWITCH_FORK_OUTPUT").map(PathBuf::from);
    if let Some(root) = &export {
        fs::create_dir(root).unwrap();
        fs::create_dir_all(root.join("before/sessions")).unwrap();
        fs::create_dir_all(root.join("after/sessions")).unwrap();
    }
    let tmp = tempfile::tempdir().unwrap();
    let s = Settings {
        codex_home: tmp.path().join("home"),
        ..Default::default()
    };
    fs::create_dir_all(s.codex_home.join("sessions")).unwrap();
    let mut originals = vec![];
    let mut forks = vec![];
    for e in walkdir::WalkDir::new(&source).follow_links(false) {
        let e = e.unwrap();
        if !e.file_type().is_file() || e.path().extension().is_none_or(|x| x != "jsonl") {
            continue;
        }
        originals.push((e.path().to_owned(), journal::hash(e.path()).unwrap()));
        let raw = fs::read(e.path()).unwrap();
        let parsed: std::result::Result<Vec<Value>, _> = raw
            .split(|b| *b == b'\n')
            .filter(|r| !r.iter().all(u8::is_ascii_whitespace))
            .map(serde_json::from_slice)
            .collect();
        let Ok(parsed) = parsed else {
            continue;
        };
        let meta: Vec<_> = parsed
            .iter()
            .filter(|r| r["type"] == "session_meta")
            .collect();
        if meta.len() < 2 || !meta[0]["payload"]["subagent_history_start_ordinal"].is_null() {
            continue;
        }
        let path = s.codex_home.join("sessions").join(e.file_name());
        fs::write(&path, &raw).unwrap();
        forks.push((path, raw));
    }
    assert_eq!(forks.len(), 7);
    let items = scan(&s);
    assert_eq!(items.len(), 7);
    let mut j = Journal::new(&tmp.path().join("store"), "private copies").unwrap();
    sessions::stage_changes(
        &mut j,
        &s,
        &items,
        "provider",
        Some("local-test-api"),
        &CancellationToken::new(),
    )
    .unwrap();
    j.commit().unwrap();
    for (path, before) in &forks {
        let after = fs::read(path).unwrap();
        assert_eq!(body(&after), body(before));
        if let Some(root) = &export {
            fs::write(
                root.join("before/sessions").join(path.file_name().unwrap()),
                before,
            )
            .unwrap();
            fs::write(
                root.join("after/sessions").join(path.file_name().unwrap()),
                &after,
            )
            .unwrap();
        }
        history::validate_records(path, &CancellationToken::new()).unwrap();
        let out = tmp.path().join("legacy.jsonl");
        sessions::patch_metadata(path, &out, "model_provider", "second-test-api").unwrap();
        assert_eq!(body(&fs::read(&out).unwrap()), body(before));
    }
    assert!(scan(&s).iter().all(|x| x.provider == "local-test-api"));
    j.rollback().unwrap();
    for (path, before) in forks {
        assert_eq!(fs::read(path).unwrap(), before);
    }
    for (path, hash) in &originals {
        assert_eq!(&journal::hash(path).unwrap(), hash);
    }
    println!(
        "7 fork copies migrated and restored; {} original hashes unchanged",
        originals.len()
    );
}
