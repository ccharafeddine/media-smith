//! Timeline filmstrip (ported from ClipSmith): one horizontal PNG montage of
//! square thumbnails sampled across a clip, written to stdout and returned as
//! a data URI. Each thumbnail uses a fast keyframe seek, so it stays quick on
//! long sources. Only argument building is here; the shell runs ffmpeg.

/// Square thumbnail edge in pixels.
pub const SIZE: u32 = 96;

/// Evenly spaced sample times (seconds) across `[in, in+dur)`.
pub fn sample_times(source_in_secs: f64, duration_secs: f64, count: u32) -> Vec<f64> {
    let count = count.clamp(1, 40);
    let dur = if duration_secs > 0.05 {
        duration_secs
    } else {
        1.0
    };
    (0..count)
        .map(|i| source_in_secs + (f64::from(i) + 0.5) * dur / f64::from(count))
        .collect()
}

/// ffmpeg args that write a single PNG strip of `count` thumbnails to stdout.
pub fn filmstrip_args(
    path: &str,
    source_in_secs: f64,
    duration_secs: f64,
    count: u32,
) -> Vec<String> {
    let times = sample_times(source_in_secs, duration_secs, count);
    let n = times.len();
    let mut args: Vec<String> = vec!["-v".into(), "error".into()];
    for t in &times {
        args.extend([
            "-ss".into(),
            format!("{t:.3}"),
            "-an".into(),
            "-i".into(),
            path.to_owned(),
        ]);
    }
    let mut fc = String::new();
    for i in 0..n {
        fc.push_str(&format!(
            "[{i}:v]crop=min(iw\\,ih):min(iw\\,ih),scale={SIZE}:{SIZE},setsar=1[v{i}];"
        ));
    }
    for i in 0..n {
        fc.push_str(&format!("[v{i}]"));
    }
    if n == 1 {
        fc.push_str("null[out]");
    } else {
        fc.push_str(&format!("hstack=inputs={n}[out]"));
    }
    args.extend(
        [
            "-filter_complex",
            &fc,
            "-map",
            "[out]",
            "-frames:v",
            "1",
            "-f",
            "image2pipe",
            "-vcodec",
            "png",
            "pipe:1",
        ]
        .iter()
        .map(|s| (*s).to_string()),
    );
    args
}

/// Encode PNG bytes as a data URI (tiny local base64, no extra dependency).
pub fn png_data_uri(png: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(png.len() * 4 / 3 + 32);
    s.push_str("data:image/png;base64,");
    for c in png.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        s.push(T[(n >> 18) as usize & 63] as char);
        s.push(T[(n >> 12) as usize & 63] as char);
        s.push(if c.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        s.push(if c.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_are_centred_in_equal_slots() {
        assert_eq!(sample_times(10.0, 4.0, 4), vec![10.5, 11.5, 12.5, 13.5]);
        assert_eq!(sample_times(0.0, 0.0, 1), vec![0.5]);
        assert_eq!(sample_times(0.0, 1.0, 999).len(), 40);
    }

    #[test]
    fn args_hstack_and_pipe_png() {
        let a = filmstrip_args("v.mp4", 0.0, 8.0, 4);
        assert_eq!(a.iter().filter(|s| *s == "-i").count(), 4);
        assert!(a.iter().any(|s| s.contains("hstack=inputs=4[out]")));
        assert_eq!(a.last().unwrap(), "pipe:1");
        assert!(filmstrip_args("v.mp4", 0.0, 1.0, 1)
            .iter()
            .any(|s| s.ends_with("null[out]")));
    }

    #[test]
    fn base64_matches_known_vectors() {
        assert_eq!(png_data_uri(b""), "data:image/png;base64,");
        assert_eq!(png_data_uri(b"f"), "data:image/png;base64,Zg==");
        assert_eq!(png_data_uri(b"foobar"), "data:image/png;base64,Zm9vYmFy");
        assert_eq!(png_data_uri(b"fooba"), "data:image/png;base64,Zm9vYmE=");
    }
}
