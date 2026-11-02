use super::*;

fn missing(f: &Fixture, foreign: bool) -> String {
    let original = fs::read_to_string(&f.rollout).unwrap();
    let mut rows: Vec<Value> = original
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    if foreign {
        rows[1]["payload"]["thread_id"] = "parent-thread".into();
        rows[1]["payload"]["thread_settings"]["permission_profile"] = json!({"type":"disabled"});
    } else {
        rows.remove(1);
    }
    let raw: String = rows.iter().map(|r| format!("{r}\n")).collect();
    fs::write(&f.rollout, &raw).unwrap();
    raw
}

#[test]
fn missing_settings_require_current_confirmation_and_keep_ancestry_and_backups() {
    for foreign in [false, true] {
        let mut f = Fixture::new();
        let original = missing(&f, foreign);
        let config = fs::read(f.request.home.join("config.toml")).unwrap();
        f.select("a", "target-model");
        let preview = inspect(&f.request).unwrap();
        assert_eq!(preview["settingsPlan"]["fileCount"], 1);
        assert_eq!(fs::read_to_string(&f.rollout).unwrap(), original);
        assert!(f.run().unwrap_err().to_string().contains("初始化尚未确认"));
        assert_eq!(
            fs::read(f.request.home.join("config.toml")).unwrap(),
            config
        );
        f.request.settings_fingerprint = preview["settingsPlan"]["fingerprint"]
            .as_str()
            .map(str::to_owned);
        fs::write(&f.rollout, format!("{original}\n")).unwrap();
        assert!(f.run().unwrap_err().to_string().contains("记录已变化"));
        fs::write(&f.rollout, &original).unwrap();
        f.run().unwrap();
        let after = fs::read_to_string(&f.rollout).unwrap();
        assert!(
            after
                .split_once('\n')
                .unwrap()
                .1
                .starts_with(original.split_once('\n').unwrap().1)
        );
        let event: Value = serde_json::from_str(after.lines().last().unwrap()).unwrap();
        assert_eq!(event["payload"]["thread_id"], "old-thread");
        let s = &event["payload"]["thread_settings"];
        assert_eq!(s["model"], "target-model");
        assert_eq!(s["approval_policy"], "on-request");
        assert_eq!(s["permission_profile"]["network"], "restricted");
        assert_eq!(
            s["permission_profile"]["file_system"]["entries"],
            json!([{"path":{"type":"special","value":{"kind":"root"}},"access":"read"}])
        );
        assert_eq!(s["disabled_plugin_ids"], json!([]));
        let operations = journal::list(&f.request.store_root.join("operations"), false).unwrap();
        let op = operations.iter().find(|o| o.state == "Complete").unwrap();
        let manifest: journal::Manifest = serde_json::from_slice(
            &fs::read(
                f.request
                    .store_root
                    .join("operations")
                    .join(&op.id)
                    .join("manifest.json"),
            )
            .unwrap(),
        )
        .unwrap();
        let change = manifest.files.iter().find(|c| c.path == f.rollout).unwrap();
        let mut decrypted = vec![];
        easy_switch_core::crypto::decrypt_file(change.backup.as_ref().unwrap(), &mut decrypted)
            .unwrap();
        assert_eq!(decrypted, original.as_bytes());
    }
}

#[test]
fn batch_139_missing_threads_initialize_together_and_preserve_existing_settings() {
    let mut f = Fixture::new();
    let original_existing = fs::read_to_string(&f.rollout).unwrap();
    let cwd = &f.request.project_dirs[0];
    let db = Connection::open(f.request.home.join("state_5.sqlite")).unwrap();
    fs::create_dir_all(f.request.home.join("archived_sessions")).unwrap();
    let mut originals = vec![];
    for n in 0..139 {
        let id = format!("old-{n}");
        let archived = n % 2 == 1;
        let path = f
            .request
            .home
            .join(if archived {
                "archived_sessions"
            } else {
                "sessions"
            })
            .join(format!("{id}.jsonl"));
        let mut body = format!(
            "{}\n",
            json!({"type":"session_meta","payload":{"id":id,"cwd":cwd,"model_provider":"openai"}})
        );
        if n >= 82 {
            body.push_str(&format!("{}\n", json!({"type":"event_msg","payload":{"type":"thread_settings_applied","thread_id":"parent","thread_settings":{"permission_profile":{"type":"disabled"}}}})));
        }
        body.push_str("{\"type\":\"turn_context\",\"payload\":{\"model\":\"old-model\"}}\n");
        fs::write(&path, &body).unwrap();
        db.execute(
            "INSERT INTO threads VALUES(?1,'old chat',?2,'openai',?3,42,?4,'old-model')",
            params![id, cwd.to_str(), path.to_str(), i64::from(archived)],
        )
        .unwrap();
        originals.push((id, path, body, archived));
    }
    drop(db);
    f.select("a", "target-model");
    let preview = inspect(&f.request).unwrap();
    assert_eq!(preview["settingsPlan"]["fileCount"], 139);
    f.request.settings_fingerprint = preview["settingsPlan"]["fingerprint"]
        .as_str()
        .map(str::to_owned);
    f.run().unwrap();
    let db = Connection::open(f.request.home.join("state_5.sqlite")).unwrap();
    for (id, path, raw, archived) in originals {
        let after = fs::read_to_string(path).unwrap();
        assert!(
            after
                .split_once('\n')
                .unwrap()
                .1
                .starts_with(raw.split_once('\n').unwrap().1)
        );
        let event: Value = serde_json::from_str(after.lines().last().unwrap()).unwrap();
        assert_eq!(event["payload"]["thread_id"], id);
        let row: (String, i64, i64) = db
            .query_row(
                "SELECT model,archived,updated_at FROM threads WHERE id=?1",
                [&id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(row, ("target-model".into(), i64::from(archived), 42));
    }
    let existing = fs::read_to_string(&f.rollout).unwrap();
    assert!(
        existing
            .split_once('\n')
            .unwrap()
            .1
            .starts_with(original_existing.split_once('\n').unwrap().1)
    );
    let event: Value = serde_json::from_str(existing.lines().last().unwrap()).unwrap();
    assert_eq!(
        event["payload"]["thread_settings"]["disabled_plugin_ids"],
        json!(["preserved-plugin"])
    );
    drop(db);
    f.request.settings_fingerprint = None;
    f.select("official", "");
    assert!(inspect(&f.request).unwrap().get("settingsPlan").is_none());
    f.run().unwrap();
}

#[test]
fn incomplete_owned_settings_are_not_silently_replaced() {
    let mut f = Fixture::new();
    let raw = fs::read_to_string(&f.rollout)
        .unwrap()
        .replace("\"permission_profile\"", "\"unknown_permissions\"");
    fs::write(&f.rollout, &raw).unwrap();
    f.select("a", "target-model");
    assert!(
        inspect(&f.request)
            .unwrap_err()
            .to_string()
            .contains("权限配置")
    );
    assert_eq!(fs::read_to_string(&f.rollout).unwrap(), raw);
}

#[test]
fn initialized_settings_roll_back_after_partial_commit() {
    let f = Fixture::new();
    let raw = missing(&f, true);
    let settings = settings(&f.request);
    let scan = sessions::Scanner::default()
        .scan(&settings, &tokio_util::sync::CancellationToken::new())
        .unwrap();
    let database = f.request.home.join("state_5.sqlite");
    let before_db = fs::read(&database).unwrap();
    let mut j =
        journal::Journal::new(&f.request.store_root, "initialization rollback test").unwrap();
    sessions::stage_provider_with_model_settings(
        &mut j,
        &settings,
        &scan.sessions,
        PROVIDER,
        Some("target"),
        true,
        &tokio_util::sync::CancellationToken::new(),
        &|_| {},
    )
    .unwrap();
    assert!(j.commit_with_fault(Some(0)).is_err());
    assert_eq!(fs::read_to_string(&f.rollout).unwrap(), raw);
    assert_eq!(fs::read(&database).unwrap(), before_db);
}

#[test]
fn staging_cannot_initialize_an_additional_unconfirmed_thread() {
    let mut f = Fixture::new();
    let valid = fs::read_to_string(&f.rollout)
        .unwrap()
        .replace("old-thread", "other-thread");
    let other = f.request.home.join("sessions/other.jsonl");
    fs::write(&other, &valid).unwrap();
    let db = Connection::open(f.request.home.join("state_5.sqlite")).unwrap();
    db.execute("INSERT INTO threads SELECT 'other-thread',title,cwd,model_provider,?1,updated_at,archived,model FROM threads WHERE id='old-thread'", [other.to_str()]).unwrap();
    drop(db);
    let original = missing(&f, false);
    let config = fs::read(f.request.home.join("config.toml")).unwrap();
    f.select("a", "target");
    let preview = inspect(&f.request).unwrap();
    assert_eq!(preview["settingsPlan"]["fileCount"], 1);
    f.request.settings_fingerprint = preview["settingsPlan"]["fingerprint"]
        .as_str()
        .map(str::to_owned);
    let changed: String = valid
        .lines()
        .filter(|line| !line.contains("thread_settings_applied"))
        .map(|line| format!("{line}\n"))
        .collect();
    let error = switch(
        &f.request,
        &|p| {
            if p.detail == "检查会话关联" {
                fs::write(&other, &changed).unwrap();
            }
        },
        &|| Ok(()),
    )
    .unwrap_err();
    assert!(error.to_string().contains("初始化范围发生变化"), "{error}");
    assert_eq!(fs::read_to_string(&f.rollout).unwrap(), original);
    assert_eq!(fs::read_to_string(&other).unwrap(), changed);
    assert_eq!(
        fs::read(f.request.home.join("config.toml")).unwrap(),
        config
    );
}
