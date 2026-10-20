//! End-to-end smoke test for the LibreTranslate-compatible server: starts it
//! on an ephemeral port over a real catalog and drives it with raw HTTP.
//!
//! Skipped (passes with no work) unless `TRANSLATOR_CATALOG_JSON` (path to an
//! index JSON) and `TRANSLATOR_BASE_DIR` (install dir with models) are set.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;

use serde_json::Value;
use translator::font_provider::NoFontProvider;
use translator::http::{BindAddress, HttpServerConfig, OcrSettings, start};
use translator::{BackgroundMode, FsPackInstallChecker, TranslatorSession};

struct Reply {
    status: u16,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or_else(|error| {
            panic!(
                "body is not JSON ({error}): {}",
                String::from_utf8_lossy(&self.body)
            )
        })
    }
}

fn request(addr: SocketAddr, method: &str, path: &str, content_type: &str, body: &[u8]) -> Reply {
    let mut stream = TcpStream::connect(addr).expect("connect");
    let mut head = format!("{method} {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n");
    if !content_type.is_empty() {
        head.push_str(&format!("Content-Type: {content_type}\r\n"));
    }
    head.push_str(&format!("Content-Length: {}\r\n\r\n", body.len()));
    stream.write_all(head.as_bytes()).expect("write head");
    stream.write_all(body).expect("write body");

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).expect("read reply");
    let split = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("header terminator");
    let head = String::from_utf8_lossy(&raw[..split]).into_owned();
    let mut lines = head.lines();
    let status_line = lines.next().expect("status line");
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .expect("status code");
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.to_ascii_lowercase(), value.trim().to_owned()))
        .collect();
    Reply {
        status,
        headers,
        body: raw[split + 4..].to_vec(),
    }
}

fn multipart(fields: &[(&str, Option<&str>, &[u8])]) -> (String, Vec<u8>) {
    let boundary = "translator-smoke-boundary";
    let mut body = Vec::new();
    for (name, filename, data) in fields {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        match filename {
            Some(filename) => body.extend_from_slice(
                format!(
                    "Content-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
                )
                .as_bytes(),
            ),
            None => body.extend_from_slice(
                format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
            ),
        }
        body.extend_from_slice(data);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={boundary}"), body)
}

#[test]
fn libretranslate_endpoints() {
    let (Some(catalog_path), Some(base_dir)) = (
        std::env::var_os("TRANSLATOR_CATALOG_JSON"),
        std::env::var("TRANSLATOR_BASE_DIR").ok(),
    ) else {
        eprintln!("TRANSLATOR_CATALOG_JSON / TRANSLATOR_BASE_DIR unset; skipping");
        return;
    };
    let catalog_json = std::fs::read_to_string(catalog_path).expect("read catalog");
    let checker = FsPackInstallChecker::new(&base_dir);
    let session = Arc::new(
        TranslatorSession::open(&catalog_json, None, base_dir, &checker).expect("open session"),
    );
    let output_dir =
        std::env::temp_dir().join(format!("translator-http-smoke-{}", std::process::id()));

    let server = start(
        HttpServerConfig {
            bind: BindAddress::Localhost,
            port: 0,
            output_dir: output_dir.to_string_lossy().into_owned(),
            ocr: OcrSettings {
                max_image_size: 1000,
                min_confidence: 75,
                background_mode: BackgroundMode::AutoDetect,
            },
            translate_pdf_images: false,
        },
        Arc::new(move || Some(session.clone())),
        Arc::new(NoFontProvider),
    )
    .expect("start server");
    let addr = server.local_addr();

    let page = request(addr, "GET", "/", "", b"");
    assert_eq!(page.status, 200);
    assert!(page.body.starts_with(b"<!doctype html>"));
    assert_eq!(
        page.headers.get("cache-control").map(String::as_str),
        Some("no-store")
    );

    let preflight = request(addr, "OPTIONS", "/translate", "", b"");
    assert_eq!(preflight.status, 200);
    assert_eq!(
        preflight
            .headers
            .get("access-control-allow-origin")
            .map(String::as_str),
        Some("*")
    );

    let missing = request(addr, "GET", "/nope", "", b"");
    assert_eq!(missing.status, 404);
    assert_eq!(missing.json()["error"], "Not found");

    let languages = request(addr, "GET", "/languages", "", b"").json();
    let languages = languages.as_array().expect("languages array");
    let Some(source) = languages.iter().find(|language| {
        language["code"] != "en"
            && language["targets"]
                .as_array()
                .is_some_and(|targets| targets.iter().any(|target| target == "en"))
    }) else {
        eprintln!("no language pair into English installed; skipping translation checks");
        server.stop();
        return;
    };
    let source_code = source["code"].as_str().expect("code").to_owned();

    let no_target = request(
        addr,
        "POST",
        "/translate",
        "application/json",
        br#"{"q":"hola"}"#,
    );
    assert_eq!(no_target.status, 400);
    assert_eq!(no_target.json()["error"], "'target' is required");

    let body = serde_json::json!({"q": "Hola mundo", "source": source_code, "target": "en"});
    let translated = request(
        addr,
        "POST",
        "/translate",
        "application/json",
        body.to_string().as_bytes(),
    );
    assert_eq!(
        translated.status,
        200,
        "{}",
        String::from_utf8_lossy(&translated.body)
    );
    let translated = translated.json();
    assert!(
        translated["translatedText"]
            .as_str()
            .is_some_and(|text| !text.is_empty())
    );
    assert!(translated.get("detectedLanguage").is_none());

    let batch = serde_json::json!({"q": ["Hola", "mundo"], "source": source_code, "target": "en"});
    let batch = request(
        addr,
        "POST",
        "/translate",
        "application/json",
        batch.to_string().as_bytes(),
    )
    .json();
    assert_eq!(batch["translatedText"].as_array().map(Vec::len), Some(2));

    let form = format!("q=Hola+mundo&source={source_code}&target=en");
    let form = request(
        addr,
        "POST",
        "/translate",
        "application/x-www-form-urlencoded",
        form.as_bytes(),
    );
    assert_eq!(form.status, 200, "{}", String::from_utf8_lossy(&form.body));
    assert!(form.json()["translatedText"].is_string());

    let detected = request(
        addr,
        "POST",
        "/detect",
        "application/json",
        r#"{"q":"Hola mundo, buenos días a todos"}"#.as_bytes(),
    );
    assert_eq!(detected.status, 200);
    assert!(detected.json().is_array());

    let (content_type, upload) = multipart(&[
        ("file", Some("note.txt"), b"Hola mundo.\nBuenos dias."),
        ("source", None, source_code.as_bytes()),
        ("target", None, b"en"),
    ]);
    let file = request(addr, "POST", "/translate_file", &content_type, &upload);
    assert_eq!(file.status, 200, "{}", String::from_utf8_lossy(&file.body));
    let url = file.json()["translatedFileUrl"]
        .as_str()
        .expect("url")
        .to_owned();
    let path = url
        .split_once(&format!("http://{addr}"))
        .expect("url on this host")
        .1
        .to_owned();
    let download = request(addr, "GET", &path, "", b"");
    assert_eq!(download.status, 200);
    assert_eq!(
        download.headers.get("content-type").map(String::as_str),
        Some("text/plain")
    );
    assert!(
        download
            .headers
            .get("content-disposition")
            .is_some_and(|value| value.contains("note-en.txt"))
    );
    assert!(!download.body.is_empty());

    let expired = request(addr, "GET", "/download/does-not-exist", "", b"");
    assert_eq!(expired.status, 404);

    server.stop();
    let _ = std::fs::remove_dir_all(output_dir);
}
