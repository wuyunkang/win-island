use crate::{ActivityRequest, Kind, Shared};
use serde_json::{json, Value};
use std::{
    io::{self, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const MAX_HEADER: usize = 8192;
const MAX_BODY: usize = 32768;

pub struct Server {
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}
impl Server {
    pub fn bind(
        address: SocketAddr,
        token: String,
        store: Shared,
        repaint: Arc<dyn Fn() + Send + Sync>,
    ) -> io::Result<Self> {
        if !address.ip().is_loopback()
            || token.len() < 32
            || !token.bytes().all(|c| c.is_ascii_hexdigit())
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "接口只允许本机地址，令牌至少包含 32 位十六进制字符",
            ));
        }
        let listener = TcpListener::bind(address)?;
        let address = listener.local_addr()?;
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let worker = thread::Builder::new()
            .name("island-activity-api".into())
            .spawn(move || {
                for incoming in listener.incoming() {
                    if stopped.load(Ordering::Acquire) {
                        break;
                    }
                    let Ok(mut stream) = incoming else {
                        continue;
                    };
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                    let (code, value) = match request(&mut stream, &token) {
                        Ok(request) => route(request, &store, &repaint),
                        Err((code, message)) => (code, json!({"error": message})),
                    };
                    let _ = response(&mut stream, code, value);
                    if code >= 400 {
                        // 先发送 FIN，再有限度地排空客户端已发送的正文，避免 Windows 用 RST 吃掉错误响应。
                        let _ = stream.shutdown(std::net::Shutdown::Write);
                        let _ = stream.set_read_timeout(Some(Duration::from_millis(100)));
                        let _ = io::copy(&mut (&mut stream).take(65536), &mut io::sink());
                    }
                }
            })?;
        Ok(Self {
            address,
            stop,
            thread: Some(worker),
        })
    }
    pub fn address(&self) -> SocketAddr {
        self.address
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = TcpStream::connect_timeout(&self.address, Duration::from_millis(200));
        if let Some(worker) = self.thread.take() {
            let _ = worker.join();
        }
    }
}

struct Request {
    method: String,
    path: String,
    body: Vec<u8>,
}
type Failure = (u16, &'static str);

fn request(stream: &mut TcpStream, token: &str) -> Result<Request, Failure> {
    let mut bytes = Vec::new();
    let header_end = loop {
        if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            if end > MAX_HEADER {
                return Err((431, "请求头过大"));
            }
            break end + 4;
        }
        if bytes.len() > MAX_HEADER {
            return Err((431, "请求头过大"));
        }
        let mut chunk = [0; 1024];
        let n = stream.read(&mut chunk).map_err(|_| (408, "读取请求超时"))?;
        if n == 0 {
            return Err((400, "请求头不完整"));
        }
        bytes.extend_from_slice(&chunk[..n]);
    };
    let headers = std::str::from_utf8(&bytes[..header_end]).map_err(|_| (400, "请求头编码无效"))?;
    let mut lines = headers.split("\r\n");
    let first: Vec<_> = lines
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .collect();
    if first.len() != 3 || first[2] != "HTTP/1.1" || first[1].len() > 160 {
        return Err((400, "HTTP 请求无效"));
    }
    let method = first[0].to_string();
    let path = first[1].to_string();
    let (mut authorization, mut length, mut content_type) = (None, None, None);
    for line in lines.filter(|line| !line.is_empty()) {
        let Some((key, value)) = line.split_once(':') else {
            return Err((400, "请求头格式无效"));
        };
        let value = value.trim();
        match key.to_ascii_lowercase().as_str() {
            "origin" => return Err((403, "不接受网页发起的跨来源请求")),
            "transfer-encoding" => return Err((400, "不支持分块请求")),
            "authorization" => {
                if authorization.replace(value).is_some() {
                    return Err((400, "重复的认证请求头"));
                }
            }
            "content-length" => {
                if length.is_some()
                    || value.is_empty()
                    || !value.bytes().all(|c| c.is_ascii_digit())
                {
                    return Err((400, "Content-Length 无效或重复"));
                }
                length = Some(value.parse::<usize>().map_err(|_| (413, "请求体过大"))?);
            }
            "content-type" => content_type = Some(value),
            _ => {}
        }
    }
    let expected = format!("Bearer {token}");
    if authorization != Some(expected.as_str()) {
        return Err((401, "需要有效的本机访问令牌"));
    }
    let length = length.unwrap_or(0);
    if length > MAX_BODY {
        return Err((413, "请求体不能超过 32 KiB"));
    }
    if method == "POST"
        && !content_type.is_some_and(|v| {
            v.split(';')
                .next()
                .unwrap_or("")
                .trim()
                .eq_ignore_ascii_case("application/json")
        })
    {
        return Err((415, "需要 application/json"));
    }
    let mut body = bytes[header_end..].to_vec();
    while body.len() < length {
        let mut chunk = [0; 4096];
        let remaining = (length - body.len()).min(chunk.len());
        let n = stream
            .read(&mut chunk[..remaining])
            .map_err(|_| (408, "读取请求体超时"))?;
        if n == 0 {
            return Err((400, "请求体不完整"));
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(length);
    Ok(Request { method, path, body })
}

fn route(request: Request, shared: &Shared, repaint: &Arc<dyn Fn() + Send + Sync>) -> (u16, Value) {
    let mut store = match shared.lock() {
        Ok(store) => store,
        Err(_) => return (503, json!({"error":"活动状态暂不可用"})),
    };
    let result = match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/status") => (200, json!({"activities":store.list(Instant::now())})),
        ("POST", "/notify" | "/activity") => {
            let payload: ActivityRequest = match serde_json::from_slice(&request.body) {
                Ok(payload) => payload,
                Err(_) => return (400, json!({"error":"JSON 内容无效"})),
            };
            let kind = if request.path == "/notify" {
                Kind::Notification
            } else {
                Kind::Activity
            };
            match store.upsert(payload, kind, Instant::now()) {
                Ok(id) => (200, json!({"id":id})),
                Err(error) => (400, json!({"error":error})),
            }
        }
        ("DELETE", path) if path.starts_with("/activity/") => {
            let id = &path[10..];
            if !crate::valid_id(id) {
                (400, json!({"error":"活动 id 无效"}))
            } else if store.remove(id) {
                (200, json!({"removed":true}))
            } else {
                (404, json!({"error":"活动不存在"}))
            }
        }
        _ => (404, json!({"error":"接口不存在"})),
    };
    drop(store);
    if result.0 == 200 && request.method != "GET" {
        repaint();
    }
    result
}

fn response(stream: &mut TcpStream, code: u16, value: Value) -> io::Result<()> {
    let body = value.to_string();
    write!(stream, "HTTP/1.1 {code} Response\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\n\r\n{body}", body.len())?;
    stream.flush()
}
