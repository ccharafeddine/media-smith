//! Build an ASS subtitle script from a project's caption and text layers so
//! ffmpeg's `subtitles` filter (libass) can burn them in with their style.
//! Every item becomes one Dialogue event with inline overrides for position,
//! font, size, colour and stroke; times are timeline times.

use ms_project::{CaptionWord, ItemKind, Project, TextAnim, TextStyle, TrackKind};

/// `#rrggbb` → ASS `&H00BBGGRR` (alpha 00 = opaque). Bad input → white.
pub fn ass_color(hex: &str) -> String {
    let h = hex.trim().trim_start_matches('#');
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return "&H00FFFFFF".into();
    }
    let (r, g, b) = (&h[0..2], &h[2..4], &h[4..6]);
    format!("&H00{b}{g}{r}").to_uppercase()
}

/// Milliseconds → ASS `h:mm:ss.cc`.
pub fn ass_time(ms: u64) -> String {
    let cs = ms / 10;
    format!(
        "{}:{:02}:{:02}.{:02}",
        cs / 360_000,
        (cs / 6000) % 60,
        (cs / 100) % 60,
        cs % 100
    )
}

/// Make user text safe for a Dialogue line: braces would start override
/// blocks, backslashes would start tags, newlines become `\N`.
pub fn ass_escape(text: &str) -> String {
    text.replace('\\', "\u{29F5}")
        .replace('{', "(")
        .replace('}', ")")
        .replace("\r\n", "\n")
        .replace('\n', "\\N")
}

fn overrides(style: &TextStyle, scale: f64, (w, h): (u32, u32), karaoke: bool) -> String {
    let x = (style.x.clamp(0.0, 1.0) as f64 * w as f64).round();
    let y = (style.y.clamp(0.0, 1.0) as f64 * h as f64).round();
    let fs = (style.size_px as f64 * scale).round().max(1.0);
    let font = style.font.replace(['{', '}', '\\'], "");
    let mut o = match (&style.highlight, karaoke) {
        // \k sweeps each word from SecondaryColour (\2c) to PrimaryColour (\1c).
        (Some(hl), true) => format!(
            "{{\\an5\\pos({x},{y})\\fn{font}\\fs{fs}\\1c{}\\2c{}",
            ass_color(hl),
            ass_color(&style.color)
        ),
        _ => format!(
            "{{\\an5\\pos({x},{y})\\fn{font}\\fs{fs}\\1c{}",
            ass_color(&style.color)
        ),
    };
    match &style.stroke {
        Some(c) => o.push_str(&format!(
            "\\3c{}\\bord{}",
            ass_color(c),
            (fs / 16.0).round().max(1.0)
        )),
        None => o.push_str("\\bord0"),
    }
    o.push_str("\\shad0");
    match style.anim {
        TextAnim::None => {}
        TextAnim::Fade => o.push_str("\\fad(250,250)"),
        TextAnim::Pop => o.push_str("\\fscx70\\fscy70\\t(0,180,\\fscx100\\fscy100)"),
        TextAnim::SlideUp => {
            let dy = (h as f64 * 0.04).round();
            o = o.replacen(
                &format!("\\pos({x},{y})"),
                &format!("\\move({x},{},{x},{y},0,250)", y + dy),
                1,
            );
            o.push_str("\\fad(200,0)");
        }
    }
    o.push('}');
    o
}

/// `\\fad` for an item's edge fades (empty when it has none).
fn item_fade(fin: u64, fout: u64) -> String {
    if fin == 0 && fout == 0 {
        String::new()
    } else {
        format!("{{\\fad({fin},{fout})}}")
    }
}

/// Karaoke body: `{\\kN}` per word in centiseconds. Each word's duration
/// runs to the next word's start so pauses keep the previous word lit; a
/// leading empty `\\k` covers the silence before the first word.
pub fn karaoke_text(words: &[CaptionWord]) -> String {
    let cs = |ms: u64| ms.div_ceil(10).max(1);
    let mut out = String::new();
    if let Some(first) = words.first() {
        if first.start_ms >= 10 {
            out.push_str(&format!("{{\\k{}}}", first.start_ms / 10));
        }
    }
    for (i, w) in words.iter().enumerate() {
        let until = words
            .get(i + 1)
            .map_or(w.end_ms, |n| n.start_ms.max(w.start_ms));
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&format!(
            "{{\\k{}}}{}",
            cs(until.saturating_sub(w.start_ms)),
            ass_escape(&w.text)
        ));
    }
    out
}

/// Render `project`'s caption and text tracks for an output of `size`.
/// Font sizes are authored against the project canvas height and scaled.
pub fn build_ass(project: &Project, size: (u32, u32)) -> String {
    let (w, h) = size;
    let scale = h as f64 / project.canvas.height.max(1) as f64;
    let mut out = format!(
        "[Script Info]\nScriptType: v4.00+\nPlayResX: {w}\nPlayResY: {h}\nWrapStyle: 0\nScaledBorderAndShadow: yes\n\n\
[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n\
Style: Default,Arial,48,&H00FFFFFF,&H00FFFFFF,&H00000000,&H00000000,-1,0,0,0,100,100,0,0,1,2,0,5,20,20,20,1\n\n\
[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n"
    );
    // Text layers under captions (higher Layer draws on top).
    for (layer, kind) in [(0, TrackKind::Text), (1, TrackKind::Caption)] {
        for t in project.tracks.iter().filter(|t| t.kind == kind && !t.muted) {
            for it in &t.items {
                let (text, style, words): (&String, &TextStyle, &[CaptionWord]) = match &it.kind {
                    ItemKind::Text { text, style } => (text, style, &[]),
                    ItemKind::Caption {
                        text, style, words, ..
                    } => (text, style, words),
                    _ => continue,
                };
                let karaoke = style.highlight.is_some() && !words.is_empty();
                let body = if karaoke {
                    karaoke_text(words)
                } else {
                    ass_escape(text)
                };
                let (fin, fout) = it.fades();
                out.push_str(&format!(
                    "Dialogue: {layer},{},{},Default,,0,0,0,,{}{}{}\n",
                    ass_time(it.start_ms),
                    ass_time(it.end_ms()),
                    overrides(style, scale, size, karaoke),
                    item_fade(fin, fout),
                    body
                ));
            }
        }
    }
    out
}

/// True when the project has anything for [`build_ass`] to draw.
pub fn has_overlays(project: &Project) -> bool {
    project.tracks.iter().any(|t| {
        matches!(t.kind, TrackKind::Text | TrackKind::Caption) && !t.muted && !t.items.is_empty()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_are_bgr() {
        assert_eq!(ass_color("#ff8000"), "&H000080FF");
        assert_eq!(ass_color("a974ff"), "&H00FF74A9");
        assert_eq!(ass_color("nope"), "&H00FFFFFF");
    }

    #[test]
    fn times_and_escaping() {
        assert_eq!(ass_time(0), "0:00:00.00");
        assert_eq!(ass_time(3_723_450), "1:02:03.45");
        assert_eq!(ass_escape("a {b}\\c\nd"), "a (b)\u{29F5}c\\Nd");
    }

    #[test]
    fn builds_events_with_position_and_scaled_size() {
        let mut p = Project::new("t", 1080, 1920, 30);
        let c = p.add_track(TrackKind::Caption, "C");
        let x = p.add_track(TrackKind::Text, "T");
        p.add_item(
            &c,
            1000,
            1500,
            ItemKind::Caption {
                text: "hi {there}".into(),
                style: TextStyle::default(),
                words: vec![],
                speaker: None,
            },
        )
        .unwrap();
        p.add_item(
            &x,
            0,
            500,
            ItemKind::Text {
                text: "TOP".into(),
                style: TextStyle {
                    y: 0.1,
                    stroke: None,
                    ..Default::default()
                },
            },
        )
        .unwrap();
        assert!(has_overlays(&p));
        let s = build_ass(&p, (480, 480));
        assert!(s.contains("PlayResX: 480\nPlayResY: 480"));
        // 64px at 1920 tall → 16px at 480 tall; centred at x=0.5, y=0.82.
        assert!(s.contains("Dialogue: 1,0:00:01.00,0:00:02.50,Default,,0,0,0,,{\\an5\\pos(240,394)\\fnSyne\\fs16\\1c&H00FFFFFF\\3c&H00000000\\bord1\\shad0}hi (there)"), "{s}");
        assert!(s.contains("Dialogue: 0,0:00:00.00,0:00:00.50"));
        assert!(s.contains("\\bord0"));
        assert!(!has_overlays(&Project::new("e", 1, 1, 1)));
    }

    #[test]
    fn karaoke_timing_and_colours() {
        let w = |s, e, t: &str| CaptionWord {
            start_ms: s,
            end_ms: e,
            text: t.into(),
        };
        assert_eq!(
            karaoke_text(&[w(200, 500, "Hi"), w(700, 900, "{you}")]),
            "{\\k20}{\\k50}Hi {\\k20}(you)"
        );
        assert_eq!(karaoke_text(&[w(0, 5, "a")]), "{\\k1}a");
        let mut p = Project::new("t", 100, 100, 30);
        let c = p.add_track(TrackKind::Caption, "C");
        p.add_item(
            &c,
            0,
            1000,
            ItemKind::Caption {
                text: "Hi you".into(),
                style: TextStyle {
                    highlight: Some("#ffd400".into()),
                    ..Default::default()
                },
                words: vec![w(0, 400, "Hi"), w(400, 900, "you")],
                speaker: None,
            },
        )
        .unwrap();
        let s = build_ass(&p, (100, 100));
        assert!(s.contains("\\1c&H0000D4FF\\2c&H00FFFFFF"), "{s}");
        assert!(s.ends_with("{\\k40}Hi {\\k50}you\n"), "{s}");
        // Highlight without word timings falls back to plain text.
        if let ItemKind::Caption { words, .. } = &mut p.tracks[0].items[0].kind {
            words.clear();
        }
        let s = build_ass(&p, (100, 100));
        assert!(!s.contains("\\k") && !s.contains("\\2c") && s.ends_with("Hi you\n"));
    }

    #[test]
    fn text_animations_and_item_fades() {
        let mut p = Project::new("t", 100, 100, 30);
        let x = p.add_track(TrackKind::Text, "T");
        let add = |p: &mut Project, at: u64, anim: TextAnim| {
            p.add_item(
                &x,
                at,
                1000,
                ItemKind::Text {
                    text: "T".into(),
                    style: TextStyle {
                        anim,
                        ..Default::default()
                    },
                },
            )
            .unwrap()
        };
        add(&mut p, 0, TextAnim::Fade);
        add(&mut p, 1000, TextAnim::Pop);
        let id = add(&mut p, 2000, TextAnim::SlideUp);
        p.set_fade(
            &id,
            Some(ms_project::Fade {
                in_ms: 100,
                out_ms: 300,
            }),
        )
        .unwrap();
        let s = build_ass(&p, (100, 100));
        let lines: Vec<&str> = s.lines().filter(|l| l.starts_with("Dialogue")).collect();
        assert!(lines[0].contains("\\fad(250,250)}T"), "{s}");
        assert!(lines[1].contains("\\fscx70\\fscy70\\t(0,180,\\fscx100\\fscy100)}T"));
        assert!(
            lines[2].contains("\\move(50,86,50,82,0,250)") && !lines[2].contains("\\pos("),
            "{}",
            lines[2]
        );
        assert!(lines[2].ends_with("{\\fad(100,300)}T"), "{}", lines[2]);
    }
}
