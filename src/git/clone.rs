use crate::{git::repository::valid_repo_name, AppState};
use axum::{
    body::Body,
    extract::{Path, RawQuery, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use std::path::{Path as FsPath, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio_util::io::ReaderStream;

const INFO_REFS_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_REQUEST_BYTES: usize = 50 * 1024 * 1024;

pub async fn info_refs(
    State(st): State<AppState>,
    Path(repo): Path<String>,
    RawQuery(q): RawQuery,
) -> Response {
    if query_service(q.as_deref()).as_deref() != Some("git-upload-pack") {
        return (StatusCode::FORBIDDEN, "only git-upload-pack is supported").into_response();
    }
    let Some(dir) = repo_dir(st.root.as_path(), &repo) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let fut = Command::new("/usr/bin/git")
        .kill_on_drop(true)
        .args(["upload-pack", "--stateless-rpc", "--advertise-refs"])
        .arg(&dir)
        .output();

    let out = match tokio::time::timeout(INFO_REFS_TIMEOUT, fut).await {
        Ok(Ok(o)) if o.status.success() => o,
        _ => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let mut body = pkt_line("# service=git-upload-pack\n");
    body.extend_from_slice(b"0000");
    body.extend_from_slice(&out.stdout);

    (
        [
            (
                header::CONTENT_TYPE,
                "application/x-git-upload-pack-advertisement",
            ),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        body,
    )
        .into_response()
}

pub async fn upload_pack(
    State(st): State<AppState>,
    Path(repo): Path<String>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let Some(dir) = repo_dir(st.root.as_path(), &repo) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let permit = match st.clone_limit.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };

    let raw = match axum::body::to_bytes(body, MAX_REQUEST_BYTES).await {
        Ok(b) => b,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let gzipped = headers
        .get(header::CONTENT_ENCODING)
        .map(|v| v.as_bytes() == b"gzip")
        .unwrap_or(false);
    let input = if gzipped {
        match gunzip(&raw) {
            Ok(d) => d,
            Err(_) => return StatusCode::BAD_REQUEST.into_response(),
        }
    } else {
        raw.to_vec()
    };

    let mut child = match Command::new("/usr/bin/git")
        .kill_on_drop(true)
        .args(["upload-pack", "--stateless-rpc"])
        .arg(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };

    tokio::spawn(async move {
        let _ = stdin.write_all(&input).await;
    });

    tokio::spawn(async move {
        let _permit = permit;
        let _ = child.wait().await;
    });

    let stream = ReaderStream::new(stdout);
    (
        [
            (header::CONTENT_TYPE, "application/x-git-upload-pack-result"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        Body::from_stream(stream),
    )
        .into_response()
}

fn pkt_line(s: &str) -> Vec<u8> {
    let mut v = format!("{:04x}", s.len() + 4).into_bytes();
    v.extend_from_slice(s.as_bytes());
    v
}

fn query_service(q: Option<&str>) -> Option<String> {
    q?.split('&')
        .find_map(|p| p.strip_prefix("service=").map(|s| s.to_string()))
}

fn repo_dir(root: &FsPath, name: &str) -> Option<PathBuf> {
    if !valid_repo_name(name) {
        return None;
    }
    for cand in [root.join(name), root.join(format!("{name}.git"))] {
        if cand.join("HEAD").is_file() || cand.join(".git/HEAD").is_file() {
            return Some(cand);
        }
    }
    None
}

fn gunzip(data: &[u8]) -> std::io::Result<Vec<u8>> {
    use flate2::read::GzDecoder;
    use std::io::Read;
    let mut out = Vec::new();
    GzDecoder::new(data).read_to_end(&mut out)?;
    Ok(out)
}
