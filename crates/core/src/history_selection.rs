//! Preserve entire damaged histories and their connected pagination components.
use crate::{Result, Settings, history, journal, sessions};
use anyhow::{Context, ensure};
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Serialize)]
pub struct SkippedHistory {
    pub path: PathBuf,
    pub reason: String,
}

struct FileInfo {
    path: PathBuf,
    hash: String,
    header: Option<history::Header>,
    ids: HashSet<String>,
    reason: Option<String>,
}

pub struct Selection {
    pub excluded: HashSet<PathBuf>,
    pub skipped: Vec<SkippedHistory>,
    hashes: BTreeMap<PathBuf, String>,
    indexes: Vec<IndexImage>,
}

struct IndexImage {
    database: PathBuf,
    table: &'static str,
    key: &'static str,
    id: String,
    hash: String,
}

fn row_hash(db: &rusqlite::Connection, table: &str, key: &str, id: &str) -> Result<String> {
    // Table/key names come only from the fixed list below. Values never enter SQL text.
    let mut query = db.prepare(&format!("SELECT * FROM {table} WHERE {key}=?1"))?;
    let columns = query.column_count();
    let mut rows = query
        .query_map([id], |r| {
            (0..columns)
                .map(|i| r.get::<_, rusqlite::types::Value>(i))
                .collect::<rusqlite::Result<Vec<_>>>()
        })?
        .map(|r| r.map(|v| format!("{v:?}")))
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.sort();
    Ok(journal::hash_bytes(&serde_json::to_vec(&rows)?))
}

impl Selection {
    pub fn read(settings: &Settings, ct: &CancellationToken) -> Result<Self> {
        let mut files = vec![];
        for folder in ["sessions", "archived_sessions"] {
            let root = settings.codex_home.join(folder);
            if !root.try_exists()? {
                continue;
            }
            sessions::no_links(&root)?;
            for entry in walkdir::WalkDir::new(root).follow_links(false) {
                ensure!(!ct.is_cancelled(), "操作已取消");
                let entry = entry?;
                ensure!(
                    !entry.file_type().is_symlink(),
                    "会话目录包含符号链接，无法确认关联关系"
                );
                if !entry.file_type().is_file()
                    || entry.path().extension().is_none_or(|e| e != "jsonl")
                {
                    continue;
                }
                let path = sessions::owned(entry.path(), settings)?;
                let hash = journal::hash(&path)?.context("聊天文件在检查期间消失")?;
                let reason = match history::validate_records(&path, ct) {
                    Ok(()) => None,
                    Err(e) if e.downcast_ref::<history::CorruptRecord>().is_some() => {
                        Some(e.to_string().trim_end_matches("；已停止处理").to_owned())
                    }
                    Err(e) => {
                        return Err(e.context(format!("无法检查聊天文件：{}", path.display())));
                    }
                };
                let header = match history::header(&path) {
                    Ok(h) => Some(h),
                    Err(_) if reason.is_some() => None,
                    Err(e) => {
                        return Err(e.context(format!("无法确认聊天结构：{}", path.display())));
                    }
                };
                ensure!(
                    journal::hash(&path)?.as_deref() == Some(hash.as_str()),
                    "检查期间聊天文件发生变化，请停止生成后重试：{}",
                    path.display()
                );
                let ids = header
                    .as_ref()
                    .map(|h| HashSet::from([h.id.clone()]))
                    .unwrap_or_default();
                files.push(FileInfo {
                    path,
                    hash,
                    header,
                    ids,
                    reason,
                });
            }
        }
        let by_path = files
            .iter()
            .enumerate()
            .map(|(i, f)| Ok((sessions::path_key(&f.path)?, i)))
            .collect::<Result<HashMap<_, _>>>()?;
        let databases = sessions::databases(settings)?;
        // Index identities also connect files whose damaged metadata cannot be read.
        for database in &databases {
            let db = sessions::open(database)?;
            let cols = sessions::columns(&db, "threads")?;
            if cols.is_empty() {
                continue;
            }
            ensure!(
                cols.contains("id") && cols.contains("rollout_path"),
                "线程索引缺少身份或历史路径"
            );
            let mut query = db.prepare("SELECT id,rollout_path FROM threads")?;
            let rows =
                query.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            for row in rows {
                let (id, path) = row?;
                let key = sessions::path_key(&sessions::owned(Path::new(&path), settings)?)?;
                if let Some(&i) = by_path.get(&key) {
                    files[i].ids.insert(id);
                }
            }
        }
        let mut edges = vec![vec![]; files.len()];
        let mut by_id = HashMap::new();
        let mut by_rollout = HashMap::new();
        for (i, f) in files.iter().enumerate() {
            for id in &f.ids {
                if let Some(j) = by_id.insert(id.clone(), i) {
                    edges[i].push(j);
                    edges[j].push(i);
                }
            }
            if let Some(id) = history::rollout_id(&f.path) {
                ensure!(
                    by_rollout.insert(id, i).is_none(),
                    "历史文件标识重复，无法确认关联关系"
                );
            }
        }
        for (i, f) in files.iter().enumerate() {
            if let Some(base) = f.header.as_ref().and_then(|h| h.base.as_ref()) {
                if let Some(&j) = by_rollout.get(base) {
                    edges[i].push(j);
                    edges[j].push(i);
                }
            }
        }
        // An unreadable header could reference any paginated parent. Do not rebase any of them.
        let unknown_links = files
            .iter()
            .any(|f| f.reason.is_some() && f.header.is_none());
        let mut blocked = HashSet::new();
        let mut pending = vec![];
        for (i, f) in files.iter().enumerate() {
            if f.reason.is_some()
                || (unknown_links
                    && f.header
                        .as_ref()
                        .is_some_and(|h| h.paginated || h.base.is_some()))
            {
                pending.push(i);
            }
        }
        while let Some(i) = pending.pop() {
            if blocked.insert(i) {
                pending.extend(edges[i].iter().copied());
            }
        }
        let mut out = Self {
            excluded: HashSet::new(),
            skipped: vec![],
            hashes: BTreeMap::new(),
            indexes: vec![],
        };
        let mut ids = HashSet::new();
        for (i, f) in files.into_iter().enumerate() {
            if !blocked.contains(&i) {
                continue;
            }
            out.excluded.insert(sessions::path_key(&f.path)?);
            ids.extend(f.ids);
            if let Some(id) = history::rollout_id(&f.path) {
                ids.insert(id);
            }
            out.hashes.insert(f.path.clone(), f.hash);
            out.skipped.push(SkippedHistory {
                path: f.path,
                reason: f.reason.unwrap_or_else(|| {
                    if unknown_links {
                        "有损坏元数据无法确认分页关联，此聊天一并保留".into()
                    } else {
                        "与损坏聊天共享历史文件或分页引用，一并保留".into()
                    }
                }),
            });
        }
        out.skipped.sort_by(|a, b| a.path.cmp(&b.path));
        if !out.excluded.is_empty() {
            for database in databases {
                let db = sessions::open(&database)?;
                for (table, key) in [
                    ("threads", "id"),
                    ("local_thread_catalog", "thread_id"),
                    ("thread_turns", "thread_id"),
                    ("thread_history_projection_state", "thread_id"),
                ] {
                    let cols = sessions::columns(&db, table)?;
                    if cols.is_empty() {
                        continue;
                    }
                    ensure!(cols.contains(key), "无法保护被跳过聊天的索引：{table}");
                    for id in &ids {
                        out.indexes.push(IndexImage {
                            database: database.clone(),
                            table,
                            key,
                            id: id.clone(),
                            hash: row_hash(&db, table, key, id)?,
                        });
                    }
                }
            }
        }
        Ok(out)
    }

    pub fn verify_preserved(&self) -> Result<()> {
        for (path, hash) in &self.hashes {
            sessions::no_links(path)?;
            ensure!(
                journal::hash(path)?.as_deref() == Some(hash.as_str()),
                "被跳过的聊天发生变化，停止提交：{}",
                path.display()
            );
        }
        for image in &self.indexes {
            sessions::no_links(&image.database)?;
            let db = sessions::open(&image.database)?;
            ensure!(
                row_hash(&db, image.table, image.key, &image.id)? == image.hash,
                "被跳过聊天的索引发生变化，停止提交：{}",
                image.table
            );
        }
        Ok(())
    }
}
