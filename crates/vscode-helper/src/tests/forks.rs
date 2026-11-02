use super::*;
#[cfg(not(windows))]
use std::io::{BufRead, BufReader};

#[cfg(not(windows))]
fn sample_forks(root: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        let kind = entry.file_type().unwrap();
        if kind.is_dir() {
            sample_forks(&entry.path(), found);
        } else if kind.is_file() && entry.path().extension().is_some_and(|e| e == "jsonl") {
            let first = BufReader::new(fs::File::open(entry.path()).unwrap())
                .lines()
                .next()
                .unwrap()
                .unwrap();
            let Ok(meta) = serde_json::from_str::<Value>(&first) else {
                continue;
            };
            if meta["type"] == "session_meta"
                && meta["payload"]["forked_from_id"].is_string()
                && meta["payload"]["subagent_history_start_ordinal"].is_null()
            {
                found.push(entry.path());
            }
        }
    }
}

#[test]
#[cfg(not(windows))]
#[ignore = "private fork samples; only disposable fixtures may be changed"]
fn private_fork_preflight_and_owned_model_switch() {
    let source = PathBuf::from(std::env::var_os("EASY_SWITCH_FORK_SAMPLES").unwrap());
    let mut found = vec![];
    sample_forks(&source, &mut found);
    assert_eq!(found.len(), 7);
    let mut switched = 0;
    let mut missing_settings = 0;
    for path in found {
        let hash = journal::hash(&path).unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        let meta: Value = serde_json::from_str(raw.lines().next().unwrap()).unwrap();
        let id = meta["payload"]["id"].as_str().unwrap();
        let mut f = Fixture::new();
        fs::write(&f.rollout, &raw).unwrap();
        Connection::open(f.request.home.join("state_5.sqlite"))
            .unwrap()
            .execute(
                "UPDATE threads SET id=?1,model_provider=?2",
                params![id, meta["payload"]["model_provider"].as_str().unwrap()],
            )
            .unwrap();
        let mut status = f.request.clone();
        status.action = "inspect".into();
        assert_eq!(inspect(&status).unwrap()["sessionCount"], 1);
        assert_eq!(fs::read_to_string(&f.rollout).unwrap(), raw);
        f.select("a", "test-api-model");
        match inspect(&f.request) {
            Ok(_) => {
                f.run().unwrap();
                let after = fs::read_to_string(&f.rollout).unwrap();
                assert!(
                    after
                        .split_once('\n')
                        .unwrap()
                        .1
                        .starts_with(raw.split_once('\n').unwrap().1)
                );
                let last: Value = serde_json::from_str(after.lines().last().unwrap()).unwrap();
                assert_eq!(last["payload"]["thread_id"], id);
                assert_eq!(
                    last["payload"]["thread_settings"]["model"],
                    "test-api-model"
                );
                f.select("official", "");
                f.run().unwrap();
                switched += 1;
            }
            Err(e) => {
                assert!(
                    e.to_string()
                        .contains("旧聊天缺少归属于当前线程的完整设置快照"),
                    "{e}"
                );
                assert_eq!(fs::read_to_string(&f.rollout).unwrap(), raw);
                missing_settings += 1;
            }
        }
        assert_eq!(journal::hash(&path).unwrap(), hash);
    }
    assert_eq!((switched, missing_settings), (1, 6));
    println!(
        "7 sample copies scanned; 1 model/provider round trip passed; 6 missing settings snapshots explicitly rejected without writes"
    );
}

#[test]
fn ordinary_forks_round_trip_provider_and_model_without_rewriting_ancestry() {
    for modern in [false, true] {
        for depth in [1, 2] {
            let mut f = Fixture::new();
            let ids: Vec<_> = (0..=depth)
                .map(|n| format!("00000000-0000-4000-8000-{n:012}"))
                .collect();
            let mut metadata = String::new();
            for (n, id) in ids.iter().enumerate() {
                let mut payload = json!({"id":id,"cwd":f.request.project_dirs[0],"model_provider":"openai","source":"vscode"});
                if n < depth {
                    payload["forked_from_id"] = ids[n + 1].clone().into();
                }
                if modern {
                    payload["session_id"] = format!("00000000-0000-4000-9000-{n:012}").into();
                    payload["history_mode"] = "legacy".into();
                }
                metadata.push_str(&format!(
                    "{}\n",
                    json!({"type":"session_meta","payload":payload})
                ));
            }
            f.body = f.body.replace("old-thread", &ids[0]);
            fs::write(&f.rollout, format!("{metadata}{}", f.body)).unwrap();
            Connection::open(f.request.home.join("state_5.sqlite"))
                .unwrap()
                .execute("UPDATE threads SET id=?1,archived=1", [&ids[0]])
                .unwrap();
            let original = fs::read_to_string(&f.rollout).unwrap();
            let inherited = original.split_once('\n').unwrap().1;
            let auth = fs::read(f.request.home.join("auth.json")).unwrap();
            for (id, model, provider) in [
                ("a", "api-a", PROVIDER),
                ("b", "api-b", PROVIDER),
                ("official", "", "openai"),
            ] {
                f.select(id, model);
                assert!(inspect(&f.request).is_ok());
                f.run().unwrap();
                let text = fs::read_to_string(&f.rollout).unwrap();
                assert!(text.split_once('\n').unwrap().1.starts_with(inherited));
                let first: Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
                assert_eq!(first["payload"]["id"], ids[0]);
                assert_eq!(first["payload"]["model_provider"], provider);
                let last: Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
                assert_eq!(last["payload"]["thread_id"], ids[0]);
                assert_eq!(
                    last["payload"]["thread_settings"]["model"],
                    if id == "official" {
                        "official-model"
                    } else {
                        model
                    }
                );
                let row: (String, i64, i64) =
                    Connection::open(f.request.home.join("state_5.sqlite"))
                        .unwrap()
                        .query_row(
                            "SELECT model_provider,archived,updated_at FROM threads WHERE id=?1",
                            [&ids[0]],
                            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                        )
                        .unwrap();
                assert_eq!(row, (provider.into(), 1, 42));
                assert_eq!(fs::read(f.request.home.join("auth.json")).unwrap(), auth);
            }
        }
    }
}
