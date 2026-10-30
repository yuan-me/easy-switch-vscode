//! Explicit, evidence-bound repair of legacy rollouts. No guessing of missing payloads.
use crate::{Result, Settings, history, journal, sessions};
use anyhow::{Context, ensure};
use serde::Serialize;
use serde_json::{Value, json};
use std::{collections::HashSet, fs, path::PathBuf};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairItem {
    pub path: PathBuf,
    pub line: usize,
    pub detail: String,
    pub missing_image: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub fingerprint: String,
    pub file_count: usize,
    pub items: Vec<RepairItem>,
}
struct FileRepair {
    path: PathBuf,
    before: Option<String>,
    after: String,
    bytes: Vec<u8>,
}
pub struct Plan {
    pub summary: Summary,
    files: Vec<FileRepair>,
    databases: Vec<(PathBuf, Option<String>, Option<String>)>,
}

fn database_images(settings: &Settings) -> Result<Vec<(PathBuf, Option<String>, Option<String>)>> {
    sessions::databases(settings)?
        .into_iter()
        .map(|p| {
            let wal = PathBuf::from(format!("{}-wal", p.display()));
            sessions::no_links(&p)?;
            sessions::no_links(&wal)?;
            Ok((p.clone(), journal::hash(&p)?, journal::hash(&wal)?))
        })
        .collect()
}

fn find(raw: &[u8], needle: &[u8]) -> Option<usize> {
    raw.windows(needle.len()).position(|v| v == needle)
}
fn record(raw: &[u8]) -> Result<Value> {
    let v: Value = serde_json::from_slice(raw)?;
    ensure!(
        v.is_object()
            && v["timestamp"].is_string()
            && v["type"].is_string()
            && v["payload"].is_object(),
        "未知历史记录结构"
    );
    Ok(v)
}
fn matching_payload(prefix: &[u8], full: &[u8]) -> bool {
    match (find(prefix, b"\"payload\":"), find(full, b"\"payload\":")) {
        (Some(a), Some(b)) => full[b..].starts_with(&prefix[a..]),
        _ => false,
    }
}

fn repair_line(
    raw: &[u8],
    previous: Option<&[u8]>,
    next: Option<&[u8]>,
    offset: usize,
) -> Result<(Vec<u8>, &'static str, bool)> {
    let starts: Vec<_> = raw
        .windows(14)
        .enumerate()
        .filter_map(|(i, w)| (i > 0 && w == b"{\"timestamp\":\"").then_some(i))
        .collect();
    let complete: Vec<_> = starts
        .into_iter()
        .filter(|&i| record(&raw[i..]).is_ok())
        .collect();
    if complete.len() == 1 {
        let cut = complete[0];
        let prefix = &raw[..cut];
        let short_timestamp = prefix.starts_with(b"{\"timestamp\":\"")
            && prefix.len() <= 38
            && prefix[14..]
                .iter()
                .all(|c| c.is_ascii_digit() || b"-T:.Z+".contains(c));
        ensure!(
            (offset + cut) % 4096 == 0
                && (matching_payload(prefix, &raw[cut..]) || short_timestamp),
            "粘连记录缺少可核实的截断边界或匹配内容"
        );
        return Ok((
            raw[cut..].to_vec(),
            "保留粘连行中的完整记录；截断前缀保存在加密原件备份中，不猜补缺失尾部",
            false,
        ));
    }
    ensure!(complete.is_empty(), "存在多个候选记录，无法确定修复方式");
    let prefix = raw.strip_suffix(b"\n").unwrap_or(raw);
    let prefix = prefix.strip_suffix(b"\r").unwrap_or(prefix);
    ensure!(
        (offset + prefix.len()) % 4096 == 0,
        "不是已验证的 4096 字节边界截断，未自动修复"
    );
    if let Some(next) = next {
        record(next)?;
        ensure!(
            matching_payload(prefix, next),
            "截断片段与后续完整记录不匹配，未自动修复"
        );
        return Ok((
            vec![],
            "保留下一条完整记录；其 payload 包含残片的全部现存 payload 字节，残片另存于加密原件备份",
            false,
        ));
    }
    if let Some(image) = find(raw, b"{\"type\":\"input_image\"") {
        let tail = raw[image..]
            .strip_prefix(b"{\"type\":\"input_image\",\"image_url\":\"data:image/")
            .context("图片结构不匹配，未替换图片")?;
        let separator = find(tail, b";base64,").context("截断图片不是可确认的内嵌图片")?;
        ensure!(
            matches!(&tail[..separator], b"png" | b"jpeg" | b"gif" | b"webp")
                && !tail[separator + 8..].is_empty()
                && tail[separator + 8..]
                    .iter()
                    .all(|c| c.is_ascii_alphanumeric() || b"+/=".contains(c)),
            "无法确认图片字符串在末尾截断，不替换完整图片或未知字段"
        );
        let notice = json!({"type":"input_text","text":"[历史修复说明] 原工具输出图片数据在文件末尾被截断，无法恢复。原始片段保存在加密原件备份中；本说明不是原始工具返回内容。"});
        let mut bytes = raw[..image].to_vec();
        bytes.extend(serde_json::to_vec(&notice)?);
        bytes.extend_from_slice(b"]}}\n");
        let v = record(&bytes)?;
        ensure!(
            v["type"] == "response_item"
                && v["payload"]["type"] == "custom_tool_call_output"
                && v["payload"]["call_id"].is_string()
                && v["payload"]["output"]
                    .as_array()
                    .is_some_and(|a| !a.is_empty()
                        && a.iter()
                            .all(|i| i["type"] == "input_text" && i["text"].is_string())),
            "不能确认截断图片所在工具记录的结构"
        );
        return Ok((
            bytes,
            "图片及未知尾部无法恢复：保留调用 ID 与完整文字，用明确修复说明替代损坏图片；原始字节保存在加密备份",
            true,
        ));
    }
    let prior = record(previous.context("缺少用于恢复回复的相邻事件")?)?;
    ensure!(
        prior["type"] == "event_msg" && prior["payload"]["type"] == "agent_message",
        "缺少可核实的完整回复副本"
    );
    let text = prior["payload"]["message"]
        .as_str()
        .context("完整回复不是文字")?;
    let start = find(raw, b"\"text\":\"").context("无法确定回复截断位置")? + 7;
    let encoded = serde_json::to_vec(text)?;
    ensure!(
        encoded.starts_with(&raw[start..]),
        "完整回复与截断文字不匹配"
    );
    let mut bytes = raw[..start].to_vec();
    bytes.extend(encoded);
    bytes.extend_from_slice(b"}]}}\n");
    let v = record(&bytes)?;
    ensure!(
        v["type"] == "response_item"
            && v["payload"]["type"] == "message"
            && v["payload"]["role"] == "assistant"
            && v["payload"]["content"]
                .as_array()
                .is_some_and(|a| a.len() == 1
                    && a[0]["type"] == "output_text"
                    && a[0]["text"] == text),
        "不能确认回复记录结构"
    );
    Ok((
        bytes,
        "从紧邻的完整 agent_message 恢复回复全文；现存文字严格匹配，保留原 ID、角色和时间戳，未知尾部字段不猜补",
        false,
    ))
}

fn no_projection_references(settings: &Settings, ids: &HashSet<String>) -> Result<()> {
    for path in sessions::databases(settings)? {
        let db = sessions::open(&path)?;
        let check = db
            .prepare("PRAGMA quick_check")?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ensure!(check == ["ok"], "数据库完整性检查未通过，停止修复");
        let tables = db
            .prepare("SELECT name FROM sqlite_master WHERE type='table'")?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for table in tables {
            let cols = sessions::columns(&db, &table)?;
            if !cols.iter().any(|c| c.ends_with("byte_offset")) {
                continue;
            }
            ensure!(
                cols.contains("thread_id"),
                "未知偏移索引结构，不能安全修复：{table}"
            );
            let mut query = db.prepare(&format!(
                "SELECT EXISTS(SELECT 1 FROM {table} WHERE thread_id=?1)"
            ))?;
            for id in ids {
                ensure!(
                    !query.query_row([id], |r| r.get::<_, bool>(0))?,
                    "该聊天存在分页偏移索引，当前修复暂不支持，未修改原件"
                );
            }
        }
    }
    Ok(())
}

impl Plan {
    pub fn read(settings: &Settings, ct: &CancellationToken) -> Result<Self> {
        let mut out = Self {
            summary: Summary {
                fingerprint: String::new(),
                file_count: 0,
                items: vec![],
            },
            files: vec![],
            databases: vec![],
        };
        let mut ids = HashSet::new();
        let mut paths = vec![];
        for folder in ["sessions", "archived_sessions"] {
            let root = settings.codex_home.join(folder);
            if !root.try_exists()? {
                continue;
            }
            sessions::no_links(&root)?;
            for entry in walkdir::WalkDir::new(root).follow_links(false) {
                let entry = entry?;
                ensure!(
                    !entry.file_type().is_symlink(),
                    "会话目录包含符号链接，停止修复检查"
                );
                if entry.file_type().is_file()
                    && entry.path().extension().is_some_and(|e| e == "jsonl")
                {
                    paths.push(sessions::owned(entry.path(), settings)?);
                }
            }
        }
        paths.sort();
        for path in paths {
            ensure!(!ct.is_cancelled(), "操作已取消");
            match history::validate_records(&path, ct) {
                Ok(()) => continue,
                Err(e) if e.downcast_ref::<history::CorruptRecord>().is_some() => {}
                Err(e) => return Err(e),
            }
            // shortcut: bounded in-memory repair of damaged files; larger files need a streaming planner.
            ensure!(
                fs::metadata(&path)?.len() <= 512 * 1024 * 1024,
                "损坏聊天超过 512 MiB，当前修复暂不支持"
            );
            let before = journal::hash(&path)?;
            let data = fs::read(&path)?;
            ensure!(
                Some(journal::hash_bytes(&data)) == before,
                "检查期间聊天发生变化"
            );
            let rows: Vec<_> = data.split_inclusive(|c| *c == b'\n').collect();
            let meta: Value = serde_json::from_slice(rows.first().context("空聊天文件无法修复")?)?;
            ensure!(
                meta["type"] == "session_meta"
                    && meta["payload"]["id"].is_string()
                    && (meta["payload"]["history_mode"].is_null()
                        || meta["payload"]["history_mode"] == "legacy")
                    && meta["payload"]["history_base"].is_null()
                    && meta.get("ordinal").is_none(),
                "当前修复仅支持元数据完整的普通聊天，分页或未知结构未修改"
            );
            ids.insert(meta["payload"]["id"].as_str().unwrap().to_owned());
            if let Some(id) = history::rollout_id(&path) {
                ids.insert(id);
            }
            let mut bytes = vec![];
            let mut offset = 0;
            for (i, raw) in rows.iter().enumerate() {
                ensure!(!ct.is_cancelled(), "操作已取消");
                if raw.iter().all(u8::is_ascii_whitespace)
                    || history::parse_record(raw, &path, (i + 1) as u64).is_ok()
                {
                    bytes.extend_from_slice(raw);
                } else {
                    let (replacement, detail, missing_image) = repair_line(
                        raw,
                        i.checked_sub(1).map(|n| rows[n]),
                        rows.get(i + 1).copied(),
                        offset,
                    )
                    .with_context(|| {
                        format!(
                            "无法自动修复 {} 第 {} 行；保留原件，请检查修复条件",
                            path.display(),
                            i + 1
                        )
                    })?;
                    bytes.extend(replacement);
                    out.summary.items.push(RepairItem {
                        path: path.clone(),
                        line: i + 1,
                        detail: detail.into(),
                        missing_image,
                    });
                }
                offset += raw.len();
            }
            for (i, raw) in bytes.split_inclusive(|c| *c == b'\n').enumerate() {
                if !raw.iter().all(u8::is_ascii_whitespace) {
                    history::parse_record(raw, &path, (i + 1) as u64)?;
                }
            }
            ensure!(
                bytes.starts_with(rows[0]) && journal::hash(&path)? == before,
                "生成修复方案期间原件变化"
            );
            out.files.push(FileRepair {
                path,
                before,
                after: journal::hash_bytes(&bytes),
                bytes,
            });
        }
        if !out.files.is_empty() {
            out.databases = database_images(settings)?;
            no_projection_references(settings, &ids)?;
            // A legacy rollout can still be referenced by another thread's paginated history.
            let paths: Vec<_> = out.files.iter().map(|f| f.path.clone()).collect();
            history::ensure_unreferenced(settings, &paths)?;
            ensure!(
                database_images(settings)? == out.databases,
                "检查期间数据库发生变化，请停止写入后重试"
            );
        }
        out.summary.file_count = out.files.len();
        out.summary.fingerprint = journal::hash_bytes(&serde_json::to_vec(
            &out.files
                .iter()
                .map(|f| (&f.path, &f.before, &f.after))
                .collect::<Vec<_>>(),
        )?);
        Ok(out)
    }
    pub fn stage(&self, j: &mut journal::Journal) -> Result<()> {
        for file in &self.files {
            let temp = j.dir.join(format!("{}.jsonl", uuid::Uuid::new_v4()));
            use std::io::Write;
            let mut output = crate::store::private_file(&temp, true)?;
            output.write_all(&file.bytes)?;
            output.sync_all()?;
            drop(output);
            let result = j.stage_file_from_snapshot(&file.path, &temp, &file.before);
            let _ = fs::remove_file(temp);
            result?;
        }
        Ok(())
    }
    pub fn verify(&self) -> Result<()> {
        for file in &self.files {
            sessions::no_links(&file.path)?;
            ensure!(
                journal::hash(&file.path)?.as_deref() == Some(file.after.as_str()),
                "修复后文件校验失败"
            );
            history::validate_records(&file.path, &CancellationToken::new())?;
        }
        Ok(())
    }
    pub fn verify_databases(&self, settings: &Settings) -> Result<()> {
        ensure!(
            database_images(settings)? == self.databases,
            "修复期间数据库发生变化，停止操作并恢复原件"
        );
        history::ensure_unreferenced(
            settings,
            &self
                .files
                .iter()
                .map(|f| f.path.clone())
                .collect::<Vec<_>>(),
        )?;
        Ok(())
    }
}
