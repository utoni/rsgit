use crate::http1::{Body, Request, Response, StatusCode, with_timeout};
use crate::{AppState, config::config as Config, git::repository::valid_repo_name};
use futures_lite::{AsyncReadExt, AsyncWriteExt};
use smol::process::Command;
use std::path::{Path as FsPath, PathBuf};
use std::process::Stdio;
use std::time::Duration;

fn status_resp(code: StatusCode, msg: &str) -> Response {
    let mut res = Response::new(code);
    if !msg.is_empty() {
        res.set_body(msg.to_string());
    }
    res
}

pub async fn info_refs(st: &AppState, repo: &str, query: Option<&str>) -> Response {
    if query_service(query).as_deref() != Some("git-upload-pack") {
        return status_resp(StatusCode::FORBIDDEN, "Only git-upload-pack is supported");
    }
    let Some(dir) = repo_dir(st.root.as_path(), repo) else {
        return status_resp(StatusCode::NOT_FOUND, "Repository not found");
    };

    let mut child = match Command::new("/usr/bin/git")
        .kill_on_drop(true)
        .args(["upload-pack", "--stateless-rpc", "--advertise-refs"])
        .arg(&dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to spawn git: {e}");
            return status_resp(StatusCode::INTERNAL_SERVER_ERROR, "");
        }
    };

    let Some(mut stdout) = child.stdout.take() else {
        eprintln!("Failed to read stdout from git executable");
        return status_resp(StatusCode::INTERNAL_SERVER_ERROR, "");
    };
    let mut buf = Vec::new();
    let read = async { stdout.read_to_end(&mut buf).await };
    match with_timeout(Duration::from_secs(Config().git_clone_refs_timeout), read).await {
        Some(Ok(_)) => {}
        Some(Err(e)) => {
            eprintln!("info/refs read error: {e}");
            let _ = child.kill();
            return status_resp(StatusCode::INTERNAL_SERVER_ERROR, "");
        }
        None => {
            eprintln!("info/refs timeout");
            let _ = child.kill();
            return status_resp(StatusCode::INTERNAL_SERVER_ERROR, "");
        }
    }
    match child.status().await {
        Ok(s) if s.success() => {}
        _ => return status_resp(StatusCode::INTERNAL_SERVER_ERROR, ""),
    }

    let mut body = pkt_line("# service=git-upload-pack\n");
    body.extend_from_slice(b"0000");
    body.extend_from_slice(&buf);

    let mut res = Response::new(StatusCode::OK);
    res.insert_header(
        "content-type",
        "application/x-git-upload-pack-advertisement",
    );
    res.insert_header("cache-control", "no-cache");
    res.set_body(body);
    res
}

pub async fn upload_pack(st: &AppState, repo: &str, mut req: Request) -> Response {
    let Some(dir) = repo_dir(st.root.as_path(), repo) else {
        return status_resp(StatusCode::NOT_FOUND, "Repository not found");
    };

    let permit = match st.clone_limit.try_acquire_arc() {
        Some(p) => p,
        None => return status_resp(StatusCode::SERVICE_UNAVAILABLE, "Too many clients to serve"),
    };

    let gzipped = req
        .header("content-encoding")
        .map(|v| v == "gzip")
        .unwrap_or(false);

    let raw = req.take_body();
    if raw.len() > Config().max_body_bytes {
        return status_resp(StatusCode::BAD_REQUEST, "Allowed request size exceeded");
    }

    let input = if gzipped {
        match gunzip(&raw) {
            Ok(d) => d,
            Err(_) => return status_resp(StatusCode::BAD_REQUEST, ""),
        }
    } else {
        raw
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
        Err(_) => {
            eprintln!("Failed to spawn git");
            return status_resp(StatusCode::INTERNAL_SERVER_ERROR, "");
        }
    };

    let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        eprintln!("Could not acquire stdin and stdout from spawned git child");
        return status_resp(StatusCode::INTERNAL_SERVER_ERROR, "");
    };

    smol::spawn(async move {
        let _ = stdin.write_all(&input).await;
    })
    .detach();

    smol::spawn(async move {
        let _permit = permit;
        let _ = child.status().await;
    })
    .detach();

    let reader = futures_lite::io::BufReader::new(stdout);
    let mut res = Response::new(StatusCode::OK);
    res.insert_header("content-type", "application/x-git-upload-pack-result");
    res.insert_header("cache-control", "no-cache");
    res.set_body(Body::from_reader(reader));
    res
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
