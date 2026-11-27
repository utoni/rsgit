use crate::config::config as Config;
use futures_lite::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader,
};
use http::header::{
    AsHeaderName, CONNECTION, CONTENT_LENGTH, CONTENT_TYPE, EXPECT, HeaderMap, HeaderName,
    HeaderValue, IntoHeaderName, TRANSFER_ENCODING,
};
pub use http::{Method, StatusCode};
use std::future::Future;
use std::io;
use std::time::Duration;
pub use url::Url;

const MAX_HEADERS: usize = 128;
const CONTINUE_RESPONSE: &[u8] = b"HTTP/1.1 100 Continue\r\n\r\n";
const KNOWN_METHODS: &[&str] = &["GET", "POST"];

pub mod mime {
    pub const HTML: &str = "text/html;charset=utf-8";
    pub const PLAIN: &str = "text/plain;charset=utf-8";
    pub const BYTE_STREAM: &str = "application/octet-stream";

    pub fn from_extension(extension: impl AsRef<str>) -> Option<&'static str> {
        match extension.as_ref() {
            "html" => Some(HTML),
            "js" | "mjs" | "jsonp" => Some("application/javascript;charset=utf-8"),
            "json" => Some("application/json"),
            "css" => Some("text/css;charset=utf-8"),
            "svg" => Some("image/svg+xml"),
            "xml" => Some("application/xml;charset=utf-8"),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct Request {
    method: Method,
    url: Url,
    headers: HeaderMap,
    body: Vec<u8>,
}

impl Request {
    pub fn method(&self) -> &Method {
        &self.method
    }

    pub fn url(&self) -> &Url {
        &self.url
    }

    pub fn header(&self, name: impl AsHeaderName) -> Option<&str> {
        self.headers.get(name).map(value_str)
    }

    pub fn take_body(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.body)
    }
}

fn value_str(v: &HeaderValue) -> &str {
    std::str::from_utf8(v.as_bytes()).unwrap_or("")
}

enum BodyInner {
    Bytes(Vec<u8>),
    Reader(Box<dyn AsyncRead + Unpin + Send + 'static>),
}

pub struct Body {
    inner: BodyInner,
    mime: &'static str,
}

impl Body {
    pub fn empty() -> Self {
        Body {
            inner: BodyInner::Bytes(Vec::new()),
            mime: mime::BYTE_STREAM,
        }
    }

    pub fn from_reader(reader: impl AsyncRead + Unpin + Send + 'static) -> Self {
        Body {
            inner: BodyInner::Reader(Box::new(reader)),
            mime: mime::BYTE_STREAM,
        }
    }

    pub fn len(&self) -> Option<usize> {
        match &self.inner {
            BodyInner::Bytes(b) => Some(b.len()),
            BodyInner::Reader(_) => None,
        }
    }
}

impl From<String> for Body {
    fn from(s: String) -> Self {
        Body {
            inner: BodyInner::Bytes(s.into_bytes()),
            mime: mime::PLAIN,
        }
    }
}

impl From<Vec<u8>> for Body {
    fn from(b: Vec<u8>) -> Self {
        Body {
            inner: BodyInner::Bytes(b),
            mime: mime::BYTE_STREAM,
        }
    }
}

pub struct Response {
    status: StatusCode,
    headers: HeaderMap,
    body: Body,
}

impl Response {
    pub fn new(status: StatusCode) -> Self {
        Response {
            status,
            headers: HeaderMap::new(),
            body: Body::empty(),
        }
    }

    pub fn insert_header(&mut self, name: impl IntoHeaderName, value: impl AsRef<str>) {
        let value = value.as_ref();
        assert!(value.is_ascii(), "String slice should be valid ASCII");
        let value = HeaderValue::from_str(value).expect("header value contains control characters");
        self.headers.insert(name, value);
    }

    pub fn set_content_type(&mut self, mime: &'static str) {
        self.insert_header(CONTENT_TYPE, mime);
    }

    pub fn set_body(&mut self, body: impl Into<Body>) {
        self.body = body.into();
        if !self.headers.contains_key(CONTENT_TYPE) {
            self.set_content_type(self.body.mime);
        }
    }
}

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
    ReadError::Status(StatusCode::BAD_REQUEST)
}

fn parse_method(m: &str) -> Option<Method> {
    let upper = m.to_ascii_uppercase();
    if KNOWN_METHODS.contains(&upper.as_str()) {
        Method::from_bytes(upper.as_bytes()).ok()
    } else {
        None
    }
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
        let (req, keep_alive) = match with_timeout(
            Duration::from_secs(Config().head_timeout),
            read_request(&mut reader, &mut writer),
        )
        .await
        {
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
                Err(ReadError::Status(StatusCode::BAD_REQUEST))
            };
        }
        let blank = line == b"\r\n" || line == b"\n";
        if head.is_empty() && blank {
            continue;
        }
        head.extend_from_slice(&line);
        if head.len() > Config().max_head_bytes {
            return Err(ReadError::Status(
                StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE,
            ));
        }
        if blank {
            break;
        }
    }
    let mut raw_headers = [httparse::EMPTY_HEADER; MAX_HEADERS];
    let mut parsed = httparse::Request::new(&mut raw_headers);
    match parsed.parse(&head) {
        Ok(httparse::Status::Complete(_)) => {}
        _ => return Err(ReadError::Status(StatusCode::BAD_REQUEST)),
    }
    let method = parsed
        .method
        .and_then(parse_method)
        .ok_or(ReadError::Status(StatusCode::BAD_REQUEST))?;
    let http11 = match parsed.version {
        Some(1) => true,
        Some(0) => false,
        _ => return Err(ReadError::Status(StatusCode::BAD_REQUEST)),
    };
    let target = parsed
        .path
        .filter(|p| p.starts_with('/'))
        .ok_or(ReadError::Status(StatusCode::BAD_REQUEST))?;
    let url = Url::parse(&format!("http://localhost{target}")).map_err(bad_request)?;
    let mut headers = HeaderMap::new();
    for h in parsed.headers.iter() {
        let value = std::str::from_utf8(h.value).map_err(bad_request)?;
        if !value.is_ascii() {
            return Err(ReadError::Io);
        }
        let name = HeaderName::from_bytes(h.name.as_bytes()).map_err(bad_request)?;
        let value = HeaderValue::from_str(value).map_err(bad_request)?;
        headers.append(name, value);
    }
    let chunked = match headers.get(TRANSFER_ENCODING).map(value_str) {
        Some(te) => {
            if !te.trim().eq_ignore_ascii_case("chunked") {
                return Err(ReadError::Status(StatusCode::BAD_REQUEST));
            }
            true
        }
        None => false,
    };
    let content_length = headers
        .get(CONTENT_LENGTH)
        .map(|v| value_str(v).trim().parse::<usize>())
        .transpose()
        .map_err(bad_request)?;
    if chunked && content_length.is_some() {
        return Err(ReadError::Status(StatusCode::BAD_REQUEST));
    }
    let connection = headers
        .get(CONNECTION)
        .map(|v| value_str(v).to_ascii_lowercase())
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
        if content_length.is_some_and(|len| len > Config().max_body_bytes) {
            return Err(ReadError::Status(StatusCode::PAYLOAD_TOO_LARGE));
        }
        if headers
            .get(EXPECT)
            .is_some_and(|v| value_str(v).eq_ignore_ascii_case("100-continue"))
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
    let req = Request {
        method,
        url,
        headers,
        body,
    };
    Ok(Some((req, keep_alive)))
}

async fn read_line<R: AsyncBufRead + Unpin>(reader: &mut R) -> Result<Vec<u8>, ReadError> {
    let mut line = Vec::new();
    if reader.read_until(b'\n', &mut line).await? == 0 {
        return Err(ReadError::Io);
    }
    if line.len() > Config().max_head_bytes {
        return Err(ReadError::Status(StatusCode::BAD_REQUEST));
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
        if body.len() + size > Config().max_body_bytes {
            return Err(ReadError::Status(StatusCode::PAYLOAD_TOO_LARGE));
        }
        let start = body.len();
        body.resize(start + size, 0);
        reader.read_exact(&mut body[start..]).await?;
        let crlf = read_line(reader).await?;
        if !trim_crlf(&crlf).is_empty() {
            return Err(ReadError::Status(StatusCode::BAD_REQUEST));
        }
    }
}

async fn write_response<W: AsyncWrite + Unpin>(
    w: &mut W,
    res: Response,
    keep_alive: bool,
) -> io::Result<()> {
    let status = res.status;
    let mut head = format!(
        "HTTP/1.1 {} {}\r\n",
        status.as_u16(),
        status.canonical_reason().unwrap_or("")
    )
    .into_bytes();
    for (name, value) in res.headers.iter() {
        head.extend_from_slice(name.as_str().as_bytes());
        head.extend_from_slice(b": ");
        head.extend_from_slice(value.as_bytes());
        head.extend_from_slice(b"\r\n");
    }
    match res.body.len() {
        Some(n) => head.extend_from_slice(format!("content-length: {n}\r\n").as_bytes()),
        None => head.extend_from_slice(b"transfer-encoding: chunked\r\n"),
    }
    if !keep_alive {
        head.extend_from_slice(b"connection: close\r\n");
    }
    head.extend_from_slice(b"\r\n");
    w.write_all(&head).await?;
    match res.body.inner {
        BodyInner::Bytes(bytes) => w.write_all(&bytes).await?,
        BodyInner::Reader(mut body) => {
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
    }
    w.flush().await
}

#[cfg(test)]
mod tests {
    pub fn body(req: &Request) -> &[u8] {
        &req.body
    }

    use super::*;
    use crate::config::init_config as InitConfig;
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
    async fn serialize(res: Response, keep_alive: bool) -> String {
        let mut out = Vec::new();
        write_response(&mut out, res, keep_alive).await.unwrap();
        String::from_utf8(out).unwrap()
    }
    #[test]
    fn content_length_body_is_read() {
        block_on(async {
            let (req, keep) = parse(
                b"POST /r/git-upload-pack HTTP/1.1\r\nHost: x\r\nContent-Length: 5\r\n\r\nhello",
            )
            .await
            .unwrap()
            .unwrap();
            assert!(keep);
            assert_eq!(req.url().path(), "/r/git-upload-pack");
            assert_eq!(body(&req), b"hello");
        });
    }
    #[test]
    fn chunked_body_is_decoded() {
        InitConfig();
        block_on(async {
            let raw = b"POST /r HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nwiki\r\n5;ext=1\r\npedia\r\n0\r\nX-Trailer: y\r\n\r\n";
            let (req, _) = parse(raw).await.unwrap().unwrap();
            assert_eq!(body(&req), b"wikipedia");
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
                StatusCode::BAD_REQUEST
            );
            assert_eq!(
                parse(b"GET http://evil/ HTTP/1.1\r\n\r\n")
                    .await
                    .unwrap_err(),
                StatusCode::BAD_REQUEST
            );
            assert!(parse(b"").await.unwrap().is_none());
        });
    }
    #[test]
    fn methods_test() {
        block_on(async {
            let (req, _) = parse(b"get / HTTP/1.1\r\n\r\n").await.unwrap().unwrap();
            assert_eq!(req.method(), Method::GET);
            let (req, _) = parse(b"pOsT / HTTP/1.1\r\n\r\n").await.unwrap().unwrap();
            assert_eq!(req.method().as_str(), "POST");
            assert_eq!(
                parse(b"FOO / HTTP/1.1\r\n\r\n").await.unwrap_err(),
                StatusCode::BAD_REQUEST
            );
        });
    }
    #[test]
    fn non_ascii_header_value_drops_connection() {
        block_on(async {
            let raw = "GET / HTTP/1.1\r\nX-Name: \u{e4}\r\n\r\n"
                .as_bytes()
                .to_vec();
            let mut reader = BufReader::new(Cursor::new(raw));
            let mut sink = Vec::new();
            assert!(matches!(
                read_request(&mut reader, &mut sink).await,
                Err(ReadError::Io)
            ));
        });
    }
    #[test]
    fn response_format() {
        block_on(async {
            let mut res = Response::new(StatusCode::NOT_FOUND);
            res.set_body("nope".to_string());
            assert_eq!(
                serialize(res, false).await,
                "HTTP/1.1 404 Not Found\r\ncontent-type: text/plain;charset=utf-8\r\ncontent-length: 4\r\nconnection: close\r\n\r\nnope"
            );
            let res = Response::new(StatusCode::PAYLOAD_TOO_LARGE);
            assert_eq!(
                serialize(res, true).await,
                "HTTP/1.1 413 Payload Too Large\r\ncontent-length: 0\r\n\r\n"
            );
            let mut res = Response::new(StatusCode::OK);
            res.set_body(Body::from_reader(Cursor::new(b"abc".to_vec())));
            assert_eq!(
                serialize(res, true).await,
                "HTTP/1.1 200 OK\r\ncontent-type: application/octet-stream\r\ntransfer-encoding: chunked\r\n\r\n3\r\nabc\r\n0\r\n\r\n"
            );
        });
    }
}
