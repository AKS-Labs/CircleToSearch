#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum SpeechChunkBoundary {
    #[default]
    None,
    Sentence,
    Paragraph,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct PhonemeChunk {
    pub content: String,
    pub boundary_after: SpeechChunkBoundary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct TtsVoiceOption {
    pub name: String,
    pub speaker_id: i64,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct PcmAudio {
    pub sample_rate: i32,
    pub pcm_samples: Vec<i16>,
}

impl PcmAudio {
    pub fn silence(sample_rate: i32, duration_ms: i32) -> Self {
        let clamped_duration_ms = duration_ms.max(1) as i64;
        let sample_count = ((sample_rate as i64) * clamped_duration_ms / 1000).max(1) as usize;
        Self {
            sample_rate,
            pcm_samples: vec![0; sample_count],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct SpeechChunk {
    pub content: String,
    pub is_phonemes: bool,
    pub pause_after_ms: Option<i32>,
}

/// Whether a voice speaks URLs and hashtags or passes over them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum UrlsAndHashtags {
    Read,
    Skip,
}

const CLAUSE_PAUSE_MS: i32 = 0;
const SENTENCE_PAUSE_MS: i32 = 0;
const PARAGRAPH_PAUSE_MS: i32 = 0;
const MIN_PAUSE_SPLIT_CHARS: usize = 12;
const MAX_DIRECT_PHONEME_CHARS: usize = 100;
const SENTENCE_BOUNDARY_CHARS: [char; 6] = ['.', '?', '!', '。', '？', '！'];
const CLAUSE_PAUSE_CHARS: [char; 8] = [',', ';', ':', '、', '，', '；', '：', '،'];

pub fn plan_speech_chunks<F>(text: &str, mut phonemize_text: F) -> Result<Vec<SpeechChunk>, String>
where
    F: FnMut(&str) -> Result<Vec<PhonemeChunk>, String>,
{
    let mut chunks = plan_speech_chunks_internal(text, &mut phonemize_text)?;
    if let Some(last_chunk) = chunks.last_mut() {
        last_chunk.pause_after_ms = None;
    }
    Ok(chunks)
}

fn plan_speech_chunks_internal<F>(
    text: &str,
    phonemize_text: &mut F,
) -> Result<Vec<SpeechChunk>, String>
where
    F: FnMut(&str) -> Result<Vec<PhonemeChunk>, String>,
{
    let source_chunks = split_text_into_speech_chunks(text);
    if source_chunks.len() > 1 {
        let mut requests = Vec::new();
        for (source_chunk, boundary) in source_chunks {
            let mut recursive = plan_speech_chunks_internal(&source_chunk, phonemize_text)?;
            if let Some(last) = recursive.last_mut() {
                last.pause_after_ms = boundary_pause_ms(boundary);
            }
            requests.extend(recursive);
        }
        return Ok(requests);
    }

    let phoneme_chunks = phonemize_text(text)?
        .into_iter()
        .filter(|chunk| !chunk.content.trim().is_empty())
        .collect::<Vec<_>>();
    if phoneme_chunks.is_empty() {
        return Ok(Vec::new());
    }

    if phoneme_chunks.len() > 1 {
        return Ok(phoneme_chunks
            .into_iter()
            .map(|chunk| SpeechChunk {
                content: chunk.content,
                is_phonemes: true,
                pause_after_ms: boundary_pause_ms(chunk.boundary_after),
            })
            .collect());
    }

    if let Some(split_requests) =
        build_split_chunk_requests(text, &phoneme_chunks[0], phonemize_text)?
    {
        return Ok(split_requests);
    }

    Ok(vec![SpeechChunk {
        content: text.to_owned(),
        is_phonemes: false,
        pause_after_ms: None,
    }])
}

fn build_split_chunk_requests<F>(
    source_chunk: &str,
    phoneme_chunk: &PhonemeChunk,
    phonemize_text: &mut F,
) -> Result<Option<Vec<SpeechChunk>>, String>
where
    F: FnMut(&str) -> Result<Vec<PhonemeChunk>, String>,
{
    if phoneme_chunk.content.chars().count() <= MAX_DIRECT_PHONEME_CHARS {
        return Ok(None);
    }

    let Some((first, second)) = split_at_best_pause(source_chunk) else {
        return Ok(None);
    };
    let remaining_requests = plan_speech_chunks_internal(&second, phonemize_text)?;
    if remaining_requests.is_empty() {
        return Ok(None);
    }

    let mut requests = Vec::with_capacity(1 + remaining_requests.len());
    requests.push(SpeechChunk {
        content: first,
        is_phonemes: false,
        pause_after_ms: Some(CLAUSE_PAUSE_MS),
    });
    requests.extend(remaining_requests);
    Ok(Some(requests))
}

fn split_text_into_speech_chunks(text: &str) -> Vec<(String, SpeechChunkBoundary)> {
    let paragraphs = split_into_paragraphs(text);
    let paragraph_count = paragraphs.len();
    let mut out = Vec::new();
    for (paragraph_index, paragraph) in paragraphs.into_iter().enumerate() {
        let is_last_paragraph = paragraph_index + 1 == paragraph_count;
        let sentences = split_paragraph_into_sentenceish_segments(&paragraph);
        let sentence_count = sentences.len();
        for (sentence_index, sentence) in sentences.into_iter().enumerate() {
            let is_last_sentence = sentence_index + 1 == sentence_count;
            let boundary = match (is_last_sentence, is_last_paragraph) {
                (false, _) => SpeechChunkBoundary::Sentence,
                (true, false) => SpeechChunkBoundary::Paragraph,
                (true, true) => SpeechChunkBoundary::None,
            };
            out.push((sentence, boundary));
        }
    }
    out
}

fn split_into_paragraphs(text: &str) -> Vec<String> {
    let mut paragraphs = Vec::new();
    let mut current = String::new();
    let mut previous_line_ended_sentence = false;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !current.is_empty() {
                paragraphs.push(std::mem::take(&mut current));
            }
            previous_line_ended_sentence = false;
            continue;
        }

        if !current.is_empty() && previous_line_ended_sentence {
            paragraphs.push(std::mem::take(&mut current));
        } else if !current.is_empty() {
            current.push(' ');
        }

        current.push_str(trimmed);
        previous_line_ended_sentence = trimmed
            .chars()
            .last()
            .map(is_sentence_boundary_char)
            .unwrap_or(false);
    }

    if !current.is_empty() {
        paragraphs.push(current);
    }

    paragraphs
}

fn split_paragraph_into_sentenceish_segments(paragraph: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let chars = paragraph.chars().collect::<Vec<_>>();
    let mut current = String::new();
    let mut index = 0usize;

    while index < chars.len() {
        let ch = chars[index];
        current.push(ch);
        index += 1;

        if is_sentence_boundary_char(ch) {
            while index < chars.len() && is_sentence_boundary_char(chars[index]) {
                current.push(chars[index]);
                index += 1;
            }

            let is_boundary = index == chars.len() || chars[index].is_whitespace();
            if is_boundary {
                let segment = current.trim();
                if !segment.is_empty() {
                    segments.push(segment.to_owned());
                }
                current.clear();
                while index < chars.len() && chars[index].is_whitespace() {
                    index += 1;
                }
            }
        }
    }

    let tail = current.trim();
    if !tail.is_empty() {
        segments.push(tail.to_owned());
    }

    if segments.is_empty() {
        let trimmed = paragraph.trim();
        if !trimmed.is_empty() {
            segments.push(trimmed.to_owned());
        }
    }

    segments
}

fn split_at_best_pause(text: &str) -> Option<(String, String)> {
    let chars = text.chars().collect::<Vec<_>>();
    let min_side_chars = MIN_PAUSE_SPLIT_CHARS.max(chars.len() / 4);
    let midpoint = chars.len() as f64 / 2.0;

    chars
        .iter()
        .enumerate()
        .filter(|(_, ch)| CLAUSE_PAUSE_CHARS.contains(ch))
        .filter_map(|(split_index, _)| {
            let first = chars[..=split_index].iter().collect::<String>();
            let second = chars[split_index + 1..].iter().collect::<String>();
            let first = first.trim().to_owned();
            let second = second.trim().to_owned();
            if first.chars().count() < min_side_chars || second.chars().count() < min_side_chars {
                return None;
            }
            let balance_penalty =
                (first.chars().count() as i32 - second.chars().count() as i32).abs();
            let midpoint_penalty = (midpoint - split_index as f64).abs();
            Some((balance_penalty, midpoint_penalty, (first, second)))
        })
        .min_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| left.1.total_cmp(&right.1))
        })
        .map(|(_, _, split)| split)
}

fn is_sentence_boundary_char(ch: char) -> bool {
    SENTENCE_BOUNDARY_CHARS.contains(&ch)
}

fn boundary_pause_ms(boundary: SpeechChunkBoundary) -> Option<i32> {
    match boundary {
        SpeechChunkBoundary::None => None,
        SpeechChunkBoundary::Sentence => Some(SENTENCE_PAUSE_MS),
        SpeechChunkBoundary::Paragraph => Some(PARAGRAPH_PAUSE_MS),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_sentences_and_clears_final_pause() {
        let chunks = plan_speech_chunks("Hello. World!", |_| {
            Ok(vec![PhonemeChunk {
                content: "abc".to_owned(),
                boundary_after: SpeechChunkBoundary::Paragraph,
            }])
        })
        .unwrap();

        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].content, "Hello.");
        assert_eq!(chunks[0].pause_after_ms, Some(SENTENCE_PAUSE_MS));
        assert_eq!(chunks[1].content, "World!");
        assert_eq!(chunks[1].pause_after_ms, None);
    }

    #[test]
    fn paragraphs_use_paragraph_pause() {
        let chunks = plan_speech_chunks("Hello.\n\nWorld!", |_| {
            Ok(vec![PhonemeChunk {
                content: "abc".to_owned(),
                boundary_after: SpeechChunkBoundary::Paragraph,
            }])
        })
        .unwrap();

        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].pause_after_ms, Some(PARAGRAPH_PAUSE_MS));
        assert_eq!(chunks[1].pause_after_ms, None);
    }

    #[test]
    fn forwards_multi_chunk_phonemizer_boundaries() {
        let chunks = plan_speech_chunks("Hello world", |_| {
            Ok(vec![
                PhonemeChunk {
                    content: "a".to_owned(),
                    boundary_after: SpeechChunkBoundary::Sentence,
                },
                PhonemeChunk {
                    content: "b".to_owned(),
                    boundary_after: SpeechChunkBoundary::Paragraph,
                },
            ])
        })
        .unwrap();

        assert_eq!(chunks.len(), 2);
        assert!(chunks.iter().all(|chunk| chunk.is_phonemes));
        assert_eq!(chunks[0].pause_after_ms, Some(SENTENCE_PAUSE_MS));
        assert_eq!(chunks[1].pause_after_ms, None);
    }

    #[test]
    fn long_phoneme_chunk_splits_at_clause_pause() {
        let text = "This is a long clause, and this is the rest of the sentence for splitting";
        let chunks = plan_speech_chunks(text, |_| {
            Ok(vec![PhonemeChunk {
                content: "x".repeat(MAX_DIRECT_PHONEME_CHARS + 1),
                boundary_after: SpeechChunkBoundary::Paragraph,
            }])
        })
        .unwrap();

        assert_eq!(chunks.len(), 2);
        assert!(!chunks[0].is_phonemes);
        assert_eq!(chunks[0].pause_after_ms, Some(CLAUSE_PAUSE_MS));
        assert_eq!(chunks[1].pause_after_ms, None);
    }
}
