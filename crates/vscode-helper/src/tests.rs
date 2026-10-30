use super::*;
use rusqlite::{Connection, params};
mod appserver;

struct Fixture {
    _root: tempfile::TempDir,
    request: Request,
    rollout: PathBuf,
    body: String,
}
impl Fixture {
    fn new() -> Self {
        #[cfg(not(windows))]
        easy_switch_core::crypto::set_session_key([73; 32]).unwrap();
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("codex");
        let project = root.path().join("project");
        fs::create_dir_all(home.join("sessions")).unwrap();
        fs::create_dir_all(&project).unwrap();
        fs::write(
            home.join("config.toml"),
            "# keep this comment\nmodel = 'official-model'\n[features]\nweb_search = true\n",
        )
        .unwrap();
        fs::write(
            home.join("auth.json"),
            "{\"tokens\":{\"access_token\":\"synthetic-do-not-send\"}}",
        )
        .unwrap();
        let rollout = home.join("sessions/rollout.jsonl");
        let snapshot = json!({"type":"event_msg","timestamp":"2026-10-29T00:00:00Z","payload":{
        "type":"thread_settings_applied","thread_id":"old-thread","thread_settings":{
            "model":"official-model","model_provider_id":"openai","approval_policy":"on-request",
            "approvals_reviewer":"user","permission_profile":{"type":"managed","file_system":{"type":"restricted","entries":[{"path":{"type":"special","value":{"kind":"root"}},"access":"read"}]},"network":"restricted"},
            "cwd":project,"collaboration_mode":{"mode":"default","settings":{"model":"official-model","reasoning_effort":null,"developer_instructions":null}},"disabled_plugin_ids":["preserved-plugin"]
        }}});
        let body = format!(
            "{snapshot}\n{{\"type\":\"turn_context\",\"payload\":{{\"model\":\"official-model\"}}}}\n{{\"type\":\"event_msg\",\"payload\":{{\"type\":\"user_message\",\"message\":\"Keep my old conversation\"}}}}\n{{\"type\":\"response_item\",\"payload\":{{\"type\":\"function_call\",\"name\":\"test_tool\",\"call_id\":\"keep-call\",\"arguments\":\"{{}}\"}}}}\n{{\"type\":\"response_item\",\"payload\":{{\"type\":\"function_call_output\",\"call_id\":\"keep-call\",\"output\":\"keep tool output\"}}}}\n"
        );
        let meta = json!({"type":"session_meta","payload":{"id":"old-thread","cwd":project,"model_provider":"openai"}});
        fs::write(&rollout, format!("{meta}\n{body}")).unwrap();
        let db = Connection::open(home.join("state_5.sqlite")).unwrap();
        db.execute_batch("CREATE TABLE threads (id TEXT PRIMARY KEY,title TEXT,cwd TEXT,model_provider TEXT,rollout_path TEXT,updated_at INTEGER,archived INTEGER,model TEXT)").unwrap();
        db.execute_batch("CREATE TABLE backfill_state(id INTEGER PRIMARY KEY,status TEXT); INSERT INTO backfill_state VALUES(1,'complete')").unwrap();
        db.execute("INSERT INTO threads VALUES ('old-thread','Old chat',?1,'openai',?2,42,0,'official-model')",params![project.to_str(),rollout.to_str()]).unwrap();
        drop(db);
        let request = Request {
            protocol_version: 1,
            action: "switch".into(),
            home,
            sqlite_home: None,
            store_root: root.path().join("plugin"),
            provider: None,
            api_key: None,
            official_model: None,
            journal_key: None,
            operation_id: None,
            expected_config_fingerprint: None,
            project_dirs: vec![project],
        };
        Self {
            _root: root,
            request,
            rollout,
            body,
        }
    }
    fn select(&mut self, id: &str, model: &str) {
        self.request.provider = Some(Provider {
            id: id.into(),
            name: format!("Provider {id}"),
            base_url: "https://example.test/v1".into(),
            model: model.into(),
        });
        self.request.api_key = Some(format!("synthetic-key-{id}"));
        self.request.expected_config_fingerprint = Some(journal::hash_bytes(
            read_config(&self.request.home).unwrap().0.as_bytes(),
        ));
    }
    fn run(&self) -> Result<Value> {
        validate_paths(&self.request)?;
        switch(&self.request, &|_| {}, &|| Ok(()))
    }
}

#[test]
fn corrupt_history_is_reported_and_preserved_while_config_switches() {
    for bad in [
        b"{\"text\":\"PRIVATE_CANARY\n".as_slice(),
        b"{\"text\":\"PRIVATE_CANARY\", broken}\n".as_slice(),
        b"{\"text\":\"PRIVATE_CANARY\xff\"}\n".as_slice(),
    ] {
        let mut f = Fixture::new();
        f.select("a", "api-model");
        let mut damaged = fs::read(&f.rollout).unwrap();
        for _ in 0..42 {
            damaged.extend_from_slice(b"{\"type\":\"event_msg\",\"payload\":{}}\n");
        }
        let line = damaged.iter().filter(|b| **b == b'\n').count() + 1;
        damaged.extend_from_slice(bad);
        damaged.extend_from_slice(b"{\"type\":\"event_msg\",\"payload\":{}}\n");
        fs::write(&f.rollout, &damaged).unwrap();
        let config = fs::read(f.request.home.join("config.toml")).unwrap();
        let database = fs::read(f.request.home.join("state_5.sqlite")).unwrap();
        let inspected = inspect(&f.request).unwrap();
        assert_eq!(inspected["sessionCount"], 0);
        let error = inspected["skippedHistories"][0]["reason"].as_str().unwrap();
        assert!(
            error.contains(
                &sessions::normalize(&f.rollout)
                    .unwrap()
                    .display()
                    .to_string()
            ),
            "{error}"
        );
        assert!(error.contains(&format!("第 {line} 行")), "{error}");
        assert!(!error.contains("PRIVATE_CANARY"));
        assert_eq!(fs::read(&f.rollout).unwrap(), damaged);
        assert_eq!(
            fs::read(f.request.home.join("config.toml")).unwrap(),
            config
        );
        assert_eq!(
            fs::read(f.request.home.join("state_5.sqlite")).unwrap(),
            database
        );
        assert!(!f.request.store_root.exists());
        let result = f.run().unwrap();
        assert_eq!(result["committed"], true);
        assert_eq!(result["sessionCount"], 0);
        assert_eq!(result["skippedHistories"].as_array().unwrap().len(), 1);
        assert_eq!(fs::read(&f.rollout).unwrap(), damaged);
        assert_eq!(
            fs::read(f.request.home.join("state_5.sqlite")).unwrap(),
            database
        );
    }
}

fn extra_history(
    f: &Fixture,
    name: &str,
    id: &str,
    archived: bool,
    body: &[u8],
    indexed: bool,
) -> PathBuf {
    let folder = f.request.home.join(if archived {
        "archived_sessions"
    } else {
        "sessions"
    });
    fs::create_dir_all(&folder).unwrap();
    let path = folder.join(name);
    let meta = json!({"type":"session_meta","payload":{"id":id,"cwd":f.request.project_dirs[0],"model_provider":"openai"}});
    let mut bytes = format!("{meta}\n").into_bytes();
    bytes.extend_from_slice(body);
    fs::write(&path, bytes).unwrap();
    if indexed {
        let db = Connection::open(f.request.home.join("state_5.sqlite")).unwrap();
        db.execute(
            "INSERT INTO threads VALUES (?1,'Preserve',?2,'openai',?3,9,?4,'official-model')",
            params![
                id,
                f.request.project_dirs[0].to_str(),
                path.to_str(),
                archived
            ],
        )
        .unwrap();
    }
    path
}

#[test]
fn mixed_histories_switch_both_directions_without_changing_skipped_files_or_indexes() {
    let mut f = Fixture::new();
    let bad = extra_history(
        &f,
        "bad.jsonl",
        "damaged",
        true,
        b"{\"text\":\"PRIVATE_CANARY\xff\"}\n",
        true,
    );
    let unindexed = extra_history(
        &f,
        "unindexed.jsonl",
        "unindexed",
        false,
        b"{broken\n",
        false,
    );
    let broken_header = f.request.home.join("sessions/broken-header.jsonl");
    fs::write(&broken_header, b"{\"type\":\"session_meta\",\"payload\":").unwrap();
    let originals: Vec<_> = [&bad, &unindexed, &broken_header]
        .into_iter()
        .map(|p| (p.clone(), fs::read(p).unwrap()))
        .collect();
    let database = f.request.home.join("state_5.sqlite");
    let db = Connection::open(&database).unwrap();
    db.execute_batch("CREATE TABLE local_thread_catalog_hosts(host_id TEXT,host_kind TEXT);
        INSERT INTO local_thread_catalog_hosts VALUES('local','local');
        CREATE TABLE local_thread_catalog(host_id TEXT,thread_id TEXT,model_provider TEXT);
        INSERT INTO local_thread_catalog VALUES('local','damaged','openai'),('local','old-thread','openai');
        CREATE TABLE thread_history_projection_state(thread_id TEXT,next_rollout_byte_offset INTEGER);
        INSERT INTO thread_history_projection_state VALUES('damaged',15);
        CREATE TABLE thread_turns(thread_id TEXT,rollout_byte_offset INTEGER,rollout_end_byte_offset INTEGER);
        INSERT INTO thread_turns VALUES('damaged',3,15);").unwrap();
    drop(db);
    let sqlite = f._root.path().join("independent-sqlite");
    fs::create_dir_all(&sqlite).unwrap();
    let moved_database = sqlite.join("state_5.sqlite");
    fs::rename(&database, &moved_database).unwrap();
    let database = moved_database;
    f.request.sqlite_home = Some(sqlite);
    for (id, model, expected) in [
        ("a", "model-a", PROVIDER),
        ("b", "model-b", PROVIDER),
        ("official", "", "openai"),
    ] {
        f.select(id, model);
        let inspected = inspect(&f.request).unwrap();
        assert_eq!(inspected["sessionCount"], 1);
        assert_eq!(inspected["skippedHistories"].as_array().unwrap().len(), 3);
        let preservation = easy_switch_core::history_selection::Selection::read(
            &settings(&f.request),
            &CancellationToken::new(),
        )
        .unwrap();
        let result = f.run().unwrap();
        assert_eq!(result["sessionCount"], 1);
        assert_eq!(result["skippedHistories"].as_array().unwrap().len(), 3);
        assert!(!result.to_string().contains("PRIVATE_CANARY"));
        preservation.verify_preserved().unwrap();
        for (path, bytes) in &originals {
            assert_eq!(&fs::read(path).unwrap(), bytes);
        }
        assert!(fs::read_to_string(&f.rollout).unwrap().contains(&f.body));
        let db = Connection::open(&database).unwrap();
        assert_eq!(
            db.query_row(
                "SELECT model_provider FROM threads WHERE id='old-thread'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            expected
        );
        assert_eq!(
            db.query_row(
                "SELECT model_provider FROM threads WHERE id='damaged'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "openai"
        );
    }
}

#[test]
fn corrupt_paginated_child_preserves_ancestors_siblings_and_all_projection_rows() {
    let mut f = Fixture::new();
    let healthy_id = "44444444-4444-4444-8444-444444444444";
    let healthy_path = f
        .request
        .home
        .join(format!("sessions/rollout-{healthy_id}.jsonl"));
    let mut healthy_bytes = vec![];
    for (i, line) in fs::read_to_string(&f.rollout).unwrap().lines().enumerate() {
        let mut row: Value = serde_json::from_str(line).unwrap();
        row["ordinal"] = i.into();
        if i == 0 {
            row["payload"]["history_mode"] = "paginated".into();
            row["payload"]["session_id"] = "old-thread".into();
        }
        healthy_bytes.extend_from_slice(format!("{row}\n").as_bytes());
    }
    fs::rename(&f.rollout, &healthy_path).unwrap();
    fs::write(&healthy_path, &healthy_bytes).unwrap();
    f.rollout = healthy_path;
    Connection::open(f.request.home.join("state_5.sqlite"))
        .unwrap()
        .execute(
            "UPDATE threads SET rollout_path=?1 WHERE id='old-thread'",
            [f.rollout.to_str()],
        )
        .unwrap();
    let ids = [
        "11111111-1111-4111-8111-111111111111",
        "22222222-2222-4222-8222-222222222222",
        "33333333-3333-4333-8333-333333333333",
    ];
    let mut paths = vec![];
    for (i, id) in ids.iter().enumerate() {
        let p = extra_history(&f, &format!("rollout-{id}.jsonl"), id, false, b"", true);
        let mut meta = json!({"type":"session_meta","ordinal":0,"payload":{"id":id,"session_id":id,"history_mode":"paginated","model_provider":"openai","cwd":f.request.project_dirs[0]}});
        if i > 0 {
            meta["payload"]["history_base"] = json!({"thread_id":ids[0],"end_byte_offset":fs::metadata(&paths[0]).unwrap().len(),"end_ordinal_exclusive":1});
        }
        fs::write(
            &p,
            format!("{meta}\n{}", if i == 1 { "{\"broken\":\"\n" } else { "" }),
        )
        .unwrap();
        paths.push(p);
    }
    let db = Connection::open(f.request.home.join("state_5.sqlite")).unwrap();
    db.execute_batch("CREATE TABLE thread_history_projection_state(thread_id TEXT,next_rollout_byte_offset INTEGER);
        CREATE TABLE thread_turns(thread_id TEXT,rollout_byte_offset INTEGER,rollout_end_byte_offset INTEGER);").unwrap();
    for (id, p) in ids.iter().zip(&paths) {
        db.execute(
            "INSERT INTO thread_history_projection_state VALUES(?1,?2)",
            params![id, fs::metadata(p).unwrap().len()],
        )
        .unwrap();
        db.execute(
            "INSERT INTO thread_turns VALUES(?1,0,?2)",
            params![id, fs::metadata(p).unwrap().len()],
        )
        .unwrap();
    }
    db.execute(
        "INSERT INTO thread_history_projection_state VALUES(?1,?2)",
        params![healthy_id, healthy_bytes.len()],
    )
    .unwrap();
    drop(db);
    f.select("a", "model-a");
    let selection = easy_switch_core::history_selection::Selection::read(
        &settings(&f.request),
        &CancellationToken::new(),
    )
    .unwrap();
    assert_eq!(selection.skipped.len(), 3);
    let result = f.run().unwrap();
    assert_eq!(result["sessionCount"], 1);
    assert_eq!(result["skippedHistories"].as_array().unwrap().len(), 3);
    selection.verify_preserved().unwrap();
    let db = Connection::open(f.request.home.join("state_5.sqlite")).unwrap();
    let end: usize = db.query_row("SELECT next_rollout_byte_offset FROM thread_history_projection_state WHERE thread_id=?1",[healthy_id],|r|r.get(0)).unwrap();
    assert!(
        end > healthy_bytes.len(),
        "independent healthy pagination was not rebased"
    );
    assert_eq!(history::header(&f.rollout).unwrap().id, "old-thread");
}

#[test]
fn modifying_a_skipped_index_during_commit_rolls_back_the_entire_switch() {
    let mut f = Fixture::new();
    let bad = extra_history(&f, "bad.jsonl", "damaged", false, b"{broken\n", true);
    let db = Connection::open(f.request.home.join("state_5.sqlite")).unwrap();
    db.execute_batch("CREATE TRIGGER disturb_skipped AFTER UPDATE OF model_provider ON threads WHEN NEW.id='old-thread'
        BEGIN UPDATE threads SET model_provider='changed-by-trigger' WHERE id='damaged'; END;").unwrap();
    drop(db);
    let originals: Vec<_> = [
        &bad,
        &f.rollout,
        &f.request.home.join("config.toml"),
        &f.request.home.join("state_5.sqlite"),
    ]
    .into_iter()
    .map(|p| (p.clone(), fs::read(p).unwrap()))
    .collect();
    f.select("a", "model-a");
    let error = f.run().unwrap_err().to_string();
    assert!(error.contains("索引发生变化"), "{error}");
    for (path, bytes) in originals {
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
}

#[test]
fn damaged_alternate_rollout_preserves_the_whole_thread_in_mirrored_indexes() {
    let mut f = Fixture::new();
    let bad = extra_history(
        &f,
        "alternate.jsonl",
        "old-thread",
        false,
        b"{broken\n",
        false,
    );
    extra_history(&f, "intact.jsonl", "intact", false, b"", true);
    let mirror = f.request.home.join("state_4.sqlite");
    fs::copy(f.request.home.join("state_5.sqlite"), &mirror).unwrap();
    let bytes = fs::read(&f.rollout).unwrap();
    let bad_bytes = fs::read(&bad).unwrap();
    f.select("a", "model-a");
    let selection = easy_switch_core::history_selection::Selection::read(
        &settings(&f.request),
        &CancellationToken::new(),
    )
    .unwrap();
    let result = f.run().unwrap();
    assert_eq!(result["sessionCount"], 1);
    assert_eq!(result["skippedHistories"].as_array().unwrap().len(), 2);
    assert_eq!(fs::read(&f.rollout).unwrap(), bytes);
    assert_eq!(fs::read(&bad).unwrap(), bad_bytes);
    selection.verify_preserved().unwrap();
    for path in [&mirror, &f.request.home.join("state_5.sqlite")] {
        let db = Connection::open(path).unwrap();
        assert_eq!(
            db.query_row(
                "SELECT model_provider FROM threads WHERE id='old-thread'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "openai"
        );
        assert_eq!(
            db.query_row(
                "SELECT model_provider FROM threads WHERE id='intact'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            PROVIDER
        );
    }
}

#[test]
fn unreadable_metadata_preserves_unknown_pagination_links() {
    let mut f = Fixture::new();
    let good = extra_history(
        &f,
        "rollout-55555555-5555-4555-8555-555555555555.jsonl",
        "other",
        false,
        b"",
        true,
    );
    let mut meta: Value = serde_json::from_slice(&fs::read(&good).unwrap()).unwrap();
    meta["payload"]["history_mode"] = "paginated".into();
    fs::write(&good, format!("{meta}\n")).unwrap();
    fs::write(
        f.request.home.join("sessions/bad-header.jsonl"),
        b"{\"type\":\"session_meta\",\"payload\":",
    )
    .unwrap();
    let bytes = fs::read(&good).unwrap();
    f.select("a", "model-a");
    let result = f.run().unwrap();
    assert_eq!(result["sessionCount"], 1);
    assert_eq!(result["skippedHistories"].as_array().unwrap().len(), 2);
    assert_eq!(fs::read(&good).unwrap(), bytes);
}

#[test]
#[cfg(unix)]
fn skip_policy_does_not_bypass_symlink_or_permission_errors() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let mut f = Fixture::new();
    f.select("a", "model-a");
    let path = extra_history(&f, "damaged.jsonl", "damaged", false, b"{broken\n", true);
    let link = f.request.home.join("sessions/link.jsonl");
    symlink(&path, &link).unwrap();
    assert!(
        inspect(&f.request)
            .unwrap_err()
            .to_string()
            .contains("符号链接")
    );
    fs::remove_file(link).unwrap();
    let permissions = fs::metadata(&path).unwrap().permissions();
    fs::set_permissions(&path, fs::Permissions::from_mode(0)).unwrap();
    // Root can read mode 000; native CI runs as an unprivileged user.
    if fs::File::open(&path).is_err() {
        assert!(inspect(&f.request).is_err());
    }
    fs::set_permissions(&path, permissions).unwrap();
    assert!(inspect(&f.request).is_ok());
}

#[test]
fn unknown_structure_still_blocks_and_preservation_detects_external_edits() {
    let mut f = Fixture::new();
    f.select("a", "model-a");
    let path = extra_history(&f, "broken.jsonl", "damaged", false, b"{broken\n", true);
    let selection = easy_switch_core::history_selection::Selection::read(
        &settings(&f.request),
        &CancellationToken::new(),
    )
    .unwrap();
    fs::write(&path, b"external edit\n").unwrap();
    assert!(
        selection
            .verify_preserved()
            .unwrap_err()
            .to_string()
            .contains("聊天发生变化")
    );
    let unknown = f.request.home.join("sessions/unknown.jsonl");
    fs::write(&unknown, b"{\"future_schema\":true}\n").unwrap();
    let original = fs::read(f.request.home.join("config.toml")).unwrap();
    assert!(
        inspect(&f.request)
            .unwrap_err()
            .to_string()
            .contains("无法确认聊天结构")
    );
    assert!(f.run().is_err());
    assert_eq!(
        fs::read(f.request.home.join("config.toml")).unwrap(),
        original
    );
}

#[test]
fn successful_switch_preserves_encrypted_before_images_across_next_switch() {
    let mut f = Fixture::new();
    let original = fs::read(&f.rollout).unwrap();
    f.select("a", "api-a-model");
    f.run().unwrap();
    let operations = journal::list(&f.request.store_root.join("operations"), false).unwrap();
    assert_eq!(
        operations.len(),
        1,
        "successful switch removed its audit and backups"
    );
    let dir = f
        .request
        .store_root
        .join("operations")
        .join(&operations[0].id);
    let manifest: journal::Manifest =
        serde_json::from_slice(&fs::read(dir.join("manifest.json")).unwrap()).unwrap();
    let image = manifest
        .files
        .iter()
        .find(|c| c.path == f.rollout)
        .unwrap()
        .backup
        .as_ref()
        .unwrap();
    let mut recovered = Vec::new();
    easy_switch_core::crypto::decrypt_file(image, &mut recovered).unwrap();
    assert_eq!(recovered, original);
    f.select("b", "api-b-model");
    f.run().unwrap();
    assert!(image.exists());
    assert_eq!(
        journal::list(&f.request.store_root.join("operations"), false)
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn official_api_a_api_b_official_keeps_login_and_history() {
    let mut f = Fixture::new();
    let auth = fs::read(f.request.home.join("auth.json")).unwrap();
    for (id, model, provider) in [
        ("a", "api-a-model", PROVIDER),
        ("b", "api-b-model", PROVIDER),
        ("official", "", "openai"),
    ] {
        f.select(id, model);
        assert_eq!(f.run().unwrap()["committed"], true);
        let text = fs::read_to_string(&f.rollout).unwrap();
        assert!(text.split_once('\n').unwrap().1.starts_with(&f.body));
        let last: Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
        assert_eq!(last["payload"]["type"], "thread_settings_applied");
        assert_eq!(
            last["payload"]["thread_settings"]["model_provider_id"],
            provider
        );
        assert_eq!(
            last["payload"]["thread_settings"]["model"],
            if id == "official" {
                "official-model"
            } else {
                model
            }
        );
        assert_eq!(fs::read(f.request.home.join("auth.json")).unwrap(), auth);
        let db = Connection::open(f.request.home.join("state_5.sqlite")).unwrap();
        let row: (String, String, i64) = db
            .query_row(
                "SELECT model_provider,model,updated_at FROM threads WHERE id='old-thread'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            row,
            (
                provider.into(),
                if id == "official" {
                    "official-model".into()
                } else {
                    model.into()
                },
                42
            )
        );
        let cfg = read_config(&f.request.home).unwrap().0;
        assert!(
            cfg.contains("# keep this comment") && cfg.contains("web_search = true"),
            "{id}: {cfg}"
        );
        if id == "official" {
            assert!(!cfg.contains("synthetic-key"));
        }
        assert_eq!(
            active_provider(
                &read_config(&f.request.home).unwrap().1,
                &read_state(&f.request).unwrap()
            ),
            Some(id.into())
        );
    }
}

#[test]
fn stale_configuration_and_busy_guard_leave_files_unchanged() {
    let mut f = Fixture::new();
    f.select("a", "model-a");
    let before = fs::read(&f.rollout).unwrap();
    fs::write(f.request.home.join("config.toml"), "# external change\n").unwrap();
    assert!(f.run().unwrap_err().to_string().contains("外部修改"));
    assert_eq!(fs::read(&f.rollout).unwrap(), before);
    f.select("a", "model-a");
    assert!(switch(&f.request, &|_| {}, &|| anyhow::bail!("busy")).is_err());
    assert_eq!(fs::read(&f.rollout).unwrap(), before);
}

#[test]
fn rejects_collision_and_project_overrides_without_stopping_backend() {
    let mut f = Fixture::new();
    f.select("a", "model-a");
    let config = f.request.home.join("config.toml");
    fs::write(
        &config,
        format!("[model_providers.{PROVIDER}]\nname='not ours'\n"),
    )
    .unwrap();
    assert!(
        inspect(&f.request)
            .unwrap_err()
            .to_string()
            .contains("冲突")
    );
    fs::write(&config, "").unwrap();
    let project_config = f.request.project_dirs[0].join(".codex");
    fs::create_dir(&project_config).unwrap();
    fs::write(
        project_config.join("config.toml"),
        "model_provider='another'\n",
    )
    .unwrap();
    assert!(
        inspect(&f.request)
            .unwrap_err()
            .to_string()
            .contains("项目配置覆盖")
    );
}

#[test]
fn missing_key_and_invalid_url_do_not_touch_configuration() {
    let mut f = Fixture::new();
    f.select("a", "model-a");
    let before = read_config(&f.request.home).unwrap().0;
    f.request.api_key = None;
    assert!(f.run().is_err());
    assert_eq!(read_config(&f.request.home).unwrap().0, before);
    f.request.api_key = Some("synthetic-key".into());
    f.request.provider.as_mut().unwrap().base_url = "https://user:password@example.test/v1".into();
    assert!(f.run().is_err());
    assert_eq!(read_config(&f.request.home).unwrap().0, before);
}

#[test]
fn incomplete_active_index_is_rejected_before_switching() {
    for damage in ["missing-db", "missing-row", "missing-model", "backfilling"] {
        let mut f = Fixture::new();
        f.select("a", "model-a");
        let path = f.request.home.join("state_5.sqlite");
        if damage == "missing-db" {
            fs::rename(&path, f._root.path().join("offline.sqlite")).unwrap();
        } else {
            let db = Connection::open(&path).unwrap();
            db.execute_batch(match damage {
                "missing-row" => "DELETE FROM threads",
                "missing-model" => "ALTER TABLE threads DROP COLUMN model",
                _ => "UPDATE backfill_state SET status='running'",
            })
            .unwrap();
        }
        let config = fs::read(f.request.home.join("config.toml")).unwrap();
        let history = fs::read(&f.rollout).unwrap();
        assert!(inspect(&f.request).is_err(), "{damage}");
        assert_eq!(
            fs::read(f.request.home.join("config.toml")).unwrap(),
            config
        );
        assert_eq!(fs::read(&f.rollout).unwrap(), history);
        assert!(!f.request.home.join(OWNER_FILE).exists());
    }
}

#[test]
fn model_history_without_owned_settings_is_rejected_in_preflight() {
    let mut f = Fixture::new();
    f.select("a", "model-a");
    let meta = fs::read_to_string(&f.rollout)
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .to_owned();
    let original = format!("{meta}\n{}", f.body.split_once('\n').unwrap().1);
    fs::write(&f.rollout, &original).unwrap();
    assert!(inspect(&f.request).is_err());
    assert_eq!(fs::read_to_string(&f.rollout).unwrap(), original);
    assert!(!f.request.home.join(OWNER_FILE).exists());
}

#[test]
fn official_without_config_model_uses_confirmed_model_and_restores_default_config() {
    let mut f = Fixture::new();
    fs::write(
        f.request.home.join("config.toml"),
        "# default official model\n",
    )
    .unwrap();
    f.select("a", "api-only");
    let mut status = f.request.clone();
    status.provider = None;
    assert!(inspect(&status).unwrap()["officialModel"].is_null());
    assert!(f.run().unwrap_err().to_string().contains("确认切回官方"));
    f.request.official_model = Some("confirmed-official".into());
    assert_eq!(
        inspect(&f.request).unwrap()["officialModel"],
        "confirmed-official"
    );
    f.run().unwrap();
    f.request.official_model = None;
    f.select("official", "");
    f.run().unwrap();
    let db = Connection::open(f.request.home.join("state_5.sqlite")).unwrap();
    let model: Option<String> = db
        .query_row("SELECT model FROM threads", [], |r| r.get(0))
        .unwrap();
    assert_eq!(model.as_deref(), Some("confirmed-official"));
    assert!(
        !read_config(&f.request.home)
            .unwrap()
            .1
            .contains_key("model")
    );
}

#[test]
fn user_config_selects_real_database_directory_over_environment() {
    let mut f = Fixture::new();
    let dbdir = f._root.path().join("external-db");
    fs::create_dir(&dbdir).unwrap();
    fs::rename(
        f.request.home.join("state_5.sqlite"),
        dbdir.join("state_5.sqlite"),
    )
    .unwrap();
    let wrong = f._root.path().join("unused-env-db");
    fs::create_dir(&wrong).unwrap();
    let mut doc = read_config(&f.request.home).unwrap().1;
    doc["sqlite_home"] = value(dbdir.to_str().unwrap());
    fs::write(f.request.home.join("config.toml"), doc.to_string()).unwrap();
    f.request.sqlite_home = Some(wrong);
    f.select("a", "target-model");
    let resolved = resolve_request(&f.request).unwrap();
    assert_eq!(resolved.sqlite_home.as_deref(), Some(dbdir.as_path()));
    assert_eq!(
        inspect(&resolved).unwrap()["sqliteHome"],
        dbdir.to_str().unwrap()
    );
    switch(&resolved, &|_| {}, &|| Ok(())).unwrap();
    let db = Connection::open(dbdir.join("state_5.sqlite")).unwrap();
    let row: (String, String) = db
        .query_row("SELECT model_provider,model FROM threads", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(row, (PROVIDER.into(), "target-model".into()));
}

#[test]
fn interrupted_operation_remains_visible_even_if_live_config_is_invalid() {
    let mut f = Fixture::new();
    f.request.action = "inspect".into();
    let op = f.request.store_root.join("operations/test-operation");
    fs::create_dir_all(&op).unwrap();
    fs::write(
        op.join("manifest.json"),
        r#"{"state":"Applying","description":"Interrupted test","files":[]}"#,
    )
    .unwrap();
    fs::write(
        f.request.store_root.join("target.json"),
        serde_json::to_vec(&Target {
            home: f.request.home.clone(),
            sqlite_home: None,
        })
        .unwrap(),
    )
    .unwrap();
    fs::write(
        f.request.home.join("config.toml"),
        "this is not valid toml = [",
    )
    .unwrap();
    let result = execute(&f.request, &|_| {}).unwrap();
    assert_eq!(result["operations"][0]["state"], "Applying");
    assert!(result["configFingerprint"].is_null());
}

#[test]
fn another_store_cannot_inspect_switch_or_recover_an_owned_home() {
    let mut f = Fixture::new();
    f.request.action = "inspect".into();
    execute(&f.request, &|_| {}).unwrap();
    assert!(!f.request.home.join(OWNER_FILE).exists());
    fs::create_dir(&f.request.store_root).unwrap();
    claim_owner(&f.request).unwrap();
    let owner_bytes = fs::read(f.request.home.join(OWNER_FILE)).unwrap();
    let before = fs::read(&f.rollout).unwrap();
    let config = read_config(&f.request.home).unwrap().0;
    let operation = f.request.store_root.join("operations/interrupted");
    fs::create_dir_all(&operation).unwrap();
    fs::write(
        operation.join("manifest.json"),
        r#"{"state":"Applying","description":"Original store interrupted","files":[]}"#,
    )
    .unwrap();
    let mut foreign = f.request.clone();
    foreign.store_root = f._root.path().join("another-profile");
    foreign.operation_id = Some("interrupted".into());
    for action in ["inspect", "switch", "recover"] {
        foreign.action = action.into();
        assert!(
            execute(&foreign, &|_| {})
                .unwrap_err()
                .to_string()
                .contains("已绑定另一 VS Code 存储环境"),
            "{action} must reject before inspecting foreign operations or running the Linux guard"
        );
    }
    assert!(switch(&foreign, &|_| {}, &|| Ok(())).is_err());
    assert!(recover(&foreign, &|| Ok(())).is_err());
    assert!(!foreign.store_root.exists());
    assert_eq!(
        fs::read(f.request.home.join(OWNER_FILE)).unwrap(),
        owner_bytes
    );
    assert_eq!(fs::read(&f.rollout).unwrap(), before);
    assert_eq!(read_config(&f.request.home).unwrap().0, config);
    assert!(operation.join("manifest.json").exists());
}

#[test]
fn concurrent_first_claim_has_one_owner_and_never_overwrites_it() {
    let f = Fixture::new();
    let first = f.request.clone();
    let mut second = first.clone();
    second.store_root = f._root.path().join("another-profile");
    fs::create_dir(&first.store_root).unwrap();
    fs::create_dir(&second.store_root).unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles = [first.clone(), second.clone()].map(|request| {
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            barrier.wait();
            let result = claim_owner(&request);
            (request, result)
        })
    });
    let outcomes = handles.map(|handle| handle.join().unwrap());
    assert_eq!(
        outcomes.iter().filter(|(_, result)| result.is_ok()).count(),
        1
    );
    let (winner, _) = outcomes.iter().find(|(_, result)| result.is_ok()).unwrap();
    let (loser, _) = outcomes.iter().find(|(_, result)| result.is_err()).unwrap();
    let path = f.request.home.join(OWNER_FILE);
    let before = fs::read(&path).unwrap();
    let owner: HomeOwner = serde_json::from_slice(&before).unwrap();
    assert_eq!(owner.home, canonical_directory(&winner.home).unwrap());
    assert_eq!(
        owner.store_root,
        canonical_directory(&winner.store_root).unwrap()
    );
    claim_owner(winner).unwrap();
    assert!(claim_owner(loser).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
    assert!(fs::read_dir(&f.request.home).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".easy-switch-owner-")
    }));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[cfg(unix)]
#[test]
fn sqlite_directory_symlink_is_rejected_before_migration() {
    use std::os::unix::fs::symlink;
    let mut f = Fixture::new();
    let target = f._root.path().join("real-db");
    fs::create_dir(&target).unwrap();
    let link = f._root.path().join("linked-db");
    symlink(target, &link).unwrap();
    let mut doc = read_config(&f.request.home).unwrap().1;
    doc["sqlite_home"] = value(link.to_str().unwrap());
    fs::write(f.request.home.join("config.toml"), doc.to_string()).unwrap();
    f.select("a", "model-a");
    assert!(
        resolve_request(&f.request)
            .err()
            .unwrap()
            .to_string()
            .contains("链接")
    );
}
