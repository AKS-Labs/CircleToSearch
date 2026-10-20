use std::path::{Path, PathBuf};

use crate::bergamot::{BergamotEngine, ModelPaths, TranslateCtx};
#[cfg(feature = "html")]
use crate::html_translate;
use crate::routing::{MixedTextTranslationResult, translate_mixed_texts_in_snapshot};
use translator_core::api::{LanguageCode, TranslatorError};
use translator_core::catalog::{CatalogSnapshot, translation_direction_from_files};

pub struct Translator<'a> {
    engine: &'a mut BergamotEngine,
    snapshot: &'a CatalogSnapshot,
}

impl<'a> Translator<'a> {
    pub fn new(engine: &'a mut BergamotEngine, snapshot: &'a CatalogSnapshot) -> Self {
        Self { engine, snapshot }
    }

    pub fn warm(
        &mut self,
        from_code: &LanguageCode,
        to_code: &LanguageCode,
    ) -> Result<(), TranslatorError> {
        let plan = resolve_translation_plan_in_snapshot(
            self.snapshot,
            from_code.as_str(),
            to_code.as_str(),
        )
        .ok_or_else(|| {
            TranslatorError::missing_asset(format!(
                "translation pack not installed for {}->{}",
                from_code.as_str(),
                to_code.as_str()
            ))
        })?;
        ensure_plan_loaded(self.engine, &plan).map_err(TranslatorError::translation)
    }

    pub fn translate_text(
        &mut self,
        from_code: &LanguageCode,
        to_code: &LanguageCode,
        text: &str,
    ) -> Result<String, TranslatorError> {
        let normalized = text.trim();
        if normalized.is_empty() {
            return Ok(String::new());
        }
        if from_code == to_code || normalized.parse::<f32>().is_ok() {
            return Ok(normalized.to_string());
        }

        let merged = translate_lines_preserving(normalized, |inputs| {
            self.translate_texts(from_code, to_code, &inputs)
                .map(|texts| texts.into_iter().map(plain_translation).collect())
        })?;
        Ok(merged.translated_text)
    }

    /// Translate one text and surface per-word alternatives. Only direct (non
    /// pivoted) language pairs carry alternatives; pivoted pairs translate
    /// normally with an empty alternatives list.
    pub fn translate_text_with_alternatives(
        &mut self,
        from_code: &LanguageCode,
        to_code: &LanguageCode,
        text: &str,
    ) -> Result<TranslationWithAlternatives, TranslatorError> {
        let normalized = text.trim();
        if normalized.is_empty() || from_code == to_code {
            return Ok(TranslationWithAlternatives {
                source_text: text.to_string(),
                translated_text: normalized.to_string(),
                alternatives: Vec::new(),
            });
        }
        let plan = resolve_translation_plan_in_snapshot(
            self.snapshot,
            from_code.as_str(),
            to_code.as_str(),
        )
        .ok_or_else(|| {
            TranslatorError::missing_asset(format!(
                "translation pack not installed for {}->{}",
                from_code.as_str(),
                to_code.as_str()
            ))
        })?;
        ensure_plan_loaded(self.engine, &plan).map_err(TranslatorError::translation)?;
        let mut result = translate_lines_preserving(normalized, |inputs| {
            match plan.steps.as_slice() {
                [step] => self
                    .engine
                    .translate_multiple_with_alternatives(&inputs, &step.cache_key)
                    .map_err(TranslatorError::translation),
                // Pivot (e.g. nl->en->es): translate the first hop plainly, then
                // run the final hop with alternatives so the offered swaps are in
                // the target language.
                [first, second] => {
                    let intermediate = self
                        .engine
                        .translate_multiple(&inputs, &first.cache_key)
                        .map_err(TranslatorError::translation)?;
                    self.engine
                        .translate_multiple_with_alternatives(&intermediate, &second.cache_key)
                        .map_err(TranslatorError::translation)
                }
                _ => Ok(inputs.into_iter().map(plain_translation).collect()),
            }
        })?;
        // `source_text` stays the original input so a later steer re-derives it.
        result.source_text = text.to_string();
        Ok(result)
    }

    /// Re-translate `source` forcing `forced_prefix` (confirmed target text up
    /// to and including a swapped word). For pivoted pairs the forcing applies
    /// to the final hop only.
    pub fn steer_text(
        &mut self,
        from_code: &LanguageCode,
        to_code: &LanguageCode,
        source: &str,
        forced_prefix: &str,
    ) -> Result<TranslationWithAlternatives, TranslatorError> {
        let plan = resolve_translation_plan_in_snapshot(
            self.snapshot,
            from_code.as_str(),
            to_code.as_str(),
        )
        .ok_or_else(|| {
            TranslatorError::missing_asset(format!(
                "translation pack not installed for {}->{}",
                from_code.as_str(),
                to_code.as_str()
            ))
        })?;
        ensure_plan_loaded(self.engine, &plan).map_err(TranslatorError::translation)?;
        match plan.steps.as_slice() {
            [step] => self
                .engine
                .steer(source, forced_prefix, &step.cache_key)
                .map_err(TranslatorError::translation),
            // Pivot: re-derive the intermediate (nl->en), then force the target
            // prefix on the final hop (en->es). Keep the original `source` so a
            // follow-up steer routes the same way.
            [first, second] => {
                let intermediate = self
                    .engine
                    .translate_multiple(&[source.to_string()], &first.cache_key)
                    .map_err(TranslatorError::translation)?;
                let pivot = intermediate.into_iter().next().unwrap_or_default();
                let mut result = self
                    .engine
                    .steer(&pivot, forced_prefix, &second.cache_key)
                    .map_err(TranslatorError::translation)?;
                result.source_text = source.to_string();
                Ok(result)
            }
            _ => Err(TranslatorError::translation(
                "cannot steer an empty translation plan",
            )),
        }
    }

    pub fn translate_texts(
        &mut self,
        from_code: &LanguageCode,
        to_code: &LanguageCode,
        texts: &[String],
    ) -> Result<Vec<String>, TranslatorError> {
        let plan = resolve_translation_plan_in_snapshot(
            self.snapshot,
            from_code.as_str(),
            to_code.as_str(),
        )
        .ok_or_else(|| {
            TranslatorError::missing_asset(format!(
                "translation pack not installed for {}->{}",
                from_code.as_str(),
                to_code.as_str()
            ))
        })?;
        execute_translation_plan(self.engine, &plan, texts).map_err(TranslatorError::translation)
    }

    /// Cancellable, progress-reporting [`Self::translate_texts`]. Errors with
    /// `TranslatorErrorKind::Cancelled` if the run was cancelled mid-flight.
    pub fn translate_texts_ctx(
        &mut self,
        from_code: &LanguageCode,
        to_code: &LanguageCode,
        texts: &[String],
        ctx: &TranslateCtx,
    ) -> Result<Vec<String>, TranslatorError> {
        let plan = resolve_translation_plan_in_snapshot(
            self.snapshot,
            from_code.as_str(),
            to_code.as_str(),
        )
        .ok_or_else(|| {
            TranslatorError::missing_asset(format!(
                "translation pack not installed for {}->{}",
                from_code.as_str(),
                to_code.as_str()
            ))
        })?;
        execute_translation_plan_ctx(self.engine, &plan, texts, ctx)
            .map_err(TranslatorError::translation)?
            .ok_or_else(TranslatorError::cancelled)
    }

    pub fn translate_html_fragments(
        &mut self,
        from_code: &LanguageCode,
        to_code: &LanguageCode,
        fragments: &[String],
    ) -> Result<Vec<String>, TranslatorError> {
        if fragments.is_empty() {
            return Ok(Vec::new());
        }
        if from_code == to_code {
            return Ok(fragments.to_vec());
        }
        let plan = resolve_translation_plan_in_snapshot(
            self.snapshot,
            from_code.as_str(),
            to_code.as_str(),
        )
        .ok_or_else(|| {
            TranslatorError::missing_asset(format!(
                "translation pack not installed for {}->{}",
                from_code.as_str(),
                to_code.as_str()
            ))
        })?;
        translate_html_via_dom(self.engine, &plan, fragments).map_err(TranslatorError::translation)
    }

    pub fn translate_mixed_texts(
        &mut self,
        inputs: &[String],
        forced_source_code: Option<&LanguageCode>,
        target_code: &LanguageCode,
        available_language_codes: &[LanguageCode],
    ) -> Result<MixedTextTranslationResult, TranslatorError> {
        let available_language_codes = available_language_codes
            .iter()
            .map(|code| code.as_str().to_string())
            .collect::<Vec<_>>();
        translate_mixed_texts_in_snapshot(
            self.engine,
            self.snapshot,
            inputs,
            forced_source_code.map(LanguageCode::as_str),
            target_code.as_str(),
            &available_language_codes,
        )
        .map_err(TranslatorError::translation)
    }

    pub fn translate_mixed_texts_with_alignment(
        &mut self,
        inputs: &[String],
        forced_source_code: Option<&LanguageCode>,
        target_code: &LanguageCode,
        available_language_codes: &[LanguageCode],
    ) -> Result<Vec<TranslationWithAlignment>, TranslatorError> {
        let available_language_codes = available_language_codes
            .iter()
            .map(|code| code.as_str().to_string())
            .collect::<Vec<_>>();
        crate::routing::translate_mixed_texts_with_alignment_in_snapshot(
            self.engine,
            self.snapshot,
            inputs,
            forced_source_code.map(LanguageCode::as_str),
            target_code.as_str(),
            &available_language_codes,
        )
        .map_err(TranslatorError::translation)
    }

    /// Alignment translation for documents, with cancellation + per-sentence
    /// progress. `Ok(None)` means no translation plan (passthrough); a
    /// cancelled run errors with `TranslatorErrorKind::Cancelled`.
    pub fn translate_texts_with_alignment_ctx(
        &mut self,
        from_code: &LanguageCode,
        to_code: &LanguageCode,
        texts: &[String],
        ctx: &TranslateCtx,
    ) -> Result<Option<Vec<TranslationWithAlignment>>, TranslatorError> {
        let Some(plan) = resolve_translation_plan_in_snapshot(
            self.snapshot,
            from_code.as_str(),
            to_code.as_str(),
        ) else {
            return Ok(None);
        };
        match execute_translation_plan_with_alignment_ctx(self.engine, &plan, texts, ctx)
            .map_err(TranslatorError::translation)?
        {
            Some(result) => Ok(Some(result)),
            None => Err(TranslatorError::cancelled()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct TokenAlignment {
    pub src_begin: u64,
    pub src_end: u64,
    pub tgt_begin: u64,
    pub tgt_end: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct TranslationWithAlignment {
    pub source_text: String,
    pub translated_text: String,
    pub alignments: Vec<TokenAlignment>,
}

/// A whole-word substitute the model would accept for a chosen target word.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct WordAlternative {
    pub text: String,
    pub prob: f32,
}

/// A low-confidence target word with the alternatives worth offering.
/// `tgt_begin`/`tgt_end` are char offsets into `TranslationWithAlternatives::translated_text`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct WordAlternatives {
    pub tgt_begin: u64,
    pub tgt_end: u64,
    pub confidence: f32,
    pub options: Vec<WordAlternative>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct TranslationWithAlternatives {
    pub source_text: String,
    pub translated_text: String,
    pub alternatives: Vec<WordAlternatives>,
}

fn plain_translation(text: String) -> TranslationWithAlternatives {
    TranslationWithAlternatives {
        source_text: String::new(),
        translated_text: text,
        alternatives: Vec::new(),
    }
}

/// Translate `text` while preserving its line structure. The input is split on
/// '\n'; only non-blank lines are handed to `translate` (which returns one
/// result per input line, in order), and blank lines are kept verbatim. The
/// engine collapses inter-sentence whitespace, so translating a multi-line
/// string as one input would flatten every paragraph break into a single space
/// — splitting here keeps the line breaks. Results are rejoined on '\n' and each
/// line's alternative char offsets are rebased into the joined output.
/// `source_text` is left empty for the caller to fill in.
fn translate_lines_preserving<E>(
    text: &str,
    translate: impl FnOnce(Vec<String>) -> Result<Vec<TranslationWithAlternatives>, E>,
) -> Result<TranslationWithAlternatives, E> {
    let mut lines: Vec<String> = text.split('\n').map(ToOwned::to_owned).collect();
    let non_empty: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| (!line.trim().is_empty()).then_some(index))
        .collect();
    let inputs = non_empty
        .iter()
        .map(|&index| lines[index].clone())
        .collect();
    let translated = translate(inputs)?;

    for (&index, piece) in non_empty.iter().zip(translated.iter()) {
        lines[index] = piece.translated_text.clone();
    }
    let mut line_start = Vec::with_capacity(lines.len());
    let mut cursor = 0u64;
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            cursor += 1; // the rejoined '\n'
        }
        line_start.push(cursor);
        cursor += line.chars().count() as u64;
    }
    let alternatives = non_empty
        .iter()
        .zip(translated)
        .flat_map(|(&index, piece)| {
            let base = line_start[index];
            piece
                .alternatives
                .into_iter()
                .map(move |alt| WordAlternatives {
                    tgt_begin: alt.tgt_begin + base,
                    tgt_end: alt.tgt_end + base,
                    ..alt
                })
        })
        .collect();
    Ok(TranslationWithAlternatives {
        source_text: String::new(),
        translated_text: lines.join("\n"),
        alternatives,
    })
}

/// Byte offset of each char in `s`, plus a trailing `s.len()` sentinel — index by char
/// position to convert char offsets (what [`TokenAlignment`] uses) to byte offsets.
fn char_byte_offsets(s: &str) -> Vec<usize> {
    let mut v: Vec<usize> = s.char_indices().map(|(b, _)| b).collect();
    v.push(s.len());
    v
}

/// Project source byte ranges onto the translation via the char-offset alignments: each
/// source range becomes the byte span covering every target token aligned to a source token
/// it overlaps. Used to carry per-word bold from the OCR source text onto the translated
/// text. Returns coalesced, sorted target byte ranges; empty when there are no alignments.
pub fn remap_byte_ranges_through_alignment(
    src_ranges: &[(u32, u32)],
    twa: &TranslationWithAlignment,
) -> Vec<(u32, u32)> {
    if src_ranges.is_empty() || twa.alignments.is_empty() {
        return Vec::new();
    }
    let src_char_byte = char_byte_offsets(&twa.source_text);
    let tgt_char_byte = char_byte_offsets(&twa.translated_text);
    let tgt_len = twa.translated_text.len();
    let mut out: Vec<(u32, u32)> = Vec::new();
    for &(bs, be) in src_ranges {
        let c0 = src_char_byte.partition_point(|&b| b < bs as usize);
        let c1 = src_char_byte.partition_point(|&b| b < be as usize);
        let (mut lo, mut hi) = (usize::MAX, 0usize);
        for a in &twa.alignments {
            if (a.src_begin as usize) < c1 && (a.src_end as usize) > c0 {
                lo = lo.min(a.tgt_begin as usize);
                hi = hi.max(a.tgt_end as usize);
            }
        }
        if lo < hi {
            let s = tgt_char_byte.get(lo).copied().unwrap_or(0);
            let e = tgt_char_byte.get(hi).copied().unwrap_or(tgt_len);
            out.push((s as u32, e as u32));
        }
    }
    out.sort_by_key(|r| r.0);
    let mut merged: Vec<(u32, u32)> = Vec::with_capacity(out.len());
    for r in out {
        match merged.last_mut() {
            Some(last) if r.0 <= last.1 => last.1 = last.1.max(r.1),
            _ => merged.push(r),
        }
    }
    merged
}

/// One-to-one character alignment for an untranslated passthrough (e.g. source
/// language equals target): char `i` of the source maps to char `i` of the
/// "translation". Used by the document translators when no model is run.
pub fn identity_char_alignments(text: &str) -> Vec<TokenAlignment> {
    let count = text.chars().count() as u64;
    (0..count)
        .map(|idx| TokenAlignment {
            src_begin: idx,
            src_end: idx + 1,
            tgt_begin: idx,
            tgt_end: idx + 1,
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranslationStep {
    pub from_code: String,
    pub to_code: String,
    pub cache_key: String,
    pub paths: ModelPaths,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TranslationPlan {
    pub steps: Vec<TranslationStep>,
}

fn absolute_install_path(base_dir: &str, install_path: &str) -> PathBuf {
    Path::new(base_dir).join(install_path)
}

/// Target languages decoded over the full vocabulary instead of the lexical
/// shortlist. The shortlist admits, per source word, the target pieces that
/// word aligned to in running text, so a source word the target language
/// renders as a suffix or a context word ("please" as the polite -ڭ ending)
/// never has its standalone rendering admitted, and on short inputs the table
/// starves the decoder of the only right pieces. Whether that costs more than
/// it constrains is a per-pack trade: the en-es table stops a model that
/// otherwise invents ("1987" → "Categoría: 1987"), and the ug-en table holds
/// FLORES up by 0.8 chrF++, while the en-ug model gains on every short golden
/// line without it and pays ~2× decode time.
const FULL_VOCABULARY_TARGETS: &[&str] = &["ug"];

fn uses_shortlist(to_code: &str) -> bool {
    !FULL_VOCABULARY_TARGETS.contains(&to_code)
}

fn build_model_paths(
    base_dir: &str,
    step: &translator_core::language::LanguageDirection,
    to_code: &str,
) -> ModelPaths {
    let src_vocab = absolute_install_path(base_dir, &step.src_vocab.path);
    let tgt_vocab = absolute_install_path(base_dir, &step.tgt_vocab.path);
    // Most catalog packs ship a single shared `vocab.*.spm` and the catalog
    // points both src_vocab and tgt_vocab at the same file. Mozilla's CJK
    // pairs (en-zh / en-ja / en-ko / en-zh_hant / zh_hant-en) ship distinct
    // `srcvocab.*.spm` + `trgvocab.*.spm`; pass the second one as
    // `target_vocabulary` only when it really differs from the source.
    let target_vocabulary = (src_vocab != tgt_vocab).then(|| tgt_vocab);
    ModelPaths {
        model: absolute_install_path(base_dir, &step.model.path),
        vocabulary: src_vocab,
        shortlist: uses_shortlist(to_code).then(|| absolute_install_path(base_dir, &step.lex.path)),
        target_vocabulary,
    }
}

fn cache_key(from_code: &str, to_code: &str) -> String {
    format!("{from_code}-{to_code}")
}

pub fn resolve_translation_plan_in_snapshot(
    snapshot: &CatalogSnapshot,
    from_code: &str,
    to_code: &str,
) -> Option<TranslationPlan> {
    if from_code == to_code {
        return Some(TranslationPlan::default());
    }

    let step = |from: &str, to: &str| {
        let pack_id = snapshot.catalog.translation_pack_id(from, to)?;
        let status = snapshot.pack_statuses.get(&pack_id)?;
        if !status.installed {
            return None;
        }
        let direction = translation_direction_from_files(&status.active_files)?;
        Some(TranslationStep {
            from_code: from.to_string(),
            to_code: to.to_string(),
            cache_key: cache_key(from, to),
            paths: build_model_paths(&snapshot.base_dir, &direction, to),
        })
    };

    let steps = if from_code == "en" {
        vec![step("en", to_code)?]
    } else if to_code == "en" {
        vec![step(from_code, "en")?]
    } else {
        vec![step(from_code, "en")?, step("en", to_code)?]
    };

    Some(TranslationPlan { steps })
}

pub fn execute_translation_plan(
    engine: &mut BergamotEngine,
    plan: &TranslationPlan,
    texts: &[String],
) -> Result<Vec<String>, String> {
    ensure_plan_loaded(engine, plan)?;
    match plan.steps.as_slice() {
        [step] => engine.translate_multiple(texts, &step.cache_key),
        [first, second] => engine.pivot_multiple(&first.cache_key, &second.cache_key, texts),
        _ => Ok(Vec::new()),
    }
}

/// Cancellable, progress-reporting [`execute_translation_plan`]. `Ok(None)`
/// means the run was cancelled.
pub fn execute_translation_plan_ctx(
    engine: &mut BergamotEngine,
    plan: &TranslationPlan,
    texts: &[String],
    ctx: &TranslateCtx,
) -> Result<Option<Vec<String>>, String> {
    ensure_plan_loaded(engine, plan)?;
    match plan.steps.as_slice() {
        [step] => engine.translate_multiple_ctx(texts, &step.cache_key, ctx),
        [first, second] => {
            engine.pivot_multiple_ctx(&first.cache_key, &second.cache_key, texts, ctx)
        }
        _ => Ok(Some(Vec::new())),
    }
}

pub fn execute_translation_plan_with_alignment(
    engine: &mut BergamotEngine,
    plan: &TranslationPlan,
    texts: &[String],
) -> Result<Vec<TranslationWithAlignment>, String> {
    ensure_plan_loaded(engine, plan)?;
    if log::log_enabled!(log::Level::Trace) {
        for (i, t) in texts.iter().enumerate() {
            log::trace!("[bergamot in {i}/{}] {:?}", texts.len(), t);
        }
    }
    let result = match plan.steps.as_slice() {
        [step] => engine.translate_multiple_with_alignment(texts, &step.cache_key),
        [first, second] => {
            engine.pivot_multiple_with_alignment(&first.cache_key, &second.cache_key, texts)
        }
        _ => Ok(Vec::new()),
    }?;
    if log::log_enabled!(log::Level::Trace) {
        for (i, t) in result.iter().enumerate() {
            log::trace!(
                "[bergamot out {i}/{}] {:?}",
                result.len(),
                t.translated_text
            );
        }
    }
    Ok(result)
}

/// Cancellable, progress-reporting [`execute_translation_plan_with_alignment`].
/// `Ok(None)` means cancelled.
pub fn execute_translation_plan_with_alignment_ctx(
    engine: &mut BergamotEngine,
    plan: &TranslationPlan,
    texts: &[String],
    ctx: &TranslateCtx,
) -> Result<Option<Vec<TranslationWithAlignment>>, String> {
    ensure_plan_loaded(engine, plan)?;
    match plan.steps.as_slice() {
        [step] => engine.translate_multiple_with_alignment_ctx(texts, &step.cache_key, ctx),
        [first, second] => engine.pivot_multiple_with_alignment_ctx(
            &first.cache_key,
            &second.cache_key,
            texts,
            ctx,
        ),
        _ => Ok(Some(Vec::new())),
    }
}

/// HTML translation runs entirely Rust-side: html5ever parses each fragment,
/// scope-grouped text leaves are flattened to plain strings, slimt translates
/// them with token alignments, and we splice the translated content back into
/// the same DOM nodes (no structural changes — `<p>` stays `<p>`, attributes
/// pass through verbatim). This replaces slimt's old C++ HTML mode.
#[cfg(feature = "html")]
pub fn translate_html_via_dom(
    engine: &mut BergamotEngine,
    plan: &TranslationPlan,
    fragments: &[String],
) -> Result<Vec<String>, String> {
    html_translate::translate_html_with(fragments, |scope_texts| {
        execute_translation_plan_with_alignment(engine, plan, scope_texts)
    })
}

#[cfg(not(feature = "html"))]
pub fn translate_html_via_dom(
    _engine: &mut BergamotEngine,
    _plan: &TranslationPlan,
    _fragments: &[String],
) -> Result<Vec<String>, String> {
    Err("HTML translation requires the `html` feature".to_string())
}

fn ensure_plan_loaded(engine: &mut BergamotEngine, plan: &TranslationPlan) -> Result<(), String> {
    for step in &plan.steps {
        engine.load_model_into_cache(&step.paths, &step.cache_key)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_uyghur_target_decodes_without_shortlist() {
        assert!(!super::uses_shortlist("ug"));
        assert!(super::uses_shortlist("en"));
        assert!(super::uses_shortlist("es"));
        assert!(super::uses_shortlist("ka"));
    }

    use super::*;

    fn line_with_leading_alternative(text: &str, len: u64) -> TranslationWithAlternatives {
        TranslationWithAlternatives {
            source_text: text.to_string(),
            translated_text: text.to_string(),
            alternatives: vec![WordAlternatives {
                tgt_begin: 0,
                tgt_end: len,
                confidence: 1.0,
                options: Vec::new(),
            }],
        }
    }

    #[test]
    fn preserves_blank_paragraph_breaks() {
        let out = translate_lines_preserving::<()>("First para.\n\nSecond para.", |inputs| {
            Ok(inputs.into_iter().map(plain_translation).collect())
        })
        .unwrap();
        assert_eq!(out.translated_text, "First para.\n\nSecond para.");
    }

    #[test]
    fn rebases_alternative_offsets_across_lines() {
        // Two non-blank lines separated by a blank line; each line's translation
        // carries an alternative over its first five chars.
        let out = translate_lines_preserving::<()>("Hello world.\n\nSecond line.", |inputs| {
            Ok(inputs
                .into_iter()
                .map(|line| line_with_leading_alternative(&line, 5))
                .collect())
        })
        .unwrap();

        assert_eq!(out.translated_text, "Hello world.\n\nSecond line.");
        let spans: Vec<(u64, u64)> = out
            .alternatives
            .iter()
            .map(|a| (a.tgt_begin, a.tgt_end))
            .collect();
        // Line one starts at char 0; line two starts after "Hello world." (12) +
        // two '\n' separators = char 14.
        assert_eq!(spans, vec![(0, 5), (14, 19)]);
    }
}
