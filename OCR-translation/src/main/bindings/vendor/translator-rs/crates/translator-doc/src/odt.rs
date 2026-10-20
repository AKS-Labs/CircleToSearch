//! OpenDocument Text translation.
//!
//! ODT files are ZIP packages containing XML. This module rewrites the XML
//! document parts directly, preserving the package and projecting inline
//! `text:span` styles through Bergamot token alignment.

use std::fmt;
use std::io::{Cursor, Read, Write};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use translator_core::api::{LanguageCode, ScriptedLanguage, TranslatorError};
use translator_translate::document_translator::DocumentTranslator;
use translator_translate::language_detect::detect_language_robust_code;
use translator_translate::translate::TranslationWithAlignment;

const ODT_MIMETYPE: &str = "application/vnd.oasis.opendocument.text";

#[derive(Debug)]
pub enum OdtTranslateError {
    InvalidInput(String),
    Zip(String),
    Io(String),
    Utf8(String),
    Translation(String),
    Cancelled,
}

impl fmt::Display for OdtTranslateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(message)
            | Self::Zip(message)
            | Self::Io(message)
            | Self::Utf8(message)
            | Self::Translation(message) => message.fmt(f),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl std::error::Error for OdtTranslateError {}

impl From<zip::result::ZipError> for OdtTranslateError {
    fn from(value: zip::result::ZipError) -> Self {
        Self::Zip(value.to_string())
    }
}

impl From<std::io::Error> for OdtTranslateError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value.to_string())
    }
}

impl From<std::string::FromUtf8Error> for OdtTranslateError {
    fn from(value: std::string::FromUtf8Error) -> Self {
        Self::Utf8(value.to_string())
    }
}

impl From<TranslatorError> for OdtTranslateError {
    fn from(value: TranslatorError) -> Self {
        Self::Translation(value.message)
    }
}

/// Translation abstraction for ODT rewriting.
///
/// The XML/package code depends only on this trait, so tests can provide a
/// deterministic translator while the session-backed implementation below uses
/// the same alignment-producing path as PDF styled translation.
pub trait OdtTextTranslator {
    fn translate_texts_with_alignment(
        &mut self,
        texts: &[String],
    ) -> Result<Vec<TranslationWithAlignment>, OdtTranslateError>;

    /// Cancellable, progress-reporting variant. `on_progress` is called from
    /// slimt worker threads with byte-weighted `(bytes_done, bytes_total)`.
    /// The default ignores progress and delegates to the plain method, which is
    /// fine for non-session translators (e.g. tests).
    fn translate_texts_with_alignment_ctx(
        &mut self,
        texts: &[String],
        _on_progress: &(dyn Fn(usize, usize) + Sync),
    ) -> Result<Vec<TranslationWithAlignment>, OdtTranslateError> {
        self.translate_texts_with_alignment(texts)
    }
}

pub struct DocumentOdtTranslator<'a> {
    translator: &'a dyn DocumentTranslator,
    forced_source_code: Option<&'a str>,
    target_code: &'a str,
    available_languages: &'a [ScriptedLanguage],
}

impl<'a> DocumentOdtTranslator<'a> {
    pub fn new(
        translator: &'a dyn DocumentTranslator,
        forced_source_code: Option<&'a str>,
        target_code: &'a str,
        available_languages: &'a [ScriptedLanguage],
    ) -> Self {
        Self {
            translator,
            forced_source_code,
            target_code,
            available_languages,
        }
    }
}

impl OdtTextTranslator for DocumentOdtTranslator<'_> {
    fn translate_texts_with_alignment(
        &mut self,
        texts: &[String],
    ) -> Result<Vec<TranslationWithAlignment>, OdtTranslateError> {
        self.translate_texts_with_alignment_ctx(texts, &|_, _| {})
    }

    fn translate_texts_with_alignment_ctx(
        &mut self,
        texts: &[String],
        on_progress: &(dyn Fn(usize, usize) + Sync),
    ) -> Result<Vec<TranslationWithAlignment>, OdtTranslateError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let target_code = LanguageCode::from(self.target_code);
        let source_code = match self.forced_source_code {
            Some(code) => LanguageCode::from(code),
            None => {
                let combined = texts.join(" ");
                detect_language_robust_code(&combined, None, self.available_languages).ok_or_else(
                    || {
                        OdtTranslateError::Translation(
                            "could not detect ODT source language".to_string(),
                        )
                    },
                )?
            }
        };

        if source_code == target_code {
            return Ok(texts
                .iter()
                .map(|text| TranslationWithAlignment {
                    source_text: text.clone(),
                    translated_text: text.clone(),
                    alignments: translator_translate::translate::identity_char_alignments(text),
                })
                .collect());
        }

        let translations = self
            .translator
            .translate_texts_with_alignment_ctx(&source_code, &target_code, texts, on_progress)
            .map_err(|error| {
                if error.is_cancelled() {
                    OdtTranslateError::Cancelled
                } else {
                    OdtTranslateError::Translation(error.message)
                }
            })?;

        let Some(translations) = translations else {
            return Err(OdtTranslateError::Translation(format!(
                "Language pair {} -> {} not installed",
                source_code.as_str(),
                target_code.as_str()
            )));
        };

        Ok(translations)
    }
}

pub fn translate_odt(
    translator: &dyn DocumentTranslator,
    odt_bytes: &[u8],
    forced_source_code: Option<&str>,
    target_code: &str,
    available_languages: &[ScriptedLanguage],
) -> Result<Vec<u8>, OdtTranslateError> {
    translate_odt_with_progress(
        translator,
        odt_bytes,
        forced_source_code,
        target_code,
        available_languages,
        |_| {},
    )
}

/// Progress is reported per sentence from slimt worker threads via
/// `on_progress` (cheap, non-blocking, thread-safe). Cancellation is requested
/// out-of-band via `TranslatorSession::cancel_ongoing_work` and surfaces as
/// [`OdtTranslateError::Cancelled`].
pub fn translate_odt_with_progress(
    translator: &dyn DocumentTranslator,
    odt_bytes: &[u8],
    forced_source_code: Option<&str>,
    target_code: &str,
    available_languages: &[ScriptedLanguage],
    on_progress: impl Fn(f32) + Sync,
) -> Result<Vec<u8>, OdtTranslateError> {
    translator.begin_document_translation();
    let mut adapter = DocumentOdtTranslator::new(
        translator,
        forced_source_code,
        target_code,
        available_languages,
    );
    translate_odt_with_translator_and_progress(odt_bytes, &mut adapter, on_progress)
}

pub fn translate_odt_with_translator(
    odt_bytes: &[u8],
    translator: &mut dyn OdtTextTranslator,
) -> Result<Vec<u8>, OdtTranslateError> {
    translate_odt_with_translator_and_progress(odt_bytes, translator, |_| {})
}

pub fn translate_odt_with_translator_and_progress(
    odt_bytes: &[u8],
    translator: &mut dyn OdtTextTranslator,
    on_progress: impl Fn(f32) + Sync,
) -> Result<Vec<u8>, OdtTranslateError> {
    let mut archive = ZipArchive::new(Cursor::new(odt_bytes))?;
    let mut entries = Vec::with_capacity(archive.len());
    let mut mimetype = None;

    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        let name = file.name().to_string();
        let mut data = Vec::new();
        if !file.is_dir() {
            file.read_to_end(&mut data)?;
        }
        if name == "mimetype" {
            mimetype = Some(data.clone());
        }
        entries.push(PackageEntry {
            name,
            data,
            compression: file.compression(),
            modified: file.last_modified(),
            unix_mode: file.unix_mode(),
            is_dir: file.is_dir(),
        });
    }

    match mimetype {
        Some(value) if value == ODT_MIMETYPE.as_bytes() => {}
        Some(_) => {
            return Err(OdtTranslateError::InvalidInput(
                "ODT mimetype entry is not application/vnd.oasis.opendocument.text".to_string(),
            ));
        }
        None => {
            return Err(OdtTranslateError::InvalidInput(
                "ODT package is missing mimetype entry".to_string(),
            ));
        }
    }

    let total_blocks = entries
        .iter()
        .filter(|entry| is_translatable_xml_entry(&entry.name) && !entry.is_dir)
        .map(|entry| {
            String::from_utf8(entry.data.clone())
                .map(|xml| count_translatable_text_blocks(&xml))
                .unwrap_or(0)
        })
        .sum();
    let mut translated_blocks = 0usize;
    on_progress(0.0);

    for entry in &mut entries {
        if is_translatable_xml_entry(&entry.name) && !entry.is_dir {
            let xml = String::from_utf8(entry.data.clone())?;
            entry.data = rewrite_odt_xml_with_progress(
                &xml,
                translator,
                total_blocks,
                &mut translated_blocks,
                &on_progress,
            )?
            .into_bytes();
            entry.compression = CompressionMethod::Deflated;
        }
    }

    write_odt_package(&entries)
}

#[derive(Debug)]
struct PackageEntry {
    name: String,
    data: Vec<u8>,
    compression: CompressionMethod,
    modified: Option<zip::DateTime>,
    unix_mode: Option<u32>,
    is_dir: bool,
}

fn write_odt_package(entries: &[PackageEntry]) -> Result<Vec<u8>, OdtTranslateError> {
    let cursor = Cursor::new(Vec::new());
    let mut writer = ZipWriter::new(cursor);

    let mimetype_options =
        SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    writer.start_file("mimetype", mimetype_options)?;
    writer.write_all(ODT_MIMETYPE.as_bytes())?;

    for entry in entries.iter().filter(|entry| entry.name != "mimetype") {
        let mut options = SimpleFileOptions::default().compression_method(entry.compression);
        if let Some(modified) = entry.modified {
            options = options.last_modified_time(modified);
        }
        if let Some(mode) = entry.unix_mode {
            options = options.unix_permissions(mode);
        }
        if entry.is_dir {
            writer.add_directory(entry.name.clone(), options)?;
        } else {
            writer.start_file(entry.name.clone(), options)?;
            writer.write_all(&entry.data)?;
        }
    }

    Ok(writer.finish()?.into_inner())
}

fn is_translatable_xml_entry(name: &str) -> bool {
    matches!(name, "content.xml" | "styles.xml")
        || name.ends_with("/content.xml")
        || name.ends_with("/styles.xml")
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct OdtTextStyle {
    style_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OdtStyleSpan {
    start: u32,
    end: u32,
    style: OdtTextStyle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OdtTextBlock {
    text: String,
    style_spans: Vec<OdtStyleSpan>,
}

#[cfg(test)]
fn rewrite_odt_xml(
    xml: &str,
    translator: &mut dyn OdtTextTranslator,
) -> Result<String, OdtTranslateError> {
    let total_blocks = count_translatable_text_blocks(xml);
    let mut translated_blocks = 0usize;
    rewrite_odt_xml_with_progress(
        xml,
        translator,
        total_blocks,
        &mut translated_blocks,
        &|_| {},
    )
}

fn rewrite_odt_xml_with_progress(
    xml: &str,
    translator: &mut dyn OdtTextTranslator,
    total_blocks: usize,
    translated_blocks: &mut usize,
    on_progress: &(dyn Fn(f32) + Sync),
) -> Result<String, OdtTranslateError> {
    let tokens = tokenize_xml(xml);
    let mut replacements = Vec::<BlockReplacement>::new();
    let mut texts = Vec::new();
    let mut index = 0usize;

    while index < tokens.len() {
        let token = &tokens[index];
        if let XmlTokenKind::Start {
            name, self_closing, ..
        } = &token.kind
        {
            if is_translatable_block(name) && !self_closing {
                let Some(end_index) = find_matching_end(&tokens, index) else {
                    index += 1;
                    continue;
                };
                let inner_tokens = &tokens[index + 1..end_index];
                if let Some(block) = collect_text_block(inner_tokens) {
                    if !block.text.trim().is_empty() {
                        texts.push(block.text.clone());
                        replacements.push(BlockReplacement {
                            start_token: index,
                            end_token: end_index,
                            source: block,
                            translation_index: texts.len() - 1,
                        });
                    }
                }
                index = end_index + 1;
                continue;
            }
        }
        index += 1;
    }

    if texts.is_empty() {
        return Ok(xml.to_string());
    }

    // One slimt call for the whole entry. The worker callback delivers a
    // byte-weighted fraction for this entry; project it onto the document-wide
    // block bar (this entry spans `entry_blocks` blocks starting at
    // `blocks_before`) and emit the global completion fraction.
    let entry_blocks = texts.len();
    let blocks_before = *translated_blocks;
    let report = |bytes_done: usize, bytes_total: usize| {
        let filled = if bytes_total == 0 {
            entry_blocks
        } else {
            bytes_done * entry_blocks / bytes_total
        };
        if total_blocks > 0 {
            on_progress((blocks_before + filled) as f32 / total_blocks as f32);
        }
    };
    let translations = translator.translate_texts_with_alignment_ctx(&texts, &report)?;
    *translated_blocks += entry_blocks;
    if total_blocks > 0 {
        on_progress(*translated_blocks as f32 / total_blocks as f32);
    }
    let mut output = String::with_capacity(xml.len());
    let mut replacement_index = 0usize;
    let mut token_index = 0usize;

    while token_index < tokens.len() {
        if replacement_index < replacements.len()
            && replacements[replacement_index].start_token == token_index
        {
            let replacement = &replacements[replacement_index];
            output.push_str(&tokens[replacement.start_token].raw);
            if let Some(translation) = translations.get(replacement.translation_index) {
                let style_spans = project_style_spans(&replacement.source, translation);
                output.push_str(&emit_translated_inline_xml(
                    &translation.translated_text,
                    &style_spans,
                ));
            } else {
                for token in &tokens[replacement.start_token + 1..replacement.end_token] {
                    output.push_str(&token.raw);
                }
            }
            output.push_str(&tokens[replacement.end_token].raw);
            token_index = replacement.end_token + 1;
            replacement_index += 1;
        } else {
            output.push_str(&tokens[token_index].raw);
            token_index += 1;
        }
    }

    Ok(output)
}

fn count_translatable_text_blocks(xml: &str) -> usize {
    let tokens = tokenize_xml(xml);
    let mut count = 0usize;
    let mut index = 0usize;

    while index < tokens.len() {
        let token = &tokens[index];
        if let XmlTokenKind::Start {
            name, self_closing, ..
        } = &token.kind
        {
            if is_translatable_block(name) && !self_closing {
                let Some(end_index) = find_matching_end(&tokens, index) else {
                    index += 1;
                    continue;
                };
                let inner_tokens = &tokens[index + 1..end_index];
                if let Some(block) = collect_text_block(inner_tokens)
                    && !block.text.trim().is_empty()
                {
                    count += 1;
                }
                index = end_index + 1;
                continue;
            }
        }
        index += 1;
    }

    count
}

#[derive(Debug)]
struct BlockReplacement {
    start_token: usize,
    end_token: usize,
    source: OdtTextBlock,
    translation_index: usize,
}

fn is_translatable_block(name: &str) -> bool {
    matches!(name, "text:p" | "text:h")
}

fn find_matching_end(tokens: &[XmlToken], start_index: usize) -> Option<usize> {
    let XmlTokenKind::Start { name, .. } = &tokens[start_index].kind else {
        return None;
    };
    let mut depth = 0usize;
    for (index, token) in tokens.iter().enumerate().skip(start_index) {
        match &token.kind {
            XmlTokenKind::Start {
                name: token_name,
                self_closing,
                ..
            } if token_name == name && !self_closing => depth += 1,
            XmlTokenKind::End { name: token_name } if token_name == name => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

fn collect_text_block(tokens: &[XmlToken]) -> Option<OdtTextBlock> {
    if contains_unsupported_inline_markup(tokens) {
        return None;
    }

    let mut text = String::new();
    let mut style_spans = Vec::new();
    let mut current_style: Option<OdtTextStyle> = None;
    let mut stack = Vec::<StyleFrame>::new();

    for token in tokens {
        match &token.kind {
            XmlTokenKind::Text(value) => {
                let start = text.len() as u32;
                text.push_str(value);
                let end = text.len() as u32;
                if start < end {
                    if let Some(style) = &current_style {
                        style_spans.push(OdtStyleSpan {
                            start,
                            end,
                            style: style.clone(),
                        });
                    }
                }
            }
            XmlTokenKind::Start {
                name,
                attrs,
                self_closing,
            } => {
                append_space_token(name, attrs, &mut text);
                if !self_closing {
                    let previous_style = if name == "text:span" {
                        style_from_attrs(attrs).map(|style| {
                            let previous = current_style.clone();
                            current_style = Some(style);
                            previous
                        })
                    } else {
                        None
                    };
                    stack.push(StyleFrame { previous_style });
                }
            }
            XmlTokenKind::End { .. } => {
                if let Some(frame) = stack.pop() {
                    if let Some(previous) = frame.previous_style {
                        current_style = previous;
                    }
                }
            }
            XmlTokenKind::Other => {}
        }
    }

    Some(OdtTextBlock {
        text,
        style_spans: merge_odt_style_spans(style_spans),
    })
}

#[derive(Debug)]
struct StyleFrame {
    previous_style: Option<Option<OdtTextStyle>>,
}

fn contains_unsupported_inline_markup(tokens: &[XmlToken]) -> bool {
    tokens.iter().any(|token| match &token.kind {
        XmlTokenKind::Start { name, .. } => !matches!(
            name.as_str(),
            "text:span" | "text:s" | "text:tab" | "text:line-break"
        ),
        XmlTokenKind::End { name } => !matches!(name.as_str(), "text:span"),
        _ => false,
    })
}

fn append_space_token(name: &str, attrs: &[(String, String)], text: &mut String) {
    match name {
        "text:s" => {
            let count = attr_value(attrs, "text:c")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(1);
            text.extend(std::iter::repeat_n(' ', count));
        }
        "text:tab" => text.push('\t'),
        "text:line-break" => text.push('\n'),
        _ => {}
    }
}

fn style_from_attrs(attrs: &[(String, String)]) -> Option<OdtTextStyle> {
    attr_value(attrs, "text:style-name").map(|style_name| OdtTextStyle {
        style_name: Some(style_name.to_string()),
    })
}

fn attr_value<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find_map(|(key, value)| (key == name).then_some(value.as_str()))
}

fn project_style_spans(
    source: &OdtTextBlock,
    translation: &TranslationWithAlignment,
) -> Vec<OdtStyleSpan> {
    if source.style_spans.is_empty() || translation.translated_text.is_empty() {
        return Vec::new();
    }
    if source.style_spans.len() == 1
        && source.style_spans[0].start == 0
        && source.style_spans[0].end as usize == source.text.len()
    {
        return vec![OdtStyleSpan {
            start: 0,
            end: translation.translated_text.len() as u32,
            style: source.style_spans[0].style.clone(),
        }];
    }

    let source_byte_at_char = char_to_byte_offsets(&source.text);
    let target_byte_at_char = char_to_byte_offsets(&translation.translated_text);
    let mut projected = Vec::new();

    for alignment in &translation.alignments {
        let source_begin = source_byte_at_char
            .get(alignment.src_begin as usize)
            .copied()
            .unwrap_or(source.text.len());
        let source_end = source_byte_at_char
            .get(alignment.src_end as usize)
            .copied()
            .unwrap_or(source.text.len());
        let source_mid = ((source_begin + source_end) / 2) as u32;
        let Some(source_span) = source
            .style_spans
            .iter()
            .find(|span| source_mid >= span.start && source_mid < span.end)
        else {
            continue;
        };
        let target_begin = target_byte_at_char
            .get(alignment.tgt_begin as usize)
            .copied()
            .unwrap_or(translation.translated_text.len());
        let target_end = target_byte_at_char
            .get(alignment.tgt_end as usize)
            .copied()
            .unwrap_or(translation.translated_text.len());
        let (start, end) =
            expand_byte_range_to_word(&translation.translated_text, target_begin, target_end);
        if start < end {
            projected.push(OdtStyleSpan {
                start: start as u32,
                end: end as u32,
                style: source_span.style.clone(),
            });
        }
    }

    merge_odt_style_spans(projected)
}

fn emit_translated_inline_xml(text: &str, style_spans: &[OdtStyleSpan]) -> String {
    let style_spans = normalize_odt_style_spans_for_output(text, style_spans);
    let mut output = String::new();
    let mut cursor = 0usize;

    for span in &style_spans {
        let start = (span.start as usize).min(text.len());
        let end = (span.end as usize).min(text.len());
        if start < cursor
            || start >= end
            || !text.is_char_boundary(start)
            || !text.is_char_boundary(end)
        {
            continue;
        }
        output.push_str(&escape_xml_text(&text[cursor..start]));
        if let Some(style_name) = &span.style.style_name {
            output.push_str("<text:span text:style-name=\"");
            output.push_str(&escape_xml_attr(style_name));
            output.push_str("\">");
            output.push_str(&escape_xml_text(&text[start..end]));
            output.push_str("</text:span>");
        } else {
            output.push_str(&escape_xml_text(&text[start..end]));
        }
        cursor = end;
    }

    output.push_str(&escape_xml_text(&text[cursor..]));
    output
}

fn normalize_odt_style_spans_for_output(text: &str, spans: &[OdtStyleSpan]) -> Vec<OdtStyleSpan> {
    let mut normalized = Vec::<OdtStyleSpan>::new();
    let text_len = text.len() as u32;

    for mut span in merge_odt_style_spans(spans.to_vec()) {
        span.start = span.start.min(text_len);
        span.end = span.end.min(text_len);
        if span.start >= span.end
            || !text.is_char_boundary(span.start as usize)
            || !text.is_char_boundary(span.end as usize)
        {
            continue;
        }

        if let Some(last) = normalized.last_mut() {
            if is_whitespace_range(text, last.end as usize, span.start as usize) {
                last.end = span.start;
            }
            if last.style == span.style && span.start <= last.end {
                last.end = last.end.max(span.end);
                continue;
            }
        } else if is_whitespace_range(text, 0, span.start as usize) {
            span.start = 0;
        }

        normalized.push(span);
    }

    if let Some(last) = normalized.last_mut() {
        if is_whitespace_range(text, last.end as usize, text.len()) {
            last.end = text_len;
        }
    }

    merge_odt_style_spans(normalized)
}

fn is_whitespace_range(text: &str, start: usize, end: usize) -> bool {
    start <= end
        && end <= text.len()
        && text.is_char_boundary(start)
        && text.is_char_boundary(end)
        && text[start..end].chars().all(char::is_whitespace)
}

fn merge_odt_style_spans(mut spans: Vec<OdtStyleSpan>) -> Vec<OdtStyleSpan> {
    spans.sort_by_key(|span| (span.start, span.end));
    let mut merged: Vec<OdtStyleSpan> = Vec::new();
    for span in spans {
        if span.start >= span.end {
            continue;
        }
        if let Some(last) = merged.last_mut() {
            if last.style == span.style && span.start <= last.end {
                last.end = last.end.max(span.end);
                continue;
            }
            if span.start < last.end {
                continue;
            }
        }
        merged.push(span);
    }
    merged
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct XmlToken {
    raw: String,
    kind: XmlTokenKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum XmlTokenKind {
    Start {
        name: String,
        attrs: Vec<(String, String)>,
        self_closing: bool,
    },
    End {
        name: String,
    },
    Text(String),
    Other,
}

fn tokenize_xml(xml: &str) -> Vec<XmlToken> {
    let mut tokens = Vec::new();
    let mut cursor = 0usize;
    while cursor < xml.len() {
        let Some(relative_start) = xml[cursor..].find('<') else {
            push_text_token(&mut tokens, &xml[cursor..]);
            break;
        };
        let tag_start = cursor + relative_start;
        if tag_start > cursor {
            push_text_token(&mut tokens, &xml[cursor..tag_start]);
        }
        let Some(tag_end) = find_tag_end(xml, tag_start) else {
            push_text_token(&mut tokens, &xml[tag_start..]);
            break;
        };
        let raw = &xml[tag_start..tag_end];
        tokens.push(XmlToken {
            raw: raw.to_string(),
            kind: parse_xml_tag(raw),
        });
        cursor = tag_end;
    }
    tokens
}

fn push_text_token(tokens: &mut Vec<XmlToken>, raw: &str) {
    if raw.is_empty() {
        return;
    }
    tokens.push(XmlToken {
        raw: raw.to_string(),
        kind: XmlTokenKind::Text(decode_xml_entities(raw)),
    });
}

fn find_tag_end(xml: &str, tag_start: usize) -> Option<usize> {
    let mut quote = None;
    for (offset, ch) in xml[tag_start..].char_indices() {
        match (quote, ch) {
            (Some(active), c) if c == active => quote = None,
            (None, '"' | '\'') => quote = Some(ch),
            (None, '>') => return Some(tag_start + offset + ch.len_utf8()),
            _ => {}
        }
    }
    None
}

fn parse_xml_tag(raw: &str) -> XmlTokenKind {
    if raw.starts_with("</") {
        let name = raw[2..raw.len().saturating_sub(1)]
            .trim()
            .split_whitespace()
            .next()
            .unwrap_or_default();
        return XmlTokenKind::End {
            name: name.to_string(),
        };
    }
    if raw.starts_with("<?") || raw.starts_with("<!") {
        return XmlTokenKind::Other;
    }

    let body = raw[1..raw.len().saturating_sub(1)].trim();
    let self_closing = body.ends_with('/');
    let body = body.trim_end_matches('/').trim_end();
    let name_end = body
        .find(|ch: char| ch.is_whitespace())
        .unwrap_or(body.len());
    let name = &body[..name_end];
    if name.is_empty() {
        return XmlTokenKind::Other;
    }
    XmlTokenKind::Start {
        name: name.to_string(),
        attrs: parse_attrs(&body[name_end..]),
        self_closing,
    }
}

fn parse_attrs(input: &str) -> Vec<(String, String)> {
    let mut attrs = Vec::new();
    let mut cursor = 0usize;
    while cursor < input.len() {
        cursor += input[cursor..]
            .chars()
            .take_while(|ch| ch.is_whitespace())
            .map(char::len_utf8)
            .sum::<usize>();
        if cursor >= input.len() {
            break;
        }
        let key_start = cursor;
        while cursor < input.len() {
            let ch = input[cursor..].chars().next().unwrap_or_default();
            if ch.is_whitespace() || ch == '=' {
                break;
            }
            cursor += ch.len_utf8();
        }
        let key = input[key_start..cursor].to_string();
        cursor += input[cursor..]
            .chars()
            .take_while(|ch| ch.is_whitespace())
            .map(char::len_utf8)
            .sum::<usize>();
        if !input[cursor..].starts_with('=') {
            attrs.push((key, String::new()));
            continue;
        }
        cursor += 1;
        cursor += input[cursor..]
            .chars()
            .take_while(|ch| ch.is_whitespace())
            .map(char::len_utf8)
            .sum::<usize>();
        let quote = input[cursor..].chars().next();
        let Some(quote @ ('"' | '\'')) = quote else {
            attrs.push((key, String::new()));
            continue;
        };
        cursor += quote.len_utf8();
        let value_start = cursor;
        while cursor < input.len() {
            let ch = input[cursor..].chars().next().unwrap_or_default();
            if ch == quote {
                break;
            }
            cursor += ch.len_utf8();
        }
        let value = decode_xml_entities(&input[value_start..cursor]);
        if cursor < input.len() {
            cursor += quote.len_utf8();
        }
        attrs.push((key, value));
    }
    attrs
}

fn decode_xml_entities(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0usize;
    while let Some(relative_amp) = input[cursor..].find('&') {
        let amp = cursor + relative_amp;
        output.push_str(&input[cursor..amp]);
        let Some(relative_semicolon) = input[amp..].find(';') else {
            output.push_str(&input[amp..]);
            return output;
        };
        let semicolon = amp + relative_semicolon;
        let entity = &input[amp + 1..semicolon];
        if let Some(decoded) = decode_entity(entity) {
            output.push(decoded);
        } else {
            output.push_str(&input[amp..=semicolon]);
        }
        cursor = semicolon + 1;
    }
    output.push_str(&input[cursor..]);
    output
}

fn decode_entity(entity: &str) -> Option<char> {
    match entity {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ if entity.starts_with("#x") => u32::from_str_radix(&entity[2..], 16)
            .ok()
            .and_then(char::from_u32),
        _ if entity.starts_with('#') => entity[1..].parse::<u32>().ok().and_then(char::from_u32),
        _ => None,
    }
}

fn escape_xml_text(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_xml_attr(input: &str) -> String {
    escape_xml_text(input).replace('"', "&quot;")
}

fn char_to_byte_offsets(s: &str) -> Vec<usize> {
    let mut table: Vec<usize> = s.char_indices().map(|(byte, _)| byte).collect();
    table.push(s.len());
    table
}

fn expand_byte_range_to_word(text: &str, start: usize, end: usize) -> (usize, usize) {
    let start = start.min(text.len());
    let end = end.min(text.len());
    if start >= end {
        return (start, end);
    }

    let mut word_start = None;
    for (byte, ch) in text.char_indices() {
        let ch_end = byte + ch.len_utf8();
        if ch_end <= start {
            continue;
        }
        if byte >= end {
            break;
        }
        if is_word_char(ch) {
            word_start = Some(byte);
            break;
        }
    }

    let Some(mut expanded_start) = word_start else {
        return (start, end);
    };
    let mut expanded_end = expanded_start
        + text[expanded_start..]
            .chars()
            .next()
            .map(char::len_utf8)
            .unwrap_or_default();

    while let Some((prev_start, prev)) = prev_char(text, expanded_start) {
        if !is_word_char(prev) {
            break;
        }
        expanded_start = prev_start;
    }
    while expanded_end < text.len() {
        let Some(next) = text[expanded_end..].chars().next() else {
            break;
        };
        if !is_word_char(next) {
            break;
        }
        expanded_end += next.len_utf8();
    }

    (expanded_start, expanded_end)
}

fn prev_char(text: &str, before: usize) -> Option<(usize, char)> {
    text[..before].char_indices().next_back()
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_' || ch == '\''
}

#[cfg(test)]
mod tests {
    use super::*;
    use translator_translate::translate::TokenAlignment;

    struct FakeTranslator {
        outputs: Vec<TranslationWithAlignment>,
        inputs: Vec<Vec<String>>,
    }

    impl OdtTextTranslator for FakeTranslator {
        fn translate_texts_with_alignment(
            &mut self,
            texts: &[String],
        ) -> Result<Vec<TranslationWithAlignment>, OdtTranslateError> {
            self.inputs.push(texts.to_vec());
            Ok(self.outputs.clone())
        }
    }

    #[test]
    fn rewrites_paragraph_and_projects_span_style() {
        let xml = r#"<office:text><text:p>Hello <text:span text:style-name="Strong">world</text:span>.</text:p></office:text>"#;
        let mut translator = FakeTranslator {
            inputs: Vec::new(),
            outputs: vec![TranslationWithAlignment {
                source_text: "Hello world.".to_string(),
                translated_text: "Hallo wereld.".to_string(),
                alignments: vec![
                    TokenAlignment {
                        src_begin: 0,
                        src_end: 5,
                        tgt_begin: 0,
                        tgt_end: 5,
                    },
                    TokenAlignment {
                        src_begin: 6,
                        src_end: 11,
                        tgt_begin: 6,
                        tgt_end: 13,
                    },
                ],
            }],
        };

        let rewritten = rewrite_odt_xml(xml, &mut translator).unwrap();

        assert_eq!(translator.inputs, vec![vec!["Hello world.".to_string()]]);
        assert_eq!(
            rewritten,
            r#"<office:text><text:p>Hallo <text:span text:style-name="Strong">wereld</text:span>.</text:p></office:text>"#
        );
    }

    #[test]
    fn decodes_spaces_and_entities_before_translation() {
        let xml = r#"<office:text><text:p>A&amp;B<text:s text:c="2"/>C</text:p></office:text>"#;
        let mut translator = FakeTranslator {
            inputs: Vec::new(),
            outputs: vec![TranslationWithAlignment {
                source_text: "A&B  C".to_string(),
                translated_text: "X&Y".to_string(),
                alignments: Vec::new(),
            }],
        };

        let rewritten = rewrite_odt_xml(xml, &mut translator).unwrap();

        assert_eq!(translator.inputs, vec![vec!["A&B  C".to_string()]]);
        assert_eq!(
            rewritten,
            r#"<office:text><text:p>X&amp;Y</text:p></office:text>"#
        );
    }

    #[test]
    fn emits_same_style_words_as_single_span() {
        let style = OdtTextStyle {
            style_name: Some("T1".to_string()),
        };
        let spans = vec![
            OdtStyleSpan {
                start: 0,
                end: 2,
                style: style.clone(),
            },
            OdtStyleSpan {
                start: 3,
                end: 8,
                style: style.clone(),
            },
            OdtStyleSpan {
                start: 9,
                end: 11,
                style: style.clone(),
            },
            OdtStyleSpan {
                start: 12,
                end: 19,
                style,
            },
        ];

        let emitted = emit_translated_inline_xml("El panel de control", &spans);

        assert_eq!(
            emitted,
            r#"<text:span text:style-name="T1">El panel de control</text:span>"#
        );
        assert!(!emitted.contains("</text:span> <text:span"));
    }

    #[test]
    fn emits_style_boundary_space_inside_previous_span() {
        let style_1 = OdtTextStyle {
            style_name: Some("T1".to_string()),
        };
        let style_2 = OdtTextStyle {
            style_name: Some("T2".to_string()),
        };
        let spans = vec![
            OdtStyleSpan {
                start: 0,
                end: 3,
                style: style_1,
            },
            OdtStyleSpan {
                start: 4,
                end: 7,
                style: style_2.clone(),
            },
            OdtStyleSpan {
                start: 8,
                end: 12,
                style: style_2,
            },
        ];

        let emitted = emit_translated_inline_xml("uno dos tres", &spans);

        assert_eq!(
            emitted,
            r#"<text:span text:style-name="T1">uno </text:span><text:span text:style-name="T2">dos tres</text:span>"#
        );
        assert!(!emitted.contains("</text:span> <text:span"));
    }

    #[test]
    fn skips_paragraph_with_unsupported_inline_markup() {
        let xml = r#"<office:text><text:p><draw:frame/>Hello</text:p></office:text>"#;
        let mut translator = FakeTranslator {
            inputs: Vec::new(),
            outputs: Vec::new(),
        };

        let rewritten = rewrite_odt_xml(xml, &mut translator).unwrap();

        assert_eq!(rewritten, xml);
        assert!(translator.inputs.is_empty());
    }

    #[test]
    fn rewrites_odt_package_with_mimetype_first_and_stored() {
        let input = make_test_odt(
            r#"<?xml version="1.0"?><office:text><text:p>Hello</text:p></office:text>"#,
        );
        let mut translator = FakeTranslator {
            inputs: Vec::new(),
            outputs: vec![TranslationWithAlignment {
                source_text: "Hello".to_string(),
                translated_text: "Hallo".to_string(),
                alignments: Vec::new(),
            }],
        };

        let output = translate_odt_with_translator(&input, &mut translator).unwrap();
        let mut archive = ZipArchive::new(Cursor::new(output)).unwrap();
        let first = archive.by_index(0).unwrap();
        assert_eq!(first.name(), "mimetype");
        assert_eq!(first.compression(), CompressionMethod::Stored);
        drop(first);

        let mut content = String::new();
        archive
            .by_name("content.xml")
            .unwrap()
            .read_to_string(&mut content)
            .unwrap();
        assert!(content.contains("<text:p>Hallo</text:p>"));
    }

    #[test]
    fn rewrites_embedded_object_content_xml() {
        let input = make_test_odt_with_entries(&[
            ("content.xml", r#"<?xml version="1.0"?><office:text/>"#),
            (
                "Object 1/content.xml",
                r#"<?xml version="1.0"?><office:chart><text:p>Column 1</text:p><text:p>Row 1</text:p></office:chart>"#,
            ),
        ]);
        let mut translator = FakeTranslator {
            inputs: Vec::new(),
            outputs: vec![
                TranslationWithAlignment {
                    source_text: "Column 1".to_string(),
                    translated_text: "Columna 1".to_string(),
                    alignments: Vec::new(),
                },
                TranslationWithAlignment {
                    source_text: "Row 1".to_string(),
                    translated_text: "Fila 1".to_string(),
                    alignments: Vec::new(),
                },
            ],
        };

        let output = translate_odt_with_translator(&input, &mut translator).unwrap();
        let mut archive = ZipArchive::new(Cursor::new(output)).unwrap();
        let mut object_content = String::new();
        archive
            .by_name("Object 1/content.xml")
            .unwrap()
            .read_to_string(&mut object_content)
            .unwrap();

        assert!(object_content.contains("<text:p>Columna 1</text:p>"));
        assert!(object_content.contains("<text:p>Fila 1</text:p>"));
    }

    fn make_test_odt(content_xml: &str) -> Vec<u8> {
        make_test_odt_with_entries(&[("content.xml", content_xml)])
    }

    fn make_test_odt_with_entries(entries: &[(&str, &str)]) -> Vec<u8> {
        let cursor = Cursor::new(Vec::new());
        let mut writer = ZipWriter::new(cursor);
        writer
            .start_file(
                "mimetype",
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(ODT_MIMETYPE.as_bytes()).unwrap();
        for (name, content) in entries {
            writer
                .start_file(
                    name,
                    SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
                )
                .unwrap();
            writer.write_all(content.as_bytes()).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }
}
