//! whisper-cli sidecar (ported from CaptionSmith's transcribe.rs).
//!
//! Flags from `whisper-cli --help` (v1.9.1):
//!   -m <model> -f <wav> -of <base> -oj|-ojf -pp -t N -l LANG [-tr] [-ng]
//! whisper writes `<base>.json` and reports `progress = N%` on stderr. JSON:
//!   { "transcription": [ { "offsets": {"from":ms,"to":ms}, "text": "...",
//!                          "tokens": [ {"text":" And","offsets":{...}} ] } ] }
//! Token text has a leading space at word starts; specials look like [_BEG_].
//! Times here are integer milliseconds, like the project model.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Word {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub words: Option<Vec<Word>>,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum WhisperError {
    #[error("transcription cancelled")]
    Cancelled,
    /// Died before any progress (bad binary, missing runtime, bad model).
    #[error("whisper failed to start: {0}")]
    Startup(String),
    #[error("whisper failed: {0}")]
    Failed(String),
}

#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Whisper language code; `None` = "en".
    pub language: Option<String>,
    pub translate: bool,
    /// Per-word timings (`-ojf`), used to split long segments into short cues.
    pub words: bool,
    /// Pass `-ng` (CPU only).
    pub no_gpu: bool,
    pub threads: Option<usize>,
}

pub fn threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(8)
}

/// `-of <base>` makes whisper write `<base>.json` (it appends).
pub fn json_path(out_base: &Path) -> PathBuf {
    let mut s = out_base.as_os_str().to_owned();
    s.push(".json");
    PathBuf::from(s)
}

pub fn whisper_args(model: &Path, wav: &Path, out_base: &Path, o: &Options) -> Vec<String> {
    let p = |x: &Path| x.to_string_lossy().into_owned();
    let mut a = vec![
        "-m".into(),
        p(model),
        "-f".into(),
        p(wav),
        "-of".into(),
        p(out_base),
        "-pp".into(),
        "-t".into(),
        o.threads.unwrap_or_else(threads).to_string(),
        "-l".into(),
        o.language.clone().unwrap_or_else(|| "en".into()),
        if o.words { "-ojf" } else { "-oj" }.into(),
    ];
    if o.translate {
        a.push("-tr".into());
    }
    if o.no_gpu {
        a.push("-ng".into());
    }
    a
}

/// ffmpeg args to cut `[in_ms, in_ms+dur_ms)` of `src` to 16 kHz mono PCM WAV.
pub fn extract_wav_args(src: &str, in_ms: u64, dur_ms: u64, out: &Path) -> Vec<String> {
    let s = |ms: u64| format!("{:.3}", ms as f64 / 1000.0);
    vec![
        "-y".into(),
        "-hide_banner".into(),
        "-ss".into(),
        s(in_ms),
        "-t".into(),
        s(dur_ms),
        "-i".into(),
        src.into(),
        "-vn".into(),
        "-ac".into(),
        "1".into(),
        "-ar".into(),
        "16000".into(),
        "-c:a".into(),
        "pcm_s16le".into(),
        out.to_string_lossy().into_owned(),
    ]
}

/// The integer percent from a stderr line like `... progress =  45%`.
pub fn parse_progress(line: &str) -> Option<f64> {
    let idx = line.find("progress =")?;
    let rest = line[idx + "progress =".len()..].trim_start();
    let pct = rest.find('%')?;
    rest[..pct].trim().parse::<f64>().ok()
}

fn ms_at(v: &serde_json::Value, ptr: &str) -> u64 {
    v.pointer(ptr)
        .and_then(|x| x.as_f64())
        .unwrap_or(0.0)
        .max(0.0) as u64
}

/// Rebuild words from sub-word tokens: a leading space starts a word,
/// continuation tokens and punctuation fold in, specials are skipped.
pub fn words_from_tokens(tokens: &[serde_json::Value]) -> Option<Vec<Word>> {
    let mut words: Vec<Word> = Vec::new();
    for tok in tokens {
        let raw = tok.get("text").and_then(|t| t.as_str()).unwrap_or("");
        if raw.starts_with("[_") {
            continue;
        }
        let clean = raw.trim();
        if clean.is_empty() {
            continue;
        }
        let (from, to) = (ms_at(tok, "/offsets/from"), ms_at(tok, "/offsets/to"));
        if raw.starts_with(' ') || words.is_empty() {
            words.push(Word {
                start_ms: from,
                end_ms: to,
                text: clean.into(),
            });
        } else if let Some(last) = words.last_mut() {
            last.text.push_str(clean);
            last.end_ms = to;
        }
    }
    (!words.is_empty()).then_some(words)
}

pub fn parse_segments(json: &str, want_words: bool) -> Result<Vec<Segment>, String> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("parse whisper json: {e}"))?;
    let arr = v
        .get("transcription")
        .and_then(|t| t.as_array())
        .ok_or("whisper json missing transcription array")?;
    Ok(arr
        .iter()
        .filter_map(|seg| {
            let text = seg.get("text")?.as_str()?.trim().to_string();
            if text.is_empty() {
                return None;
            }
            let words = if want_words {
                seg.get("tokens")
                    .and_then(|t| t.as_array())
                    .and_then(|t| words_from_tokens(t))
            } else {
                None
            };
            Some(Segment {
                start_ms: ms_at(seg, "/offsets/from"),
                end_ms: ms_at(seg, "/offsets/to"),
                text,
                words,
            })
        })
        .collect())
}

/// Non-zero exit -> error. Detail = last stderr line mentioning an error.
/// No progress seen means it died on startup.
pub fn classify_failure(tail: &[String], progress_seen: bool) -> WhisperError {
    let detail = tail
        .iter()
        .rev()
        .find(|l| l.to_lowercase().contains("error") || l.contains("failed"))
        .map_or("whisper failed to transcribe the audio.", String::as_str)
        .trim()
        .to_string();
    if progress_seen {
        WhisperError::Failed(detail)
    } else {
        WhisperError::Startup(detail)
    }
}

/// Run whisper on `wav`, streaming `progress(0..=100)` and polling `cancel`
/// (the child is killed on cancel). The JSON output is read and removed.
pub fn transcribe(
    whisper: &Path,
    model: &Path,
    wav: &Path,
    opts: &Options,
    cancel: &AtomicBool,
    mut progress: impl FnMut(f64),
) -> Result<Vec<Segment>, WhisperError> {
    let out_base = wav.with_extension("out");
    let json = json_path(&out_base);
    let cleanup = || {
        let _ = std::fs::remove_file(&json);
    };
    let mut cmd = Command::new(whisper);
    cmd.args(whisper_args(model, wav, &out_base, opts))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| WhisperError::Startup(format!("{}: {e}", whisper.display())))?;
    let stderr = child.stderr.take().expect("piped stderr");
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let mut tail: Vec<String> = Vec::new();
    let mut seen = false;
    let mut handle = |line: String, tail: &mut Vec<String>, seen: &mut bool| {
        if let Some(p) = parse_progress(&line) {
            *seen = true;
            progress(p);
        }
        tail.push(line);
        if tail.len() > 20 {
            tail.remove(0);
        }
    };
    let status = loop {
        while let Ok(line) = rx.try_recv() {
            handle(line, &mut tail, &mut seen);
        }
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            cleanup();
            return Err(WhisperError::Cancelled);
        }
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => {
                cleanup();
                return Err(WhisperError::Failed(format!("wait: {e}")));
            }
        }
    };
    let _ = reader.join();
    for line in rx.try_iter() {
        handle(line, &mut tail, &mut seen);
    }
    if !status.success() {
        cleanup();
        return Err(classify_failure(&tail, seen));
    }
    let text = std::fs::read_to_string(&json)
        .map_err(|e| WhisperError::Failed(format!("whisper produced no JSON output: {e}")));
    cleanup();
    let segs = parse_segments(&text?, opts.words).map_err(WhisperError::Failed)?;
    progress(100.0);
    Ok(segs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_progress_lines() {
        assert_eq!(
            parse_progress("whisper_print_progress_callback: progress =  45%"),
            Some(45.0)
        );
        assert_eq!(parse_progress("progress = 100%"), Some(100.0));
        for bad in ["no progress here", "progress = abc%", "progress = 12"] {
            assert_eq!(parse_progress(bad), None, "{bad}");
        }
    }

    #[test]
    fn parses_segments_and_skips_empty() {
        let json = r#"{"transcription":[
            {"offsets":{"from":0,"to":11000},"text":" Hello world."},
            {"offsets":{"from":11000,"to":12000},"text":"  "},
            {"offsets":{"from":12000,"to":13000},"text":" Next line."}
        ]}"#;
        let s = parse_segments(json, false).unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(
            (s[0].start_ms, s[0].end_ms, s[0].text.as_str()),
            (0, 11000, "Hello world.")
        );
        assert!(s[0].words.is_none());
    }

    #[test]
    fn reconstructs_words_from_tokens() {
        let json = r#"{"transcription":[{"offsets":{"from":0,"to":1000},"text":" And Americans.","tokens":[
            {"text":"[_BEG_]","offsets":{"from":0,"to":0}},
            {"text":" And","offsets":{"from":320,"to":370}},
            {"text":" Americ","offsets":{"from":500,"to":700}},
            {"text":"ans","offsets":{"from":700,"to":820}},
            {"text":".","offsets":{"from":820,"to":840}}
        ]}]}"#;
        let w = parse_segments(json, true).unwrap()[0]
            .words
            .clone()
            .unwrap();
        assert_eq!(w.len(), 2);
        assert_eq!((w[0].text.as_str(), w[0].start_ms), ("And", 320));
        assert_eq!(
            (w[1].text.as_str(), w[1].start_ms, w[1].end_ms),
            ("Americans.", 500, 840)
        );
    }

    #[test]
    fn malformed_json_is_an_error() {
        assert!(parse_segments("{not json", false)
            .unwrap_err()
            .contains("parse"));
        assert!(parse_segments(r#"{"result":{}}"#, false)
            .unwrap_err()
            .contains("missing transcription"));
        assert!(parse_segments(r#"{"transcription":[]}"#, true)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn classifies_failures() {
        let tail = vec![
            "loading".to_string(),
            "ggml: Error: no device found  ".to_string(),
        ];
        assert_eq!(
            classify_failure(&tail, false),
            WhisperError::Startup("ggml: Error: no device found".into())
        );
        assert_eq!(
            classify_failure(&["progress = 40%".into()], true),
            WhisperError::Failed("whisper failed to transcribe the audio.".into())
        );
    }

    #[test]
    fn builds_args() {
        let o = Options {
            language: Some("fr".into()),
            translate: true,
            words: true,
            no_gpu: true,
            threads: Some(4),
        };
        let a = whisper_args(
            Path::new("m.bin"),
            Path::new("a.wav"),
            Path::new("a.out"),
            &o,
        );
        assert_eq!(
            a.join(" "),
            "-m m.bin -f a.wav -of a.out -pp -t 4 -l fr -ojf -tr -ng"
        );
        let a = whisper_args(
            Path::new("m"),
            Path::new("w"),
            Path::new("o"),
            &Options::default(),
        );
        assert!(a.join(" ").ends_with("-l en -oj"));
        assert_eq!(
            json_path(Path::new("x/a.out")),
            PathBuf::from("x/a.out.json")
        );
        let e = extract_wav_args("in.mp4", 1500, 2000, Path::new("o.wav"));
        assert_eq!(
            e.join(" "),
            "-y -hide_banner -ss 1.500 -t 2.000 -i in.mp4 -vn -ac 1 -ar 16000 -c:a pcm_s16le o.wav"
        );
    }

    /// A fake whisper-cli (shell script) exercises spawn, progress, JSON
    /// pickup/cleanup, failure classification and cancel.
    #[cfg(unix)]
    #[test]
    fn runs_a_fake_whisper() {
        use std::os::unix::fs::PermissionsExt;
        let d = std::env::temp_dir().join(format!("ms-fake-whisper-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let script = |name: &str, body: &str| {
            let p = d.join(name);
            std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
            p
        };
        // $6 is the -of base.
        let ok = script(
            "ok",
            r#"echo "progress = 50%" >&2
printf '{"transcription":[{"offsets":{"from":100,"to":900},"text":" hi there"}]}' > "$6.json""#,
        );
        let wav = d.join("a.wav");
        let cancel = AtomicBool::new(false);
        let mut seen = vec![];
        let s = transcribe(
            &ok,
            Path::new("m"),
            &wav,
            &Options::default(),
            &cancel,
            |p| seen.push(p),
        )
        .unwrap();
        assert_eq!(s[0].text, "hi there");
        assert_eq!(seen, vec![50.0, 100.0]);
        assert!(
            !json_path(&wav.with_extension("out")).exists(),
            "json cleaned up"
        );

        let bad = script("bad", r#"echo "error: failed to load model" >&2; exit 3"#);
        assert_eq!(
            transcribe(
                &bad,
                Path::new("m"),
                &wav,
                &Options::default(),
                &cancel,
                |_| {}
            ),
            Err(WhisperError::Startup("error: failed to load model".into()))
        );

        let slow = script("slow", "sleep 5");
        let cancel = AtomicBool::new(true);
        let t = std::time::Instant::now();
        assert_eq!(
            transcribe(
                &slow,
                Path::new("m"),
                &wav,
                &Options::default(),
                &cancel,
                |_| {}
            ),
            Err(WhisperError::Cancelled)
        );
        assert!(t.elapsed() < Duration::from_secs(3));
        assert!(matches!(
            transcribe(
                &d.join("missing"),
                Path::new("m"),
                &wav,
                &Options::default(),
                &AtomicBool::new(false),
                |_| {}
            ),
            Err(WhisperError::Startup(_))
        ));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Real whisper on real speech. Opt-in: MS_WHISPER (binary),
    /// MS_WHISPER_MODEL (ggml file) and MS_WHISPER_WAV (16 kHz speech WAV),
    /// optional MS_WHISPER_EXPECT (a word that must appear).
    #[test]
    fn real_whisper_when_available() {
        let (Ok(bin), Ok(model), Ok(wav)) = (
            std::env::var("MS_WHISPER"),
            std::env::var("MS_WHISPER_MODEL"),
            std::env::var("MS_WHISPER_WAV"),
        ) else {
            return;
        };
        let o = Options {
            words: true,
            ..Default::default()
        };
        let mut last = 0.0;
        let s = transcribe(
            Path::new(&bin),
            Path::new(&model),
            Path::new(&wav),
            &o,
            &AtomicBool::new(false),
            |p| last = p,
        )
        .unwrap();
        assert!(!s.is_empty());
        assert_eq!(last, 100.0);
        assert!(s.iter().all(|x| x.words.is_some()));
        if let Ok(word) = std::env::var("MS_WHISPER_EXPECT") {
            let all: String = s
                .iter()
                .map(|x| x.text.to_lowercase())
                .collect::<Vec<_>>()
                .join(" ");
            assert!(all.contains(&word.to_lowercase()), "{all}");
        }
    }
}
