//! Map whisper segments (relative to an extracted clip range) onto the
//! timeline as short caption cues.

use crate::whisper::{Segment, Word};

/// A caption cue in timeline milliseconds.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize)]
pub struct Cue {
    pub start_ms: u64,
    pub duration_ms: u64,
    pub text: String,
    /// Word timings in timeline milliseconds (empty without word output).
    pub words: Vec<Word>,
    /// Diarized speaker of the source segment.
    pub speaker: Option<u32>,
}

/// Split a segment into cues of at most `max_words` words when word timings
/// are available; otherwise keep it whole.
fn split(seg: &Segment, max_words: usize) -> Vec<(u64, u64, String, Vec<Word>)> {
    match &seg.words {
        Some(words) if max_words > 0 && words.len() > max_words => words
            .chunks(max_words)
            .map(|c| {
                let text = c
                    .iter()
                    .map(|w| w.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                (c[0].start_ms, c[c.len() - 1].end_ms, text, c.to_vec())
            })
            .collect(),
        _ => vec![(
            seg.start_ms,
            seg.end_ms,
            seg.text.clone(),
            seg.words.clone().unwrap_or_default(),
        )],
    }
}

/// Place `segments` of a clip that starts at `clip_start_ms` on the timeline
/// and lasts `clip_dur_ms`. Cues are clipped to the clip, made non-overlapping
/// (each starts no earlier than the previous end) and given a minimum length.
pub fn place_segments(
    clip_start_ms: u64,
    clip_dur_ms: u64,
    segments: &[Segment],
    max_words: usize,
) -> Vec<Cue> {
    const MIN_MS: u64 = 200;
    let mut out: Vec<Cue> = Vec::new();
    let mut cursor = 0u64;
    for seg in segments {
        for (s, e, text, words) in split(seg, max_words) {
            let s = s.max(cursor).min(clip_dur_ms);
            let e = e.min(clip_dur_ms).max(s);
            if text.trim().is_empty() || e - s < MIN_MS / 2 {
                continue;
            }
            let e = (s + MIN_MS).max(e).min(clip_dur_ms);
            let words = words
                .into_iter()
                .filter(|w| w.end_ms > s && w.start_ms < e)
                .map(|w| Word {
                    start_ms: clip_start_ms + w.start_ms.max(s),
                    end_ms: clip_start_ms + w.end_ms.min(e),
                    text: w.text,
                })
                .collect();
            out.push(Cue {
                start_ms: clip_start_ms + s,
                duration_ms: e - s,
                text: text.trim().into(),
                words,
                speaker: seg.speaker,
            });
            cursor = e;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::whisper::Word;

    fn seg(s: u64, e: u64, t: &str) -> Segment {
        Segment {
            start_ms: s,
            end_ms: e,
            text: t.into(),
            words: None,
            speaker: None,
        }
    }

    #[test]
    fn offsets_clamps_and_dedupes_overlap() {
        let c = place_segments(
            5000,
            3000,
            &[
                seg(0, 1200, "one"),
                seg(1000, 2000, "two"),
                seg(2900, 4000, "three"),
                seg(3500, 3600, "gone"),
            ],
            0,
        );
        assert_eq!(
            c,
            vec![
                Cue {
                    start_ms: 5000,
                    duration_ms: 1200,
                    text: "one".into(),
                    ..Default::default()
                },
                Cue {
                    start_ms: 6200,
                    duration_ms: 800,
                    text: "two".into(),
                    ..Default::default()
                },
                Cue {
                    start_ms: 7900,
                    duration_ms: 100,
                    text: "three".into(),
                    ..Default::default()
                },
            ]
        );
    }

    #[test]
    fn splits_long_segments_by_words() {
        let w = |s, e, t: &str| Word {
            start_ms: s,
            end_ms: e,
            text: t.into(),
        };
        let s = Segment {
            start_ms: 0,
            end_ms: 2000,
            text: "a b c d e".into(),
            words: Some(vec![
                w(0, 300, "a"),
                w(300, 600, "b"),
                w(600, 900, "c"),
                w(1000, 1400, "d"),
                w(1400, 2000, "e"),
            ]),
            speaker: Some(2),
        };
        let c = place_segments(0, 10_000, &[s], 2);
        let t: Vec<_> = c
            .iter()
            .map(|c| (c.start_ms, c.duration_ms, c.text.as_str()))
            .collect();
        assert_eq!(
            t,
            vec![(0, 600, "a b"), (600, 800, "c d"), (1400, 600, "e")]
        );
    }

    #[test]
    fn cues_carry_offset_words_and_speaker() {
        let w = |s, e, t: &str| Word {
            start_ms: s,
            end_ms: e,
            text: t.into(),
        };
        let s = Segment {
            start_ms: 0,
            end_ms: 900,
            text: "a b".into(),
            words: Some(vec![w(100, 400, "a"), w(500, 900, "b")]),
            speaker: Some(1),
        };
        let c = place_segments(5000, 800, &[s], 6);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].speaker, Some(1));
        assert_eq!(c[0].words, vec![w(5100, 5400, "a"), w(5500, 5800, "b")]);
    }
}
