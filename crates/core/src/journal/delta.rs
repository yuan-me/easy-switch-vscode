use super::*;
use std::io::BufRead;

const LIMIT: u64 = 1024 * 1024;

#[derive(Serialize, Deserialize)]
struct Delta {
    start: u64,
    end: u64,
    head: Vec<u8>,
    tail: Vec<u8>,
}

fn boundary(file: &mut File, start: u64) -> Result<Option<u64>> {
    file.seek(SeekFrom::Start(start))?;
    let mut line = Vec::new();
    std::io::BufReader::new(file.take(LIMIT)).read_until(b'\n', &mut line)?;
    Ok((line.last() == Some(&b'\n')).then_some(start + line.len() as u64))
}

fn parts(file: &mut File, start: u64, end: u64) -> Result<(Vec<u8>, Vec<u8>)> {
    file.rewind()?;
    let mut head = Vec::new();
    Read::by_ref(file).take(start).read_to_end(&mut head)?;
    file.seek(SeekFrom::Start(end))?;
    let mut tail = Vec::new();
    Read::by_ref(file).take(LIMIT + 1).read_to_end(&mut tail)?;
    ensure!(
        head.len() as u64 + tail.len() as u64 <= LIMIT,
        "差量生成期间源文件增长"
    );
    Ok((head, tail))
}

// Preserve the shared body when changing metadata and appending a settings event.
pub(super) fn prepare(
    old: &mut File,
    new: &mut File,
    prefix: u64,
) -> Result<Option<(Vec<u8>, Vec<u8>)>> {
    let old_len = old.metadata()?.len();
    let new_len = new.metadata()?.len();
    ensure!(prefix <= old_len.min(new_len), "差量生成期间源文件被截断");
    let (a, b, mut shared) =
        if prefix >= 65536 && old_len - prefix <= LIMIT && new_len - prefix <= LIMIT {
            (0, 0, prefix)
        } else {
            let (Some(a), Some(b)) = (boundary(old, prefix)?, boundary(new, prefix)?) else {
                return Ok(None);
            };
            if a > LIMIT || b > LIMIT || a > old_len || b > new_len {
                return Ok(None);
            }
            (a, b, 0)
        };
    old.seek(SeekFrom::Start(a + shared))?;
    new.seek(SeekFrom::Start(b + shared))?;
    let mut x = [0; 65536];
    let mut y = [0; 65536];
    let limit = (old_len - a).min(new_len - b);
    while shared < limit {
        let n = (limit - shared).min(x.len() as u64) as usize;
        old.read_exact(&mut x[..n])?;
        new.read_exact(&mut y[..n])?;
        let equal = x[..n]
            .iter()
            .zip(&y[..n])
            .position(|(x, y)| x != y)
            .unwrap_or(n);
        shared += equal as u64;
        if equal != n {
            break;
        }
    }
    if shared < 65536 || old_len - shared > LIMIT || new_len - shared > LIMIT {
        return Ok(None);
    }
    let (head, tail) = parts(old, a, a + shared)?;
    let before = serde_json::to_vec(&Delta {
        start: b,
        end: b + shared,
        head,
        tail,
    })?;
    let (head, tail) = parts(new, b, b + shared)?;
    let after = serde_json::to_vec(&Delta {
        start: a,
        end: a + shared,
        head,
        tail,
    })?;
    Ok(Some((before, after)))
}

pub(super) fn write(payload: &Path, source: &Path, mut out: impl Write) -> Result<()> {
    ensure!(
        fs::metadata(payload)?.len() <= 8 * LIMIT,
        "差量文件超过限制"
    );
    let mut bytes = Vec::new();
    crypto::decrypt_file(payload, &mut bytes)?;
    let delta: Delta = serde_json::from_slice(&bytes)?;
    let mut source = File::open(source)?;
    ensure!(
        delta.start <= delta.end && delta.end <= source.metadata()?.len(),
        "差量范围无效"
    );
    out.write_all(&delta.head)?;
    source.seek(SeekFrom::Start(delta.start))?;
    let size = delta.end - delta.start;
    ensure!(
        std::io::copy(&mut source.take(size), &mut out)? == size,
        "差量源文件被截断"
    );
    out.write_all(&delta.tail)?;
    Ok(())
}
