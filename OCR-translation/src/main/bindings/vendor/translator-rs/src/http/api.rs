use std::io::Read;
use std::sync::Arc;

use rouille::input::json_input;
use rouille::input::multipart::get_multipart_input;
use rouille::input::post::raw_urlencoded_post_input;
use rouille::{Request, Response};
use serde::{Deserialize, Serialize};

use super::store::FileStore;
use super::{HttpServerConfig, SessionSource};
use crate::api::ScriptedLanguage;
use crate::catalog::language_rows_in_snapshot;
use crate::document::{DocumentFormat, DocumentOptions, translate_document_bytes};
use crate::font_provider::FontProvider;
use crate::language::Language;
use crate::language_detect::detect_language_robust_code;
use crate::txt::TxtLayout;
use crate::{CatalogSnapshot, LanguageCode, TranslatorSession};

const INDEX_HTML: &str = include_str!("index.html");
const MIME_JSON: &str = "application/json";
// The robust detector returns a single best code without a score; report
// full confidence to satisfy the LibreTranslate schema.
const DETECTED_CONFIDENCE: f64 = 100.0;
#[cfg(feature = "ppocr")]
const MIN_OVERLAY_FONT_SIZE_PX: f32 = 8.0;
const AUTO_SOURCE: &str = "auto";

pub struct Api {
    config: HttpServerConfig,
    sessions: Arc<dyn SessionSource>,
    fonts: Arc<dyn FontProvider + Send + Sync>,
    store: FileStore,
}

struct ApiError {
    status: u16,
    message: String,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: 400,
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: 404,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: 500,
            message: message.into(),
        }
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

impl From<ApiError> for Response {
    fn from(error: ApiError) -> Self {
        if error.status >= 500 {
            log::error!("http {}: {}", error.status, error.message);
        }
        Response::json(&ErrorBody {
            error: error.message,
        })
        .with_status_code(error.status)
    }
}

/// LibreTranslate's `q` is either one string or an array of strings, and the
/// response mirrors that shape.
#[derive(Deserialize)]
#[serde(untagged)]
enum Query {
    One(String),
    Many(Vec<String>),
}

impl Query {
    fn texts(&self) -> &[String] {
        match self {
            Query::One(text) => std::slice::from_ref(text),
            Query::Many(texts) => texts,
        }
    }

    fn joined(&self) -> String {
        self.texts().join("\n")
    }

    fn mirror(&self, translated: Vec<String>) -> Translated {
        match self {
            Query::One(_) => Translated::One(translated.into_iter().next().unwrap_or_default()),
            Query::Many(_) => Translated::Many(translated),
        }
    }
}

#[derive(Deserialize)]
struct TranslateBody {
    q: Option<Query>,
    source: Option<String>,
    target: Option<String>,
    format: Option<String>,
}

impl TranslateBody {
    /// Form clients send `q` once per text and every field as a string, and a
    /// field sent empty means "not given".
    fn from_fields(fields: Vec<(String, String)>) -> Self {
        let mut texts = Vec::new();
        let mut source = None;
        let mut target = None;
        let mut format = None;
        for (name, value) in fields {
            match name.as_str() {
                "q" => texts.push(value),
                "source" => source = source.or(Some(value)),
                "target" => target = target.or(Some(value)),
                "format" => format = format.or(Some(value)),
                _ => {}
            }
        }
        let q = match texts.len() {
            0 => None,
            1 => Some(Query::One(texts.remove(0))),
            _ => Some(Query::Many(texts)),
        };
        Self {
            q,
            source,
            target,
            format,
        }
    }

    fn present(value: Option<String>) -> Option<String> {
        value.filter(|value| !value.is_empty())
    }

    fn normalized(self) -> Self {
        Self {
            q: self.q,
            source: Self::present(self.source),
            target: Self::present(self.target),
            format: Self::present(self.format),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OutputFormat {
    Text,
    Html,
}

impl OutputFormat {
    fn parse(value: Option<&str>) -> Result<Self, ApiError> {
        match value {
            None | Some("text") => Ok(OutputFormat::Text),
            Some("html") => Ok(OutputFormat::Html),
            Some(other) => Err(ApiError::bad_request(format!(
                "format must be 'text' or 'html', got '{other}'"
            ))),
        }
    }
}

#[derive(Serialize)]
#[serde(untagged)]
enum Translated {
    One(String),
    Many(Vec<String>),
}

#[derive(Serialize)]
struct Detected {
    confidence: f64,
    language: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TranslateResponse {
    translated_text: Translated,
    #[serde(skip_serializing_if = "Option::is_none")]
    detected_language: Option<Detected>,
}

#[derive(Serialize)]
struct LanguageEntry {
    code: String,
    name: String,
    targets: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FileResponse {
    translated_file_url: String,
}

/// The catalog as the request sees it: one snapshot for the whole request so
/// availability checks and the translation agree.
struct Languages {
    snapshot: Arc<CatalogSnapshot>,
}

impl Languages {
    fn by_code(&self, code: &str) -> Option<Language> {
        self.snapshot
            .catalog
            .language_by_code(&LanguageCode::from(code))
    }

    fn translatable(&self) -> Vec<Language> {
        language_rows_in_snapshot(&self.snapshot)
            .into_iter()
            .filter(|row| row.availability.translator_files())
            .map(|row| row.language)
            .collect()
    }

    fn translatable_scripted(&self) -> Vec<ScriptedLanguage> {
        self.translatable().iter().map(Language::scripted).collect()
    }

    fn can_translate(&self, from: &Language, to: &Language) -> bool {
        self.snapshot.can_translate(
            &LanguageCode::from(from.code.as_str()),
            &LanguageCode::from(to.code.as_str()),
        )
    }

    fn require(&self, role: &str, code: &str) -> Result<Language, ApiError> {
        self.by_code(code)
            .ok_or_else(|| ApiError::bad_request(format!("{role} language '{code}' not available")))
    }

    fn require_pair(&self, from: &Language, to: &Language) -> Result<(), ApiError> {
        if from.code != to.code && !self.can_translate(from, to) {
            return Err(ApiError::bad_request(format!(
                "translation {} -> {} not available",
                from.code, to.code
            )));
        }
        Ok(())
    }

    fn detect(&self, text: &str) -> Option<Language> {
        let available = self.translatable_scripted();
        let code = detect_language_robust_code(text, None, &available)?;
        self.by_code(code.as_str())
    }
}

struct Upload {
    filename: String,
    bytes: Vec<u8>,
}

impl Upload {
    fn extension(&self) -> String {
        self.filename
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_ascii_lowercase())
            .unwrap_or_default()
    }

    fn stem(&self) -> &str {
        self.filename
            .rsplit_once('.')
            .map_or(self.filename.as_str(), |(stem, _)| stem)
    }
}

struct FileForm {
    upload: Upload,
    source: String,
    target: String,
}

impl Api {
    pub fn new(
        config: HttpServerConfig,
        sessions: Arc<dyn SessionSource>,
        fonts: Arc<dyn FontProvider + Send + Sync>,
        store: FileStore,
    ) -> Self {
        Self {
            config,
            sessions,
            fonts,
            store,
        }
    }

    pub fn handle(&self, request: &Request) -> Response {
        if request.method() == "OPTIONS" {
            return cors(Response::from_data(MIME_JSON, "{}"));
        }
        let url = request.url();
        let result = match (request.method(), url.as_str()) {
            ("POST", "/translate") => self.translate(request),
            ("POST", "/translate_file") => self.translate_file(request),
            ("POST", "/detect") => self.detect(request),
            ("GET", "/languages") => self.languages(),
            ("GET", path) if path.starts_with("/download/") => {
                self.download(&path["/download/".len()..])
            }
            ("GET", "/") | ("GET", "/index.html") => Ok(web_ui()),
            _ => Err(ApiError::not_found("Not found")),
        };
        cors(result.unwrap_or_else(Response::from))
    }

    fn session(&self) -> Result<Arc<TranslatorSession>, ApiError> {
        self.sessions
            .session()
            .ok_or_else(|| ApiError::internal("catalog unavailable"))
    }

    fn translate(&self, request: &Request) -> Result<Response, ApiError> {
        let body = read_translate_body(request)?;
        let target = body
            .target
            .as_deref()
            .ok_or_else(|| ApiError::bad_request("'target' is required"))?;
        let source = body.source.as_deref().unwrap_or(AUTO_SOURCE);
        let format = OutputFormat::parse(body.format.as_deref())?;
        let query = body
            .q
            .as_ref()
            .ok_or_else(|| ApiError::bad_request("'q' is required"))?;

        let session = self.session()?;
        let languages = Languages {
            snapshot: session.snapshot(),
        };
        let to = languages.require("target", target)?;
        let (from, detected) = if source == AUTO_SOURCE {
            let detected = languages
                .detect(&query.joined())
                .ok_or_else(|| ApiError::bad_request("could not detect source language"))?;
            (detected.clone(), Some(detected))
        } else {
            (languages.require("source", source)?, None)
        };
        languages.require_pair(&from, &to)?;

        let translated = query
            .texts()
            .iter()
            .map(|text| translate_one(&session, &from, &to, text, format))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Response::json(&TranslateResponse {
            translated_text: query.mirror(translated),
            detected_language: detected.map(|language| Detected {
                confidence: DETECTED_CONFIDENCE,
                language: language.code,
            }),
        }))
    }

    fn detect(&self, request: &Request) -> Result<Response, ApiError> {
        let body = read_translate_body(request)?;
        let query = body
            .q
            .as_ref()
            .ok_or_else(|| ApiError::bad_request("'q' is required"))?;
        let session = self.session()?;
        let languages = Languages {
            snapshot: session.snapshot(),
        };
        let detected: Vec<Detected> = languages
            .detect(&query.joined())
            .map(|language| Detected {
                confidence: DETECTED_CONFIDENCE,
                language: language.code,
            })
            .into_iter()
            .collect();
        Ok(Response::json(&detected))
    }

    fn languages(&self) -> Result<Response, ApiError> {
        let session = self.session()?;
        let languages = Languages {
            snapshot: session.snapshot(),
        };
        let translatable = languages.translatable();
        let entries: Vec<LanguageEntry> = translatable
            .iter()
            .map(|language| LanguageEntry {
                code: language.code.clone(),
                name: language.display_name.clone(),
                targets: translatable
                    .iter()
                    .filter(|other| {
                        other.code != language.code && languages.can_translate(language, other)
                    })
                    .map(|other| other.code.clone())
                    .collect(),
            })
            .collect();
        Ok(Response::json(&entries))
    }

    // Translates synchronously inside the worker, matching LibreTranslate's
    // blocking contract, and answers with a one-shot download URL.
    fn translate_file(&self, request: &Request) -> Result<Response, ApiError> {
        let form = read_file_form(request)?;
        let session = self.session()?;
        let languages = Languages {
            snapshot: session.snapshot(),
        };
        let to = languages.require("target", &form.target)?;
        let extension = form.upload.extension();
        let download_name = format!("{}-{}.", form.upload.stem(), to.code);

        let id = match extension.as_str() {
            "png" | "jpg" | "jpeg" => {
                let png = self.translate_image(&session, &languages, &form, &to)?;
                self.store
                    .put(&png, "png", download_name + "png", "image/png")
            }
            _ => {
                let format = DocumentFormat::from_extension(&extension).ok_or_else(|| {
                    ApiError::bad_request(format!("unsupported file type '.{extension}'"))
                })?;
                let bytes = self.translate_document(&session, &languages, &form, format, &to)?;
                let mime = document_mime(format);
                self.store.put(
                    &bytes,
                    format.extension(),
                    download_name + format.extension(),
                    mime,
                )
            }
        }
        .map_err(|error| ApiError::internal(format!("cannot store translated file: {error}")))?;

        let host = request
            .header("Host")
            .map(str::to_owned)
            .unwrap_or_else(|| format!("127.0.0.1:{}", self.config.port));
        Ok(Response::json(&FileResponse {
            translated_file_url: format!("http://{host}/download/{id}"),
        }))
    }

    #[cfg(feature = "ppocr")]
    fn translate_image(
        &self,
        session: &TranslatorSession,
        languages: &Languages,
        form: &FileForm,
        to: &Language,
    ) -> Result<Vec<u8>, ApiError> {
        use crate::image_render::{RenderOptions, render_overlay};
        use crate::ocr::OcrSourceSelection;

        let source_selection = if form.source == AUTO_SOURCE {
            OcrSourceSelection::auto()
        } else {
            let from = languages.require("source", &form.source)?;
            OcrSourceSelection::specific(from.code.as_str())
        };
        let image = decode_image(&form.upload.bytes)
            .map_err(|error| ApiError::bad_request(format!("could not decode image: {error}")))?;
        let (width, height) = image.dimensions();
        let ocr = &self.config.ocr;
        let prepared = session
            .translate_image_rgba(
                image.as_raw(),
                width,
                height,
                ocr.max_image_size,
                source_selection,
                &to.code,
                ocr.min_confidence,
                None,
                ocr.background_mode,
                None,
            )
            .map_err(|error| {
                if error.is_missing_asset() {
                    ApiError::bad_request(format!(
                        "image translation {} -> {} not available",
                        form.source, to.code
                    ))
                } else {
                    ApiError::bad_request(error.message)
                }
            })?;
        let rendered = render_overlay(
            &prepared,
            &*self.fonts,
            &RenderOptions {
                language: to.code.clone(),
                min_font_size_px: MIN_OVERLAY_FONT_SIZE_PX,
            },
        )
        .map_err(|error| ApiError::internal(format!("could not render overlay: {error}")))?;
        encode_png(prepared.width, prepared.height, rendered.rgba_bytes)
            .map_err(|error| ApiError::internal(format!("could not encode image: {error}")))
    }

    #[cfg(not(feature = "ppocr"))]
    fn translate_image(
        &self,
        _session: &TranslatorSession,
        _languages: &Languages,
        _form: &FileForm,
        _to: &Language,
    ) -> Result<Vec<u8>, ApiError> {
        Err(ApiError::bad_request("image translation is not available"))
    }

    fn translate_document(
        &self,
        session: &TranslatorSession,
        languages: &Languages,
        form: &FileForm,
        format: DocumentFormat,
        to: &Language,
    ) -> Result<Vec<u8>, ApiError> {
        let from = if form.source == AUTO_SOURCE {
            None
        } else {
            Some(languages.require("source", &form.source)?)
        };
        if format == DocumentFormat::Txt && from.is_none() {
            return Err(ApiError::bad_request(
                "'source' is required for text documents",
            ));
        }
        if let Some(from) = &from {
            languages.require_pair(from, to)?;
        }
        let options = DocumentOptions {
            forced_source_code: from.as_ref().map(|language| language.code.as_str()),
            target_code: &to.code,
            translate_pdf_images: self.config.translate_pdf_images,
            txt_layout: TxtLayout::Preserve,
            fonts: &*self.fonts,
        };
        translate_document_bytes(
            session,
            format,
            &form.upload.bytes,
            &options,
            &|_| {},
            &|| false,
        )
        .map_err(|error| ApiError::internal(format!("document translation failed: {error}")))
    }

    fn download(&self, id: &str) -> Result<Response, ApiError> {
        let entry = self
            .store
            .get(id)
            .ok_or_else(|| ApiError::not_found("file not found"))?;
        let file =
            std::fs::File::open(&entry.path).map_err(|_| ApiError::not_found("file expired"))?;
        Ok(Response::from_file(entry.mime, file)
            .with_content_disposition_attachment(&entry.download_name))
    }
}

fn translate_one(
    session: &TranslatorSession,
    from: &Language,
    to: &Language,
    text: &str,
    format: OutputFormat,
) -> Result<String, ApiError> {
    if from.code == to.code {
        return Ok(text.to_owned());
    }
    let result = match format {
        OutputFormat::Text => session.translate_text(&from.code, &to.code, text),
        OutputFormat::Html => session
            .translate_html_fragments(&from.code, &to.code, std::slice::from_ref(&text.to_owned()))
            .map(|fragments| fragments.into_iter().next().unwrap_or_default()),
    };
    result.map_err(|error| ApiError::internal(error.message))
}

// Serving the page from the server itself makes the browser's origin match the
// API's, which sidesteps the mixed-content block an https-hosted copy hits. The
// page ships inside the binary, so an app update changes it under a URL that
// never changes; without no-store a browser keeps serving the old one.
fn web_ui() -> Response {
    Response::html(INDEX_HTML).with_unique_header("Cache-Control", "no-store")
}

fn cors(response: Response) -> Response {
    response
        .with_unique_header("Access-Control-Allow-Origin", "*")
        .with_unique_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        .with_unique_header(
            "Access-Control-Allow-Headers",
            "Content-Type, Authorization",
        )
}

fn content_type(request: &Request) -> &str {
    request.header("Content-Type").unwrap_or("")
}

fn read_translate_body(request: &Request) -> Result<TranslateBody, ApiError> {
    let body = if content_type(request).starts_with(MIME_JSON) {
        json_input::<TranslateBody>(request)
            .map_err(|error| ApiError::bad_request(format!("invalid JSON body: {error}")))?
    } else {
        TranslateBody::from_fields(read_form_fields(request)?)
    };
    Ok(body.normalized())
}

fn read_form_fields(request: &Request) -> Result<Vec<(String, String)>, ApiError> {
    if content_type(request).starts_with("multipart/form-data") {
        let mut multipart = get_multipart_input(request)
            .map_err(|error| ApiError::bad_request(format!("invalid multipart body: {error}")))?;
        let mut fields = Vec::new();
        while let Some(mut field) = multipart.next() {
            let mut value = String::new();
            field
                .data
                .read_to_string(&mut value)
                .map_err(|error| ApiError::bad_request(format!("invalid form field: {error}")))?;
            fields.push((field.headers.name.to_string(), value));
        }
        return Ok(fields);
    }
    raw_urlencoded_post_input(request)
        .map_err(|error| ApiError::bad_request(format!("invalid form body: {error}")))
}

fn read_file_form(request: &Request) -> Result<FileForm, ApiError> {
    let mut multipart = get_multipart_input(request)
        .map_err(|_| ApiError::bad_request("expected a multipart/form-data upload"))?;
    let mut upload = None;
    let mut source = None;
    let mut target = None;
    while let Some(mut field) = multipart.next() {
        let mut bytes = Vec::new();
        field
            .data
            .read_to_end(&mut bytes)
            .map_err(|error| ApiError::bad_request(format!("invalid form field: {error}")))?;
        match &*field.headers.name {
            "file" => {
                upload = Some(Upload {
                    filename: field
                        .headers
                        .filename
                        .clone()
                        .unwrap_or_else(|| "upload".to_owned()),
                    bytes,
                });
            }
            "source" => source = Some(String::from_utf8_lossy(&bytes).into_owned()),
            "target" => target = Some(String::from_utf8_lossy(&bytes).into_owned()),
            _ => {}
        }
    }
    let upload = upload.ok_or_else(|| ApiError::bad_request("'file' is required"))?;
    let target = TranslateBody::present(target)
        .ok_or_else(|| ApiError::bad_request("'target' is required"))?;
    let source = TranslateBody::present(source).unwrap_or_else(|| AUTO_SOURCE.to_owned());
    Ok(FileForm {
        upload,
        source,
        target,
    })
}

fn document_mime(format: DocumentFormat) -> &'static str {
    match format {
        DocumentFormat::Txt => "text/plain",
        #[cfg(feature = "odt")]
        DocumentFormat::Odt => "application/vnd.oasis.opendocument.text",
        #[cfg(feature = "epub")]
        DocumentFormat::Epub => "application/epub+zip",
        #[cfg(feature = "pdf")]
        DocumentFormat::Pdf => "application/pdf",
    }
}

#[cfg(feature = "ppocr")]
fn decode_image(bytes: &[u8]) -> Result<image::RgbaImage, image::ImageError> {
    use image::ImageDecoder;

    let mut decoder = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()?
        .into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut image = image::DynamicImage::from_decoder(decoder)?;
    image.apply_orientation(orientation);
    Ok(image.into_rgba8())
}

#[cfg(feature = "ppocr")]
fn encode_png(width: u32, height: u32, rgba: Vec<u8>) -> Result<Vec<u8>, image::ImageError> {
    let image = image::RgbaImage::from_raw(width, height, rgba).ok_or_else(|| {
        image::ImageError::Parameter(image::error::ParameterError::from_kind(
            image::error::ParameterErrorKind::DimensionMismatch,
        ))
    })?;
    let mut png = std::io::Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png)?;
    Ok(png.into_inner())
}
