use super::*;
use easy_switch_core::history_repair::Plan;

fn aligned_prefix(f: &Fixture, prefix: &[u8], suffix: &[u8]) -> Vec<u8> {
    let mut data = fs::read(&f.rollout).unwrap();
    let mut filler = json!({"timestamp":"2026-01-01T00:00:00Z","type":"event_msg","payload":{"type":"agent_message","message":""}});
    let base = serde_json::to_vec(&filler).unwrap().len() + 1;
    let count = (4096 - (data.len() + base + prefix.len()) % 4096) % 4096;
    filler["payload"]["message"] = "x".repeat(count).into();
    data.extend(format!("{filler}\n").as_bytes());
    data.extend(prefix);
    assert_eq!(data.len() % 4096, 0);
    data.extend(suffix);
    fs::write(&f.rollout, &data).unwrap();
    data
}
fn confirmation(f: &mut Fixture) -> Value {
    f.request.action = "repair".into();
    let result = inspect(&f.request).unwrap();
    f.request.repair_fingerprint = result["repairPlan"]["fingerprint"]
        .as_str()
        .map(str::to_owned);
    f.request.expected_config_fingerprint = result["configFingerprint"].as_str().map(str::to_owned);
    result
}

#[test]
fn explicit_repair_keeps_original_backup_and_enables_strict_switch() {
    let mut f = Fixture::new();
    let full = "{\"timestamp\":\"2026-01-01T01:00:00Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"agent_message\",\"message\":\"complete synthetic message to retain\"}}
";
    let bytes = aligned_prefix(&f, &full.as_bytes()[..full.len() - 12], full.as_bytes());
    let config = fs::read(f.request.home.join("config.toml")).unwrap();
    let db = fs::read(f.request.home.join("state_5.sqlite")).unwrap();
    f.select("a", "api-model");
    assert!(f.run().unwrap_err().to_string().contains("先运行"));
    let preview = confirmation(&mut f);
    assert_eq!(preview["repairPlan"]["fileCount"], 1);
    assert_eq!(fs::read(&f.rollout).unwrap(), bytes);
    let result = execute(&f.request, &|_| {}).unwrap();
    assert_eq!(result["repairedFileCount"], 1);
    history::validate_records(&f.rollout, &CancellationToken::new()).unwrap();
    assert!(fs::read_to_string(&f.rollout).unwrap().contains(&f.body));
    assert_eq!(
        fs::read(f.request.home.join("config.toml")).unwrap(),
        config
    );
    assert_eq!(fs::read(f.request.home.join("state_5.sqlite")).unwrap(), db);
    let operations = journal::list(&f.request.store_root.join("operations"), false).unwrap();
    let dir = f
        .request
        .store_root
        .join("operations")
        .join(&operations.last().unwrap().id);
    let manifest: journal::Manifest =
        serde_json::from_slice(&fs::read(dir.join("manifest.json")).unwrap()).unwrap();
    let mut original = vec![];
    easy_switch_core::crypto::decrypt_file(
        manifest.files[0].backup.as_ref().unwrap(),
        &mut original,
    )
    .unwrap();
    assert_eq!(original, bytes);
    assert!(dir.join("repair-report.json").is_file());
    assert!(inspect(&f.request).unwrap().get("repairPlan").is_none());
    f.select("a", "api-model");
    assert_eq!(f.run().unwrap()["sessionCount"], 1);
}

#[test]
fn repair_requires_current_preview_and_stopped_writer() {
    let mut f = Fixture::new();
    let full = "{\"timestamp\":\"2026-01-01T01:00:00Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"agent_message\",\"message\":\"complete synthetic message to retain\"}}
";
    let before = aligned_prefix(&f, &full.as_bytes()[..full.len() - 8], full.as_bytes());
    confirmation(&mut f);
    f.request.repair_fingerprint = Some("wrong".into());
    assert!(crate::repair(&f.request, &|_| {}, &|| Ok(())).is_err());
    confirmation(&mut f);
    assert!(crate::repair(&f.request, &|_| {}, &|| anyhow::bail!("busy")).is_err());
    let mut changed = before.clone();
    changed.extend(b"\n");
    fs::write(&f.rollout, &changed).unwrap();
    assert!(crate::repair(&f.request, &|_| {}, &|| Ok(())).is_err());
    assert_eq!(fs::read(&f.rollout).unwrap(), changed);
}

#[test]
fn missing_image_is_explicit_and_text_call_id_survive() {
    let mut f = Fixture::new();
    let prefix=b"{\"timestamp\":\"2026-01-01T00:00:00Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"custom_tool_call_output\",\"call_id\":\"keep-call\",\"output\":[{\"type\":\"input_text\",\"text\":\"retain text\"},{\"type\":\"input_image\",\"image_url\":\"data:image/png;base64,AAA";
    aligned_prefix(&f, prefix, b"");
    let preview = confirmation(&mut f);
    assert_eq!(preview["repairPlan"]["items"][0]["missingImage"], true);
    crate::repair(&f.request, &|_| {}, &|| Ok(())).unwrap();
    let content = fs::read_to_string(&f.rollout).unwrap();
    let last: Value = serde_json::from_str(content.lines().last().unwrap()).unwrap();
    assert_eq!(last["payload"]["call_id"], "keep-call");
    assert_eq!(last["payload"]["output"][0]["text"], "retain text");
    assert!(
        last["payload"]["output"][1]["text"]
            .as_str()
            .unwrap()
            .contains("无法恢复")
    );
}

#[test]
fn unsupported_damage_and_projection_rows_are_not_silently_skipped() {
    let f = Fixture::new();
    let full = "{\"timestamp\":\"2026-01-01T01:00:00Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"agent_message\",\"message\":\"complete synthetic message to retain\"}}
";
    let before = aligned_prefix(&f, &full.as_bytes()[..full.len() - 8], full.as_bytes());
    let db = Connection::open(f.request.home.join("state_5.sqlite")).unwrap();
    db.execute_batch("CREATE TABLE thread_turns(thread_id TEXT,rollout_byte_offset INTEGER); INSERT INTO thread_turns VALUES('old-thread',0)").unwrap();
    drop(db);
    assert!(
        inspect(&f.request)
            .unwrap_err()
            .to_string()
            .contains("偏移索引")
    );
    assert_eq!(fs::read(&f.rollout).unwrap(), before);
    let mut unrelated = Fixture::new();
    unrelated.select("a", "model");
    use std::io::Write;
    fs::OpenOptions::new()
        .append(true)
        .open(&unrelated.rollout)
        .unwrap()
        .write_all(b"{broken\n")
        .unwrap();
    assert!(inspect(&unrelated.request).is_err());
    assert!(unrelated.run().is_err());
}

#[test]
#[ignore = "private sample read-only; requires EASY_SWITCH_REPAIR_SAMPLE_HOME"]
fn private_sample_read_only_preview() {
    let home = PathBuf::from(std::env::var("EASY_SWITCH_REPAIR_SAMPLE_HOME").unwrap());
    let plan = Plan::read(
        &Settings {
            codex_home: home,
            ..Settings::default()
        },
        &CancellationToken::new(),
    )
    .unwrap();
    assert_eq!(plan.summary.file_count, 7);
    assert_eq!(plan.summary.items.len(), 21);
    assert_eq!(
        plan.summary
            .items
            .iter()
            .filter(|i| i.missing_image)
            .count(),
        1
    );
}

#[test]
#[ignore = "private samples; repairs disposable copies only; requires explicit input and output directories"]
fn private_sample_transaction_in_disposable_home() {
    let source = PathBuf::from(std::env::var("EASY_SWITCH_REPAIR_SAMPLE_HOME").unwrap());
    let output = PathBuf::from(std::env::var("EASY_SWITCH_REPAIR_TEST_OUTPUT").unwrap());
    fs::create_dir(&output).unwrap();
    let plan = Plan::read(
        &Settings {
            codex_home: source.clone(),
            ..Settings::default()
        },
        &CancellationToken::new(),
    )
    .unwrap();
    let paths: std::collections::BTreeSet<_> =
        plan.summary.items.iter().map(|i| i.path.clone()).collect();
    assert_eq!(paths.len(), 7);
    let mut f = Fixture::new();
    let mut originals = vec![];
    for path in &paths {
        let relative = path.strip_prefix(&source).unwrap();
        let dest = f.request.home.join(relative);
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::copy(path, &dest).unwrap();
        originals.push((path.clone(), journal::hash(path).unwrap()));
    }
    confirmation(&mut f);
    let result = crate::repair(&f.request, &|_| {}, &|| Ok(())).unwrap();
    assert_eq!(result["repairedFileCount"], 7);
    let mut records = 0;
    for (path, hash) in originals {
        assert_eq!(journal::hash(&path).unwrap(), hash);
        let relative = path.strip_prefix(&source).unwrap();
        let repaired = f.request.home.join(relative);
        let data = fs::read(&repaired).unwrap();
        records += data
            .split(|c| *c == b'\n')
            .filter(|r| !r.is_empty())
            .count();
        history::validate_records(&repaired, &CancellationToken::new()).unwrap();
        let dest = output.join(relative);
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::copy(repaired, dest).unwrap();
    }
    assert_eq!(records, 3015);
}

#[test]
fn separate_partial_attempt_preserves_following_complete_record() {
    let mut f = Fixture::new();
    let full=b"{\"timestamp\":\"2026-01-01T00:00:00Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"agent_message\",\"message\":\"known complete reply\"}}\n";
    let mut suffix = b"\n".to_vec();
    suffix.extend(full);
    aligned_prefix(&f, &full[..full.len() - 8], &suffix);
    confirmation(&mut f);
    crate::repair(&f.request, &|_| {}, &|| Ok(())).unwrap();
    assert!(fs::read(&f.rollout).unwrap().ends_with(full));
    assert_eq!(
        fs::read_to_string(&f.rollout)
            .unwrap()
            .matches("known complete reply")
            .count(),
        1
    );
}

#[test]
fn incomplete_assistant_reply_uses_exact_adjacent_event_text() {
    let mut f = Fixture::new();
    let mut data = fs::read(&f.rollout).unwrap();
    let prefix=b"{\"timestamp\":\"2026-01-01T00:00:00Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"id\":\"keep-id\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"known reply ";
    let mut event = json!({"timestamp":"2026-01-01T00:00:00Z","type":"event_msg","payload":{"type":"agent_message","message":"known reply "}});
    let pad = (4096
        - (data.len() + serde_json::to_vec(&event).unwrap().len() + 1 + prefix.len()) % 4096)
        % 4096;
    let message = format!("known reply {}", "x".repeat(pad));
    event["payload"]["message"] = message.clone().into();
    let event_bytes = format!("{event}\n");
    data.extend(event_bytes.as_bytes());
    data.extend(prefix);
    assert_eq!(data.len() % 4096, 0);
    fs::write(&f.rollout, &data).unwrap();
    confirmation(&mut f);
    crate::repair(&f.request, &|_| {}, &|| Ok(())).unwrap();
    let result = fs::read_to_string(&f.rollout).unwrap();
    assert!(result.contains(&event_bytes));
    let last: Value = serde_json::from_str(result.lines().last().unwrap()).unwrap();
    assert_eq!(last["payload"]["id"], "keep-id");
    assert_eq!(last["payload"]["content"][0]["text"], message);
}

#[test]
fn commit_failure_restores_original_and_keeps_backup() {
    let mut f = Fixture::new();
    let full=b"{\"timestamp\":\"2026-01-01T00:00:00Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"agent_message\",\"message\":\"known reply\"}}\n";
    let before = aligned_prefix(&f, &full[..full.len() - 6], full);
    confirmation(&mut f);
    let failed = std::cell::Cell::new(false);
    let guard = || {
        if fs::read(&f.rollout)? != before && !failed.replace(true) {
            anyhow::bail!("injected failure after write");
        }
        Ok(())
    };
    assert!(crate::repair(&f.request, &|_| {}, &guard).is_err());
    assert!(failed.get());
    assert_eq!(fs::read(&f.rollout).unwrap(), before);
    let operations = journal::list(&f.request.store_root.join("operations"), false).unwrap();
    assert_eq!(operations[0].state, "Restored");
}

#[test]
fn database_changed_after_planning_prevents_commit() {
    let mut f = Fixture::new();
    let full=b"{\"timestamp\":\"2026-01-01T00:00:00Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"agent_message\",\"message\":\"known reply\"}}\n";
    let before = aligned_prefix(&f, &full[..full.len() - 6], full);
    confirmation(&mut f);
    let changed = std::cell::Cell::new(false);
    let progress = |_: Progress| {
        if !changed.replace(true) {
            Connection::open(f.request.home.join("state_5.sqlite"))
                .unwrap()
                .execute_batch("CREATE TABLE concurrent_change(id TEXT)")
                .unwrap();
        }
    };
    assert!(
        crate::repair(&f.request, &progress, &|| Ok(()))
            .unwrap_err()
            .to_string()
            .contains("数据库发生变化")
    );
    assert_eq!(fs::read(&f.rollout).unwrap(), before);
}

#[test]
fn paginated_reference_protects_legacy_file_with_custom_filename() {
    let f = Fixture::new();
    let full=b"{\"timestamp\":\"2026-01-01T00:00:00Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"agent_message\",\"message\":\"known reply\"}}\n";
    let before = aligned_prefix(&f, &full[..full.len() - 6], full);
    fs::write(f.request.home.join("sessions/child.jsonl"),format!("{}\n",json!({"type":"session_meta","payload":{"id":"child","history_mode":"paginated","history_base":{"thread_id":"old-thread","end_byte_offset":4096}}}))).unwrap();
    assert!(
        inspect(&f.request)
            .unwrap_err()
            .to_string()
            .contains("引用")
    );
    assert_eq!(fs::read(&f.rollout).unwrap(), before);
}

#[test]
fn complete_image_followed_by_unknown_damage_is_not_replaced() {
    let f = Fixture::new();
    let prefix=b"{\"timestamp\":\"2026-01-01T00:00:00Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"custom_tool_call_output\",\"call_id\":\"keep-call\",\"output\":[{\"type\":\"input_image\",\"image_url\":\"data:image/png;base64,AAAA\"}],\"unknown_tail\":\"cut";
    let before = aligned_prefix(&f, prefix, b"");
    assert!(inspect(&f.request).is_err());
    assert_eq!(fs::read(&f.rollout).unwrap(), before);
}
