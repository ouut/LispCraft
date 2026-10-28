use std::fs::File;
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::thread;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

/// 探测定位静态文件目录 (web)
pub fn find_web_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("LISPCRAFT_WEB_DIR") {
        let p = PathBuf::from(dir);
        if p.exists() && p.is_dir() {
            return p.canonicalize().unwrap_or(p);
        }
    }

    let candidates = [
        PathBuf::from("web"),
        PathBuf::from("../web"),
        PathBuf::from("/workspace/LispCraft/web"),
    ];

    for candidate in &candidates {
        if candidate.exists() && candidate.is_dir() {
            return candidate.canonicalize().unwrap_or_else(|_| candidate.clone());
        }
    }

    PathBuf::from("web")
}

/// 根据扩展名识别 MIME 类型，严格确保 WASM 和 JS 返回标准 Content-Type
pub fn mime_type_for_path(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()).unwrap_or("") {
        "wasm" => "application/wasm",
        "js" => "application/javascript",
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// 处理单个 HTTP 请求
pub fn handle_http_request(request: Request, web_root: &Path) {
    let method = request.method().clone();
    let url_raw = request.url().to_string();
    let clean_path = url_raw.split('?').next().unwrap_or("/");
    let trimmed = clean_path.trim_start_matches('/');

    let rel_path = if trimmed.is_empty() {
        "index.html"
    } else {
        trimmed
    };

    // 安全检查：防止路径穿越 (Path Traversal)
    let path_obj = Path::new(rel_path);
    let is_safe = path_obj.components().all(|c| matches!(c, Component::Normal(_)));
    if !is_safe {
        eprintln!("[HTTP] Blocked path traversal attempt: {}", url_raw);
        let resp = Response::from_string("403 Forbidden").with_status_code(StatusCode(403));
        let _ = request.respond(resp);
        return;
    }

    let target_file = web_root.join(path_obj);

    let cors_header = Header::from_str("Access-Control-Allow-Origin: *").unwrap();
    let cache_control = Header::from_str("Cache-Control: no-cache").unwrap();

    // 处理 OPTIONS 预检请求
    if method == Method::Options {
        let allow_methods = Header::from_str("Access-Control-Allow-Methods: GET, HEAD, OPTIONS").unwrap();
        let allow_headers = Header::from_str("Access-Control-Allow-Headers: *").unwrap();
        let resp = Response::empty(StatusCode(204))
            .with_header(cors_header)
            .with_header(allow_methods)
            .with_header(allow_headers);
        let _ = request.respond(resp);
        return;
    }

    // 检查文件是否存在
    if !target_file.exists() || !target_file.is_file() {
        println!("[HTTP] {} {} -> 404 Not Found", method, url_raw);
        let resp = Response::from_string("404 Not Found")
            .with_status_code(StatusCode(404))
            .with_header(cors_header);
        let _ = request.respond(resp);
        return;
    }

    let mime = mime_type_for_path(&target_file);
    let content_type = Header::from_str(&format!("Content-Type: {}", mime)).unwrap();

    match method {
        Method::Head => {
            let file_size = File::open(&target_file)
                .and_then(|f| f.metadata())
                .map(|m| m.len() as usize)
                .unwrap_or(0);

            println!("[HTTP] HEAD {} -> 200 OK ({} bytes, {})", url_raw, file_size, mime);
            let resp = Response::new(
                StatusCode(200),
                vec![content_type, cors_header, cache_control],
                std::io::empty(),
                Some(file_size),
                None,
            ).with_chunked_threshold(100 * 1024 * 1024);
            let _ = request.respond(resp);
        }
        Method::Get => {
            match File::open(&target_file) {
                Ok(file) => {
                    let file_size = file.metadata().map(|m| m.len()).unwrap_or(0);
                    println!("[HTTP] GET {} -> 200 OK ({} bytes, {})", url_raw, file_size, mime);

                    let resp = Response::from_file(file)
                        .with_chunked_threshold(100 * 1024 * 1024)
                        .with_header(content_type)
                        .with_header(cors_header)
                        .with_header(cache_control);
                    let _ = request.respond(resp);
                }
                Err(e) => {
                    eprintln!("[HTTP] Error opening file '{}': {:?}", target_file.display(), e);
                    let resp = Response::from_string("500 Internal Server Error")
                        .with_status_code(StatusCode(500))
                        .with_header(cors_header);
                    let _ = request.respond(resp);
                }
            }
        }
        _ => {
            let resp = Response::from_string("405 Method Not Allowed")
                .with_status_code(StatusCode(405))
                .with_header(cors_header);
            let _ = request.respond(resp);
        }
    }
}

/// 启动内置多线程 HTTP 静态服务器
pub fn start_http_server(
    bind_addr: &str,
    web_root: PathBuf,
) -> Result<thread::JoinHandle<()>, Box<dyn std::error::Error + Send + Sync>> {
    let server = Arc::new(Server::http(bind_addr)?);
    println!("[HTTP Server] Static Web Server serving '{}' on http://{}", web_root.display(), bind_addr);

    let handle = thread::Builder::new()
        .name("http-server".to_string())
        .spawn(move || {
            let root = Arc::new(web_root);
            for request in server.incoming_requests() {
                let root_clone = root.clone();
                thread::spawn(move || {
                    handle_http_request(request, &root_clone);
                });
            }
        })?;

    Ok(handle)
}
