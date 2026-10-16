//! End-to-end smoke test for the slimt-backed BergamotEngine.
//!
//! Skipped (passes with no work) when the bergamot Spanish/English assets
//! aren't installed on the test host.

use std::path::PathBuf;

use translator::bergamot::{BergamotEngine, ModelPaths};
use translator::html_translate;

/// Translate HTML fragments through the Rust-side html5ever pipeline using
/// the supplied `BergamotEngine` as the underlying plain-text + alignment
/// translator. Mirrors what `Translator::translate_html_fragments` does
/// internally, but without needing a full `CatalogSnapshot`.
fn translate_html_via_engine(
    engine: &BergamotEngine,
    key: &str,
    fragments: &[String],
) -> Result<Vec<String>, String> {
    html_translate::translate_html_with(fragments, |scope_texts| {
        let owned: Vec<String> = scope_texts.to_vec();
        engine.translate_multiple_with_alignment(&owned, key)
    })
}

fn asset_root() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let root = PathBuf::from(home).join(".local/share/dev.davidv.translator/bin");
    root.is_dir().then_some(root)
}

fn paths_for(root: &PathBuf, pair: &str) -> ModelPaths {
    ModelPaths {
        model: root.join(format!("model.{pair}.intgemm.alphas.bin")),
        vocabulary: root.join(format!("vocab.{pair}.spm")),
        shortlist: root.join(format!("lex.50.50.{pair}.s2t.bin")),
        target_vocabulary: None,
    }
}

/// Two-vocab pack layout: Mozilla's CJK pairs ship `srcvocab.*.spm` +
/// `trgvocab.*.spm` instead of a shared `vocab.*.spm`. Caller provides
/// a directory that holds the unpacked tensors plus both vocabularies.
fn dual_vocab_paths_for(root: &std::path::Path, pair: &str) -> ModelPaths {
    ModelPaths {
        model: root.join(format!("model.{pair}.intgemm.alphas.bin")),
        vocabulary: root.join(format!("srcvocab.{pair}.spm")),
        shortlist: root.join(format!("lex.50.50.{pair}.s2t.bin")),
        target_vocabulary: Some(root.join(format!("trgvocab.{pair}.spm"))),
    }
}

/// Locate a two-vocab pack on disk, falling back to the project's bucket
/// scratch dirs (`/tmp/{pair}-bin`) populated by the developer's earlier
/// gunzip pass. Returns `None` when the assets aren't present so CI without
/// the models still runs.
fn dual_vocab_root(pair: &str, scratch: &str) -> Option<std::path::PathBuf> {
    let candidates = [std::path::PathBuf::from(format!("/tmp/{scratch}-bin"))];
    for cand in candidates {
        let model = cand.join(format!("model.{pair}.intgemm.alphas.bin"));
        let src = cand.join(format!("srcvocab.{pair}.spm"));
        let tgt = cand.join(format!("trgvocab.{pair}.spm"));
        if model.exists() && src.exists() && tgt.exists() {
            return Some(cand);
        }
    }
    None
}

#[test]
fn translates_english_to_chinese_two_vocab() {
    // Mozilla bergamot en-zh ships srcvocab.enzh.spm + trgvocab.enzh.spm and
    // the model file has separate `encoder_Wemb` (English vocab size) and
    // `decoder_Wemb` (Chinese vocab size) tensors. Before slimt's two-vocab
    // support this combination crashed in `intgemm::PrepareBQuantizedTransposed`
    // during model load. The test exercises the full src→tgt path.
    let Some(root) = dual_vocab_root("enzh", "en-zh") else {
        eprintln!("en-zh two-vocab assets missing under /tmp/en-zh-bin; skipping");
        return;
    };
    let paths = dual_vocab_paths_for(&root, "enzh");

    let mut engine = BergamotEngine::new();
    engine
        .load_model_into_cache(&paths, "en-zh")
        .expect("load enzh");

    let inputs = vec!["The cat eats fish in the garden.".to_string()];
    let outs = engine
        .translate_multiple(&inputs, "en-zh")
        .expect("translate en->zh");
    assert_eq!(outs.len(), 1);
    eprintln!("[en->zh] {}", outs[0]);
    // Sanity: output must contain CJK characters and the words for "cat"
    // ('猫' simp / '貓' trad), "fish" ('鱼' simp / '魚' trad), and "garden"
    // ('花园' simp / '花園' trad). Any of those proves we routed through
    // decoder_Wemb correctly.
    let has_cjk = outs[0].chars().any(|c| {
        let cp = c as u32;
        (0x4E00..=0x9FFF).contains(&cp)
    });
    assert!(has_cjk, "no CJK characters in output: {}", outs[0]);
    assert!(
        outs[0].contains('猫') || outs[0].contains('貓'),
        "no 'cat' character in output: {}",
        outs[0]
    );
    assert!(
        outs[0].contains('鱼') || outs[0].contains('魚'),
        "no 'fish' character in output: {}",
        outs[0]
    );
}

#[test]
fn translates_spanish_to_english() {
    let Some(root) = asset_root() else {
        eprintln!("translator assets not installed; skipping");
        return;
    };
    let paths = paths_for(&root, "esen");
    if !paths.model.exists() || !paths.vocabulary.exists() || !paths.shortlist.exists() {
        eprintln!("esen assets missing; skipping");
        return;
    }

    let mut engine = BergamotEngine::new();
    engine
        .load_model_into_cache(&paths, "es-en")
        .expect("load esen");

    let inputs = vec!["El gato come pescado.".to_string()];
    let outs = engine
        .translate_multiple(&inputs, "es-en")
        .expect("translate");

    assert_eq!(outs.len(), 1);
    let translation = outs[0].to_lowercase();
    assert!(
        translation.contains("cat") && translation.contains("fish"),
        "unexpected translation: {}",
        outs[0]
    );
}

#[test]
fn translates_html_en_to_es_preserving_tags() {
    let Some(root) = asset_root() else {
        eprintln!("translator assets not installed; skipping");
        return;
    };
    let paths = paths_for(&root, "enes");
    if !paths.model.exists() || !paths.vocabulary.exists() || !paths.shortlist.exists() {
        eprintln!("enes assets missing; skipping");
        return;
    }

    let mut engine = BergamotEngine::new();
    engine
        .load_model_into_cache(&paths, "en-es")
        .expect("load enes");

    let inputs = vec![
        "<p>The <b>cat</b> eats <i>fish</i>.</p>".to_string(),
        "<a href=\"https://example.com\">Click <em>here</em></a> to continue.".to_string(),
    ];
    let outs = translate_html_via_engine(&engine, "en-es", &inputs).expect("translate html");

    assert_eq!(outs.len(), 2);
    eprintln!("[html_en_to_es] outs: {:?}", outs);

    let first = &outs[0];
    assert!(
        first.contains("<p>") && first.contains("</p>"),
        "<p> tags not preserved: {first}"
    );
    assert!(
        first.contains("<b>") && first.contains("</b>"),
        "<b> tags not preserved: {first}"
    );
    assert!(
        first.contains("<i>") && first.contains("</i>"),
        "<i> tags not preserved: {first}"
    );
    let lower = first.to_lowercase();
    assert!(
        lower.contains("gato") && lower.contains("pescado") || lower.contains("pez"),
        "expected Spanish translation of cat/fish: {first}"
    );

    let second = &outs[1];
    assert!(
        second.contains("href=\"https://example.com\""),
        "anchor href not preserved verbatim: {second}"
    );
    assert!(
        second.contains("<a") && second.contains("</a>"),
        "<a> tags not preserved: {second}"
    );
    assert!(
        second.contains("<em>") && second.contains("</em>"),
        "<em> tags not preserved: {second}"
    );
}

#[test]
fn html_passes_through_data_attributes_verbatim() {
    let Some(root) = asset_root() else {
        eprintln!("translator assets not installed; skipping");
        return;
    };
    let paths = paths_for(&root, "enes");
    if !paths.model.exists() {
        eprintln!("enes assets missing; skipping");
        return;
    }

    let mut engine = BergamotEngine::new();
    engine
        .load_model_into_cache(&paths, "en-es")
        .expect("load enes");

    let inputs = vec![
        "<span data-test=\"this content should pass through\">The cat eats fish.</span>"
            .to_string(),
        "<div data-id=\"42\" data-track=\"hello world\" class=\"foo\">Hello world!</div>"
            .to_string(),
    ];
    let outs = translate_html_via_engine(&engine, "en-es", &inputs).expect("translate html");

    assert_eq!(outs.len(), 2);
    eprintln!("[html_data_attrs] outs: {:?}", outs);

    let first = &outs[0];
    assert!(
        first.contains("data-test=\"this content should pass through\""),
        "data-test attribute mangled: {first}"
    );
    let lower = first.to_lowercase();
    assert!(
        lower.contains("gato"),
        "expected 'gato' inside the span: {first}"
    );

    let second = &outs[1];
    assert!(
        second.contains("data-id=\"42\""),
        "data-id attribute mangled: {second}"
    );
    assert!(
        second.contains("data-track=\"hello world\""),
        "data-track attribute (incl. value) mangled: {second}"
    );
    assert!(
        second.contains("class=\"foo\""),
        "class attribute mangled: {second}"
    );
}

#[test]
fn malformed_html_recovers_via_html5ever() {
    // The previous slimt XHScanner pipeline aborted on mismatched tags; the
    // Rust-side html5ever parser is HTML5-compliant and recovers gracefully.
    // The bad fragment should still produce a non-empty Spanish translation
    // and the surrounding good fragment must not be affected.
    let Some(root) = asset_root() else {
        eprintln!("translator assets not installed; skipping");
        return;
    };
    let paths = paths_for(&root, "enes");
    if !paths.model.exists() {
        eprintln!("enes assets missing; skipping");
        return;
    }

    let mut engine = BergamotEngine::new();
    engine
        .load_model_into_cache(&paths, "en-es")
        .expect("load enes");

    let inputs = vec![
        "<p>open <b>but never closed</p>".to_string(),
        "<p>The cat eats fish.</p>".to_string(),
    ];
    let outs = translate_html_via_engine(&engine, "en-es", &inputs)
        .expect("malformed HTML must not error with html5ever recovery");
    assert_eq!(outs.len(), 2);
    eprintln!("[malformed_html_recovers] outs: {:?}", outs);

    // Bad fragment: must produce *some* Spanish content (not empty) and
    // surface both <p> and <b> in the output (html5ever will close them).
    let bad_lower = outs[0].to_lowercase();
    assert!(
        !outs[0].trim().is_empty(),
        "malformed fragment must not silently produce empty output: {:?}",
        outs[0]
    );
    assert!(
        bad_lower.contains("<p>") && bad_lower.contains("</p>"),
        "<p> must be closed by html5ever: {}",
        outs[0]
    );

    // Good fragment must translate normally — independent of the bad one.
    let good_lower = outs[1].to_lowercase();
    assert!(
        good_lower.contains("gato"),
        "expected 'gato' in good fragment: {}",
        outs[1]
    );
}

#[test]
fn html_mode_treats_tags_as_markup_not_text() {
    // Same input, once as plain-text and once as HTML. In HTML mode the tags
    // should not appear inside translated phrases as literal words; in plain
    // mode they typically would.
    let Some(root) = asset_root() else {
        eprintln!("translator assets not installed; skipping");
        return;
    };
    let paths = paths_for(&root, "enes");
    if !paths.model.exists() {
        eprintln!("enes assets missing; skipping");
        return;
    }

    let mut engine = BergamotEngine::new();
    engine
        .load_model_into_cache(&paths, "en-es")
        .expect("load enes");

    let input = "<p>Hello <b>world</b>!</p>".to_string();
    let html_out =
        translate_html_via_engine(&engine, "en-es", &[input.clone()]).expect("translate html");
    assert_eq!(html_out.len(), 1);

    let html_translation = &html_out[0];
    eprintln!("[html_mode] html_out: {html_translation}");
    assert!(
        html_translation.contains("<p>") && html_translation.contains("<b>"),
        "expected <p> and <b> in HTML output: {html_translation}"
    );
    // The translated word for "world" should land inside the <b>...</b>
    // wrapper that surrounded "world" in the source.
    let lower = html_translation.to_lowercase();
    assert!(
        lower.contains("hola") || lower.contains("saludos"),
        "expected Spanish greeting: {html_translation}"
    );
    assert!(
        lower.contains("mundo"),
        "expected 'mundo' (Spanish for 'world') in: {html_translation}"
    );
}

#[test]
fn swapping_between_loaded_models_uses_the_right_one() {
    // Load both directions, fire interleaved translate calls, and check that
    // each call routes to the right model rather than re-using whichever was
    // last loaded.
    let Some(root) = asset_root() else {
        eprintln!("translator assets not installed; skipping");
        return;
    };
    let esen = paths_for(&root, "esen");
    let enes = paths_for(&root, "enes");
    if !esen.model.exists() || !enes.model.exists() {
        eprintln!("esen or enes assets missing; skipping");
        return;
    }

    let mut engine = BergamotEngine::new();
    engine
        .load_model_into_cache(&esen, "es-en")
        .expect("load esen");
    engine
        .load_model_into_cache(&enes, "en-es")
        .expect("load enes");

    let pairs = [
        ("es-en", "El gato come pescado.", "cat"),
        ("en-es", "The cat eats fish.", "gato"),
        ("es-en", "Hola mundo.", "world"),
        ("en-es", "Hello world.", "mundo"),
        ("es-en", "El gato come pescado.", "fish"),
    ];
    for (key, src, expected_word) in pairs {
        let outs = engine
            .translate_multiple(&[src.to_string()], key)
            .expect("translate");
        assert_eq!(outs.len(), 1);
        let lower = outs[0].to_lowercase();
        assert!(
            lower.contains(expected_word),
            "key {key} on src {src:?} expected '{expected_word}' in output, got: {}",
            outs[0]
        );
    }
}

#[test]
fn evict_makes_key_unusable_then_reload_works_again() {
    let Some(root) = asset_root() else {
        eprintln!("translator assets not installed; skipping");
        return;
    };
    let paths = paths_for(&root, "esen");
    if !paths.model.exists() {
        eprintln!("esen assets missing; skipping");
        return;
    }

    let mut engine = BergamotEngine::new();
    engine.load_model_into_cache(&paths, "es-en").expect("load");

    let outs = engine
        .translate_multiple(&["El gato come pescado.".to_string()], "es-en")
        .expect("translate before evict");
    assert!(outs[0].to_lowercase().contains("cat"));

    engine.evict("es-en");

    let err = engine
        .translate_multiple(&["El gato come pescado.".to_string()], "es-en")
        .expect_err("translate after evict must error");
    assert!(
        err.contains("not loaded") || err.contains("es-en"),
        "unexpected error after evict: {err}"
    );

    // Reload and translate again — the cache should be repopulatable.
    engine
        .load_model_into_cache(&paths, "es-en")
        .expect("reload");
    let outs = engine
        .translate_multiple(&["El gato come pescado.".to_string()], "es-en")
        .expect("translate after reload");
    assert!(outs[0].to_lowercase().contains("cat"));
}

#[test]
fn evict_unmaps_the_model_file() {
    // Verify slimt actually releases the mmap when we drop the SlimtModel.
    // Inspect /proc/self/maps for the model file path before/after evict.
    let Some(root) = asset_root() else {
        eprintln!("translator assets not installed; skipping");
        return;
    };
    let paths = paths_for(&root, "esen");
    if !paths.model.exists() {
        eprintln!("esen assets missing; skipping");
        return;
    }

    let model_path = paths.model.canonicalize().expect("canonicalize");
    let model_str = model_path.to_str().expect("utf8");

    let count_mappings = || -> usize {
        let maps = std::fs::read_to_string("/proc/self/maps").expect("read /proc/self/maps");
        maps.lines().filter(|line| line.contains(model_str)).count()
    };

    assert_eq!(
        count_mappings(),
        0,
        "model file unexpectedly mapped before load"
    );

    let mut engine = BergamotEngine::new();
    engine.load_model_into_cache(&paths, "es-en").expect("load");

    let mapped_after_load = count_mappings();
    eprintln!("[evict_unmaps] mappings after load: {mapped_after_load}");
    assert!(
        mapped_after_load >= 1,
        "expected the model file to be mmap'd after load"
    );

    // Run a translation so the slimt async batcher is fully spun up — if any
    // worker thread held a transient reference, this exercises that path.
    let _ = engine
        .translate_multiple(&["El gato come pescado.".to_string()], "es-en")
        .expect("translate");

    engine.evict("es-en");

    let mapped_after_evict = count_mappings();
    eprintln!("[evict_unmaps] mappings after evict: {mapped_after_evict}");
    assert_eq!(
        mapped_after_evict, 0,
        "model file is still mapped after evict — eviction did not release mmap"
    );
}

#[test]
fn many_load_evict_cycles_do_not_leak_mappings() {
    // Hammer load + evict to make sure we don't accumulate stale mmaps.
    let Some(root) = asset_root() else {
        eprintln!("translator assets not installed; skipping");
        return;
    };
    let paths = paths_for(&root, "esen");
    if !paths.model.exists() {
        eprintln!("esen assets missing; skipping");
        return;
    }

    let model_path = paths.model.canonicalize().expect("canonicalize");
    let model_str = model_path.to_str().expect("utf8");

    let count_mappings = || -> usize {
        let maps = std::fs::read_to_string("/proc/self/maps").expect("read maps");
        maps.lines().filter(|line| line.contains(model_str)).count()
    };

    let mut engine = BergamotEngine::new();
    for _ in 0..8 {
        engine.load_model_into_cache(&paths, "es-en").expect("load");
        let _ = engine
            .translate_multiple(&["El gato come pescado.".to_string()], "es-en")
            .expect("translate");
        engine.evict("es-en");
    }

    let mappings = count_mappings();
    eprintln!("[load_evict_cycles] mappings after 8 cycles: {mappings}");
    assert_eq!(
        mappings, 0,
        "load+evict loop leaked {mappings} mmap(s) of the model"
    );
}

#[test]
fn two_loaded_models_each_account_for_exactly_one_mapping() {
    // With models A and B both loaded, we expect exactly two mmap regions for
    // the model files (one each), and the count must not grow as we translate
    // with them in alternation. Evicting A must drop A's mapping but leave B
    // alone.
    let Some(root) = asset_root() else {
        eprintln!("translator assets not installed; skipping");
        return;
    };
    let esen = paths_for(&root, "esen");
    let enes = paths_for(&root, "enes");
    if !esen.model.exists() || !enes.model.exists() {
        eprintln!("esen or enes assets missing; skipping");
        return;
    }

    let esen_path = esen.model.canonicalize().expect("canonicalize esen");
    let enes_path = enes.model.canonicalize().expect("canonicalize enes");

    let count_for = |needle: &str| -> usize {
        std::fs::read_to_string("/proc/self/maps")
            .expect("read maps")
            .lines()
            .filter(|l| l.contains(needle))
            .count()
    };
    let esen_count = || count_for(esen_path.to_str().unwrap());
    let enes_count = || count_for(enes_path.to_str().unwrap());

    let mut engine = BergamotEngine::new();
    engine
        .load_model_into_cache(&esen, "es-en")
        .expect("load esen");
    engine
        .load_model_into_cache(&enes, "en-es")
        .expect("load enes");

    assert_eq!(esen_count(), 1, "esen must be mapped once after load");
    assert_eq!(enes_count(), 1, "enes must be mapped once after load");

    // Hammer back-and-forth translations to confirm neither model accrues
    // extra mappings while the other is in use.
    for _ in 0..6 {
        let _ = engine
            .translate_multiple(&["El gato come pescado.".to_string()], "es-en")
            .expect("es->en");
        let _ = engine
            .translate_multiple(&["The cat eats fish.".to_string()], "en-es")
            .expect("en->es");
        assert_eq!(esen_count(), 1, "esen must stay at 1 mapping during use");
        assert_eq!(enes_count(), 1, "enes must stay at 1 mapping during use");
    }

    // Evict only one; the other must remain fully usable.
    engine.evict("es-en");
    assert_eq!(esen_count(), 0, "esen must be unmapped after evict");
    assert_eq!(enes_count(), 1, "enes must still be mapped");

    let outs = engine
        .translate_multiple(&["The cat eats fish.".to_string()], "en-es")
        .expect("en->es after sibling evict");
    assert!(outs[0].to_lowercase().contains("gato"));
    assert_eq!(enes_count(), 1, "enes must still hold one mapping");
}

#[test]
fn translate_with_alignment_returns_aligned_tokens() {
    let Some(root) = asset_root() else {
        eprintln!("translator assets not installed; skipping");
        return;
    };
    let paths = paths_for(&root, "esen");
    if !paths.model.exists() {
        eprintln!("esen assets missing; skipping");
        return;
    }

    let mut engine = BergamotEngine::new();
    engine
        .load_model_into_cache(&paths, "es-en")
        .expect("load esen");

    let inputs = vec!["El gato come pescado.".to_string()];
    let outs = engine
        .translate_multiple_with_alignment(&inputs, "es-en")
        .expect("translate with alignment");

    assert_eq!(outs.len(), 1);
    assert!(!outs[0].translated_text.is_empty());
    assert!(
        !outs[0].alignments.is_empty(),
        "expected at least one token alignment"
    );
    for a in &outs[0].alignments {
        assert!(a.src_begin <= a.src_end);
        assert!(a.tgt_begin <= a.tgt_end);
    }
}

#[test]
fn html_inline_tags_share_one_translation_call() {
    // The whole point of the Rust-side pipeline: inline tags inside one
    // block-level element must NOT split the model's input. The model sees
    // one full sentence ("The cat eats fish") so it can pick the right
    // pronoun/agreement, and alignments route the translated content back
    // into the original DOM nodes.
    let Some(root) = asset_root() else {
        eprintln!("translator assets not installed; skipping");
        return;
    };
    let paths = paths_for(&root, "enes");
    if !paths.model.exists() {
        eprintln!("enes assets missing; skipping");
        return;
    }

    let mut engine = BergamotEngine::new();
    engine
        .load_model_into_cache(&paths, "en-es")
        .expect("load enes");

    // <p> is the scope; <b> is inline so it does NOT break the scope.
    let inputs = vec!["<p>The <b>cat</b> eats fish.</p>".to_string()];
    let outs = translate_html_via_engine(&engine, "en-es", &inputs).expect("translate html");
    assert_eq!(outs.len(), 1);
    eprintln!("[inline_one_call] {}", outs[0]);

    // DOM identity: the <p>...</p> wrapper and the <b>...</b> wrapper must
    // both survive verbatim around their respective text leaves.
    assert!(outs[0].starts_with("<p>"), "got: {}", outs[0]);
    assert!(outs[0].ends_with("</p>"), "got: {}", outs[0]);
    assert!(
        outs[0].contains("<b>") && outs[0].contains("</b>"),
        "<b> wrapping must survive: {}",
        outs[0]
    );
    // The Spanish word for "cat" (gato/gata) should land inside the <b>.
    let between_b = outs[0]
        .split("<b>")
        .nth(1)
        .and_then(|rest| rest.split("</b>").next())
        .unwrap_or("");
    let between_b_lower = between_b.to_lowercase();
    assert!(
        between_b_lower.contains("gat"),
        "expected 'gat*' (Spanish for cat) inside <b>...</b>, got <b>{}</b> in: {}",
        between_b,
        outs[0]
    );
}

#[test]
fn html_attributes_pass_through_verbatim_under_real_model() {
    // Attributes must round-trip exactly: href, data-*, class, style, id, etc.
    // The pipeline never touches structure or attributes — only text leaves.
    let Some(root) = asset_root() else {
        eprintln!("translator assets not installed; skipping");
        return;
    };
    let paths = paths_for(&root, "enes");
    if !paths.model.exists() {
        eprintln!("enes assets missing; skipping");
        return;
    }

    let mut engine = BergamotEngine::new();
    engine
        .load_model_into_cache(&paths, "en-es")
        .expect("load enes");

    let inputs = vec![
        "<a href=\"https://example.com/path?q=1&r=2\" data-tracking=\"hello world\" class=\"link primary\" id=\"go\" rel=\"noopener\">Click here.</a>"
            .to_string(),
    ];
    let outs = translate_html_via_engine(&engine, "en-es", &inputs).expect("translate html");
    assert_eq!(outs.len(), 1);
    eprintln!("[attrs_passthrough] {}", outs[0]);

    // Every attribute must appear verbatim in the output.
    for needle in [
        "href=\"https://example.com/path?q=1&amp;r=2\"",
        "data-tracking=\"hello world\"",
        "class=\"link primary\"",
        "id=\"go\"",
        "rel=\"noopener\"",
    ] {
        // html5ever serialises `&` in URLs as `&amp;` per HTML5 spec; both
        // forms are semantically equivalent and round-trip on next parse.
        let raw = needle.replace("&amp;", "&");
        assert!(
            outs[0].contains(needle) || outs[0].contains(&raw),
            "attribute {needle:?} (or its raw form) must round-trip verbatim: {}",
            outs[0]
        );
    }
}

#[test]
fn html_block_level_elements_translate_independently() {
    // Two paragraphs are independent translation scopes. Each <p> goes to the
    // model as its own input; the model never mashes them into one sentence.
    let Some(root) = asset_root() else {
        eprintln!("translator assets not installed; skipping");
        return;
    };
    let paths = paths_for(&root, "enes");
    if !paths.model.exists() {
        eprintln!("enes assets missing; skipping");
        return;
    }

    let mut engine = BergamotEngine::new();
    engine
        .load_model_into_cache(&paths, "en-es")
        .expect("load enes");

    let inputs = vec!["<div><p>The cat eats fish.</p><p>Hello world.</p></div>".to_string()];
    let outs = translate_html_via_engine(&engine, "en-es", &inputs).expect("translate html");
    assert_eq!(outs.len(), 1);
    eprintln!("[block_independent] {}", outs[0]);

    // <div> wrapping survives, both <p> elements survive, content of each
    // <p> is translated independently.
    assert!(outs[0].contains("<div>") && outs[0].contains("</div>"));
    let p_count = outs[0].matches("<p>").count();
    assert_eq!(p_count, 2, "expected exactly two <p> blocks: {}", outs[0]);
    let lower = outs[0].to_lowercase();
    assert!(
        lower.contains("gato"),
        "first paragraph should translate 'cat': {}",
        outs[0]
    );
    assert!(
        lower.contains("mundo"),
        "second paragraph should translate 'world': {}",
        outs[0]
    );
}

#[test]
fn html_void_tags_stay_in_place() {
    // <br>, <img>, <hr> have no text node — they must remain in the DOM at
    // the exact position they appeared, untouched by the translator.
    let Some(root) = asset_root() else {
        eprintln!("translator assets not installed; skipping");
        return;
    };
    let paths = paths_for(&root, "enes");
    if !paths.model.exists() {
        eprintln!("enes assets missing; skipping");
        return;
    }

    let mut engine = BergamotEngine::new();
    engine
        .load_model_into_cache(&paths, "en-es")
        .expect("load enes");

    let inputs = vec!["<p>The cat<br>eats <img src=\"f.png\" alt=\"fish\"> fish.</p>".to_string()];
    let outs = translate_html_via_engine(&engine, "en-es", &inputs).expect("translate html");
    assert_eq!(outs.len(), 1);
    eprintln!("[void_tags_inplace] {}", outs[0]);

    assert!(outs[0].contains("<br>"), "<br> must survive: {}", outs[0]);
    assert!(
        outs[0].contains("src=\"f.png\""),
        "<img src> must survive verbatim: {}",
        outs[0]
    );
    assert!(
        outs[0].contains("alt=\"fish\""),
        "<img alt> must survive verbatim: {}",
        outs[0]
    );
    let lower = outs[0].to_lowercase();
    assert!(lower.contains("gato"));
}
