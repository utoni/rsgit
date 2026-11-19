use futures_lite::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader,
};
use http_types::headers::{CONNECTION, CONTENT_LENGTH, EXPECT, TRANSFER_ENCODING};
use http_types::{Method, Request, Response, StatusCode, Url};
use std::future::Future;
use std::io;
use std::str::FromStr;
use std::time::Duration;

const MAX_HEADERS: usize = 128;
const MAX_HEAD_BYTES: usize = 8 * 1024;
const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
const HEAD_TIMEOUT: Duration = Duration::from_secs(30);
const CONTINUE_RESPONSE: &[u8] = b"HTTP/1.1 100 Continue\r\n\r\n";

#[derive(Debug)]
enum ReadError {
    Status(StatusCode),
    Io,
}

impl From<io::Error> for ReadError {
    fn from(_: io::Error) -> Self {
        ReadError::Io
    }
}

fn bad_request<E>(_: E) -> ReadError {
    ReadError::Status(StatusCode::BadRequest)
}

pub(crate) async fn with_timeout<T>(dur: Duration, fut: impl Future<Output = T>) -> Option<T> {
    let work = async move { Some(fut.await) };
    let timer = async move {
        smol::Timer::after(dur).await;
        None
    };
    futures_lite::FutureExt::or(work, timer).await
}

pub async fn serve_connection<F, Fut>(stream: smol::net::TcpStream, handler: F)
where
    F: Fn(Request) -> Fut,
    Fut: Future<Output = Response>,
{
    let mut writer = stream.clone();
    let mut reader = BufReader::new(stream);
    loop {
        let (req, keep_alive) =
            match with_timeout(HEAD_TIMEOUT, read_request(&mut reader, &mut writer)).await {
                Some(Ok(Some(parsed))) => parsed,
                Some(Err(ReadError::Status(code))) => {
                    let _ = write_response(&mut writer, Response::new(code), false).await;
                    return;
                }
                Some(Ok(None)) | Some(Err(ReadError::Io)) | None => return,
            };
        let res = handler(req).await;
        if write_response(&mut writer, res, keep_alive).await.is_err() || !keep_alive {
            return;
        }
    }
}

async fn read_request<R, W>(
    reader: &mut R,
    writer: &mut W,
) -> Result<Option<(Request, bool)>, ReadError>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut head = Vec::new();
    let mut line = Vec::new();
    loop {
        line.clear();
        let n = reader.read_until(b'\n', &mut line).await?;
        if n == 0 {
            return if head.is_empty() {
                Ok(None)
            } else {
                Err(ReadError::Status(StatusCode::BadRequest))
            };
        }
        let blank = line == b"\r\n" || line == b"\n";
        if head.is_empty() && blank {
            continue;
        }
        head.extend_from_slice(&line);
        if head.len() > MAX_HEAD_BYTES {
            return Err(ReadError::Status(StatusCode::RequestHeaderFieldsTooLarge));
        }
        if blank {
            break;
        }
    }
    let mut raw_headers = [httparse::EMPTY_HEADER; MAX_HEADERS];
    let mut parsed = httparse::Request::new(&mut raw_headers);
    match parsed.parse(&head) {
        Ok(httparse::Status::Complete(_)) => {}
        _ => return Err(ReadError::Status(StatusCode::BadRequest)),
    }
    let method = Method::from_str(
        parsed
            .method
            .ok_or(ReadError::Status(StatusCode::BadRequest))?,
    )
    .map_err(bad_request)?;
    let http11 = match parsed.version {
        Some(1) => true,
        Some(0) => false,
        _ => return Err(ReadError::Status(StatusCode::BadRequest)),
    };
    let target = parsed
        .path
        .filter(|p| p.starts_with('/'))
        .ok_or(ReadError::Status(StatusCode::BadRequest))?;
    let url = Url::parse(&format!("http://localhost{target}")).map_err(bad_request)?;
    let mut req = Request::new(method, url);
    for h in parsed.headers.iter() {
        let value = std::str::from_utf8(h.value).map_err(bad_request)?;
        req.append_header(h.name, value);
    }
    let chunked = match req.header(TRANSFER_ENCODING) {
        Some(te) => {
            if !te.as_str().trim().eq_ignore_ascii_case("chunked") {
                return Err(ReadError::Status(StatusCode::BadRequest));
            }
            true
        }
        None => false,
    };
    let content_length = req
        .header(CONTENT_LENGTH)
        .map(|v| v.as_str().trim().parse::<usize>())
        .transpose()
        .map_err(bad_request)?;
    if chunked && content_length.is_some() {
        return Err(ReadError::Status(StatusCode::BadRequest));
    }
    let connection = req
        .header(CONNECTION)
        .map(|v| v.as_str().to_ascii_lowercase())
        .unwrap_or_default();
    let has_token = |t: &str| connection.split(',').any(|s| s.trim() == t);
    let keep_alive = if has_token("close") {
        false
    } else if http11 {
        true
    } else {
        has_token("keep-alive")
    };
    let expects_body = chunked || content_length.is_some_and(|len| len > 0);
    if expects_body {
        if content_length.is_some_and(|len| len > MAX_BODY_BYTES) {
            return Err(ReadError::Status(StatusCode::PayloadTooLarge));
        }
        if req
            .header(EXPECT)
            .is_some_and(|v| v.as_str().eq_ignore_ascii_case("100-continue"))
        {
            writer.write_all(CONTINUE_RESPONSE).await?;
            writer.flush().await?;
        }
    }
    let body = if chunked {
        read_chunked(reader).await?
    } else if let Some(len) = content_length {
        let mut buf = vec![0u8; len];
        reader.read_exact(&mut buf).await?;
        buf
    } else {
        Vec::new()
    };
    req.set_body(body);
    Ok(Some((req, keep_alive)))
}

async fn read_line<R: AsyncBufRead + Unpin>(reader: &mut R) -> Result<Vec<u8>, ReadError> {
    let mut line = Vec::new();
    if reader.read_until(b'\n', &mut line).await? == 0 {
        return Err(ReadError::Io);
    }
    if line.len() > MAX_HEAD_BYTES {
        return Err(ReadError::Status(StatusCode::BadRequest));
    }
    Ok(line)
}

fn trim_crlf(line: &[u8]) -> &[u8] {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    line.strip_suffix(b"\r").unwrap_or(line)
}

async fn read_chunked<R: AsyncBufRead + Unpin>(reader: &mut R) -> Result<Vec<u8>, ReadError> {
    let mut body = Vec::new();
    loop {
        let line = read_line(reader).await?;
        let size_field = std::str::from_utf8(trim_crlf(&line)).map_err(bad_request)?;
        let size_str = size_field.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size_str, 16).map_err(bad_request)?;
        if size == 0 {
            loop {
                let trailer = read_line(reader).await?;
                if trim_crlf(&trailer).is_empty() {
                    return Ok(body);
                }
            }
        }
        if body.len() + size > MAX_BODY_BYTES {
            return Err(ReadError::Status(StatusCode::PayloadTooLarge));
        }
        let start = body.len();
        body.resize(start + size, 0);
        reader.read_exact(&mut body[start..]).await?;
        let crlf = read_line(reader).await?;
        if !trim_crlf(&crlf).is_empty() {
            return Err(ReadError::Status(StatusCode::BadRequest));
        }
    }
}

async fn write_response<W: AsyncWrite + Unpin>(
    w: &mut W,
    mut res: Response,
    keep_alive: bool,
) -> io::Result<()> {
    let status = res.status();
    let mut head = format!(
        "HTTP/1.1 {} {}\r\n",
        u16::from(status),
        status.canonical_reason()
    );
    for (name, values) in res.iter() {
        for value in values.iter() {
            head.push_str(&format!("{name}: {value}\r\n"));
        }
    }
    let len = res.len();
    match len {
        Some(n) => head.push_str(&format!("content-length: {n}\r\n")),
        None => head.push_str("transfer-encoding: chunked\r\n"),
    }
    if !keep_alive {
        head.push_str("connection: close\r\n");
    }
    head.push_str("\r\n");
    w.write_all(head.as_bytes()).await?;
    let mut body = res.take_body();
    if len.is_some() {
        futures_lite::io::copy(&mut body, &mut *w).await?;
    } else {
        let mut buf = vec![0u8; 8 * 1024];
        loop {
            let n = body.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            w.write_all(format!("{n:x}\r\n").as_bytes()).await?;
            w.write_all(&buf[..n]).await?;
            w.write_all(b"\r\n").await?;
        }
        w.write_all(b"0\r\n\r\n").await?;
    }
    w.flush().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_lite::future::block_on;
    use futures_lite::io::Cursor;
    async fn parse(raw: &[u8]) -> Result<Option<(Request, bool)>, StatusCode> {
        let mut reader = BufReader::new(Cursor::new(raw.to_vec()));
        let mut sink = Vec::new();
        match read_request(&mut reader, &mut sink).await {
            Ok(v) => Ok(v),
            Err(ReadError::Status(s)) => Err(s),
            Err(ReadError::Io) => panic!("unexpected io error"),
        }
    }
    #[test]
    fn content_length_body_is_read() {
        block_on(async {
            let (mut req, keep) = parse(
                b"POST /r/git-upload-pack HTTP/1.1\r\nHost: x\r\nContent-Length: 5\r\n\r\nhello",
            )
            .await
            .unwrap()
            .unwrap();
            assert!(keep);
            assert_eq!(req.url().path(), "/r/git-upload-pack");
            assert_eq!(req.body_bytes().await.unwrap(), b"hello");
        });
    }
    #[test]
    fn chunked_body_is_decoded() {
        block_on(async {
            let raw = b"POST /r HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nwiki\r\n5;ext=1\r\npedia\r\n0\r\nX-Trailer: y\r\n\r\n";
            let (mut req, _) = parse(raw).await.unwrap().unwrap();
            assert_eq!(req.body_bytes().await.unwrap(), b"wikipedia");
        });
    }
    #[test]
    fn smuggling_and_malformed_requests_are_rejected() {
        block_on(async {
            assert_eq!(
                parse(
                    b"POST / HTTP/1.1\r\nContent-Length: 1\r\nTransfer-Encoding: chunked\r\n\r\n"
                )
                .await
                .unwrap_err(),
                StatusCode::BadRequest
            );
            assert_eq!(
                parse(b"GET http://evil/ HTTP/1.1\r\n\r\n")
                    .await
                    .unwrap_err(),
                StatusCode::BadRequest
            );
            assert!(parse(b"").await.unwrap().is_none());
        });
    }
}
