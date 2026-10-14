//! MediaSmith project model: a timeline of tracks holding clips, text and
//! caption layers. Times are integer milliseconds so save/load is exact.
//! Serialized as versioned JSON (`.msproj`).

use serde::{Deserialize, Serialize};

pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("invalid project JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported project version {0} (this build reads up to {FORMAT_VERSION})")]
    Version(u32),
    #[error("item {0} overlaps another item on track {1}")]
    Overlap(String, String),
    #[error("item {0} has a non-positive duration")]
    EmptyItem(String),
    #[error("no track with id {0}")]
    NoTrack(String),
    #[error("no item with id {0}")]
    NoItem(String),
    #[error("item {0} can't move to track {1} of a different kind")]
    KindMismatch(String, String),
    #[error("trimming item {0} would start before its source")]
    TrimBeforeSource(String),
    #[error("split point {0}ms is outside item {1}")]
    BadSplit(u64, String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrackKind {
    Video,
    Audio,
    Text,
    Caption,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextStyle {
    pub font: String,
    pub size_px: u32,
    pub color: String,
    pub stroke: Option<String>,
    /// Normalised 0..1 position of the text box centre.
    pub x: f32,
    pub y: f32,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font: "Syne".into(),
            size_px: 64,
            color: "#ffffff".into(),
            stroke: Some("#000000".into()),
            x: 0.5,
            y: 0.82,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ItemKind {
    /// A span of a media source. `source_in_ms` is the offset into the file.
    Clip {
        source: String,
        source_in_ms: u64,
        volume: f32,
    },
    Text {
        text: String,
        style: TextStyle,
    },
    Caption {
        text: String,
        style: TextStyle,
    },
}

/// Placement of a clip on an overlay (non-main) video track: normalised
/// centre position, width as a fraction of the canvas, and opacity.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    pub opacity: f32,
}

impl Default for Transform {
    /// Picture-in-picture, bottom-right.
    fn default() -> Self {
        Self {
            x: 0.78,
            y: 0.78,
            scale: 0.35,
            opacity: 1.0,
        }
    }
}

impl Transform {
    /// Clamp into renderable ranges.
    pub fn clamped(self) -> Self {
        let f = |v: f32, lo: f32, hi: f32| if v.is_finite() { v.clamp(lo, hi) } else { lo };
        Self {
            x: f(self.x, 0.0, 1.0),
            y: f(self.y, 0.0, 1.0),
            scale: f(self.scale, 0.05, 1.0),
            opacity: f(self.opacity, 0.0, 1.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    pub start_ms: u64,
    pub duration_ms: u64,
    #[serde(flatten)]
    pub kind: ItemKind,
    /// Overlay placement; `None` = track default (full frame on the main
    /// track, [`Transform::default`] on overlay tracks).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<Transform>,
}

impl Item {
    pub fn end_ms(&self) -> u64 {
        self.start_ms + self.duration_ms
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub id: String,
    pub kind: TrackKind,
    pub name: String,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub version: u32,
    pub name: String,
    pub canvas: Canvas,
    pub tracks: Vec<Track>,
    #[serde(default)]
    next_id: u64,
}

impl Project {
    pub fn new(name: &str, width: u32, height: u32, fps: u32) -> Self {
        Self {
            version: FORMAT_VERSION,
            name: name.into(),
            canvas: Canvas { width, height, fps },
            tracks: Vec::new(),
            next_id: 0,
        }
    }

    fn fresh_id(&mut self, prefix: &str) -> String {
        self.next_id += 1;
        format!("{prefix}{}", self.next_id)
    }

    pub fn add_track(&mut self, kind: TrackKind, name: &str) -> String {
        let id = self.fresh_id("t");
        self.tracks.push(Track {
            id: id.clone(),
            kind,
            name: name.into(),
            muted: false,
            items: vec![],
        });
        id
    }

    fn track_mut(&mut self, id: &str) -> Result<&mut Track, ProjectError> {
        self.tracks
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or_else(|| ProjectError::NoTrack(id.into()))
    }

    /// Insert an item, rejecting zero length or overlap on the track.
    pub fn add_item(
        &mut self,
        track: &str,
        start_ms: u64,
        duration_ms: u64,
        kind: ItemKind,
    ) -> Result<String, ProjectError> {
        let id = self.fresh_id("i");
        let item = Item {
            id: id.clone(),
            start_ms,
            duration_ms,
            kind,
            transform: None,
        };
        let t = self.track_mut(track)?;
        t.items.push(item);
        t.items.sort_by_key(|i| i.start_ms);
        if let Err(e) = validate_track(t) {
            t.items.retain(|i| i.id != id);
            return Err(e);
        }
        Ok(id)
    }

    /// Split an item at an absolute timeline time; returns the new right half.
    pub fn split(&mut self, item_id: &str, at_ms: u64) -> Result<String, ProjectError> {
        let new_id = self.fresh_id("i");
        for t in &mut self.tracks {
            if let Some(pos) = t.items.iter().position(|i| i.id == item_id) {
                let it = &mut t.items[pos];
                if at_ms <= it.start_ms || at_ms >= it.end_ms() {
                    return Err(ProjectError::BadSplit(at_ms, item_id.into()));
                }
                let left = at_ms - it.start_ms;
                let mut right = it.clone();
                right.id = new_id.clone();
                right.start_ms = at_ms;
                right.duration_ms = it.duration_ms - left;
                if let ItemKind::Clip { source_in_ms, .. } = &mut right.kind {
                    *source_in_ms += left;
                }
                it.duration_ms = left;
                t.items.insert(pos + 1, right);
                return Ok(new_id);
            }
        }
        Err(ProjectError::NoItem(item_id.into()))
    }

    fn locate(&self, item_id: &str) -> Result<(usize, usize), ProjectError> {
        for (ti, t) in self.tracks.iter().enumerate() {
            if let Some(ii) = t.items.iter().position(|i| i.id == item_id) {
                return Ok((ti, ii));
            }
        }
        Err(ProjectError::NoItem(item_id.into()))
    }

    /// Apply `f` to a copy of the project and keep it only if it validates,
    /// so a rejected edit never leaves a half-applied state.
    fn transact(
        &mut self,
        f: impl FnOnce(&mut Project) -> Result<(), ProjectError>,
    ) -> Result<(), ProjectError> {
        let mut next = self.clone();
        f(&mut next)?;
        for t in &mut next.tracks {
            t.items.sort_by_key(|i| i.start_ms);
        }
        next.validate()?;
        *self = next;
        Ok(())
    }

    /// Move an item to a new start time, optionally onto another track of
    /// the same kind. Rejected (no change) if it would overlap.
    pub fn move_item(
        &mut self,
        item_id: &str,
        start_ms: u64,
        to_track: Option<&str>,
    ) -> Result<(), ProjectError> {
        let (ti, ii) = self.locate(item_id)?;
        let dest = match to_track {
            Some(id) => self
                .tracks
                .iter()
                .position(|t| t.id == id)
                .ok_or_else(|| ProjectError::NoTrack(id.into()))?,
            None => ti,
        };
        if self.tracks[dest].kind != self.tracks[ti].kind {
            return Err(ProjectError::KindMismatch(
                item_id.into(),
                self.tracks[dest].id.clone(),
            ));
        }
        self.transact(|p| {
            let mut it = p.tracks[ti].items.remove(ii);
            it.start_ms = start_ms;
            p.tracks[dest].items.push(it);
            Ok(())
        })
    }

    /// Trim an item to a new `[start, start+duration)` window on the timeline.
    /// Trimming the left edge advances a clip's source offset by the same
    /// amount so the frames under the playhead don't shift.
    pub fn trim(
        &mut self,
        item_id: &str,
        start_ms: u64,
        duration_ms: u64,
    ) -> Result<(), ProjectError> {
        let (ti, ii) = self.locate(item_id)?;
        self.transact(|p| {
            let it = &mut p.tracks[ti].items[ii];
            if let ItemKind::Clip { source_in_ms, .. } = &mut it.kind {
                let shifted = *source_in_ms as i64 + (start_ms as i64 - it.start_ms as i64);
                if shifted < 0 {
                    return Err(ProjectError::TrimBeforeSource(item_id.into()));
                }
                *source_in_ms = shifted as u64;
            }
            it.start_ms = start_ms;
            it.duration_ms = duration_ms;
            Ok(())
        })
    }

    /// Remove an item; returns it.
    pub fn delete_item(&mut self, item_id: &str) -> Result<Item, ProjectError> {
        let (ti, ii) = self.locate(item_id)?;
        Ok(self.tracks[ti].items.remove(ii))
    }

    /// Set (or clear) an item's overlay transform.
    pub fn set_transform(
        &mut self,
        item_id: &str,
        transform: Option<Transform>,
    ) -> Result<(), ProjectError> {
        let (ti, ii) = self.locate(item_id)?;
        self.tracks[ti].items[ii].transform = transform.map(Transform::clamped);
        Ok(())
    }

    /// Overlay video tracks (every video track after the first), bottom to
    /// top, with their clips in timeline order.
    pub fn overlay_video_clips(&self) -> Vec<&Item> {
        self.tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Video)
            .skip(1)
            .filter(|t| !t.muted)
            .flat_map(|t| t.items.iter())
            .filter(|i| matches!(i.kind, ItemKind::Clip { .. }))
            .collect()
    }

    /// Replace every item on a caption track with `items` (start, duration,
    /// text), using the default style. Overlapping or empty entries are
    /// dropped. Returns the number placed.
    pub fn replace_captions(
        &mut self,
        track: &str,
        items: &[(u64, u64, String)],
        style: &TextStyle,
    ) -> Result<usize, ProjectError> {
        if self.track_mut(track)?.kind != TrackKind::Caption {
            return Err(ProjectError::KindMismatch("captions".into(), track.into()));
        }
        let mut placed = Vec::new();
        let mut cursor = 0u64;
        for (start, dur, text) in items {
            let text = text.trim();
            if *dur == 0 || text.is_empty() || *start < cursor {
                continue;
            }
            let id = self.fresh_id("i");
            placed.push(Item {
                id,
                start_ms: *start,
                duration_ms: *dur,
                kind: ItemKind::Caption {
                    text: text.into(),
                    style: style.clone(),
                },
                transform: None,
            });
            cursor = start + dur;
        }
        let n = placed.len();
        self.track_mut(track)?.items = placed;
        Ok(n)
    }

    /// Clips of the first video track, in timeline order: what a simple
    /// single-track export renders.
    pub fn main_video_clips(&self) -> Vec<&Item> {
        self.tracks
            .iter()
            .find(|t| t.kind == TrackKind::Video)
            .map(|t| {
                t.items
                    .iter()
                    .filter(|i| matches!(i.kind, ItemKind::Clip { .. }))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Project length: the end of the last item on any track.
    pub fn duration_ms(&self) -> u64 {
        self.tracks
            .iter()
            .flat_map(|t| t.items.iter())
            .map(Item::end_ms)
            .max()
            .unwrap_or(0)
    }

    pub fn validate(&self) -> Result<(), ProjectError> {
        if self.version > FORMAT_VERSION {
            return Err(ProjectError::Version(self.version));
        }
        self.tracks.iter().try_for_each(validate_track)
    }

    pub fn to_json(&self) -> Result<String, ProjectError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn from_json(s: &str) -> Result<Self, ProjectError> {
        let p: Project = serde_json::from_str(s)?;
        p.validate()?;
        Ok(p)
    }
}

fn validate_track(t: &Track) -> Result<(), ProjectError> {
    let mut items: Vec<&Item> = t.items.iter().collect();
    items.sort_by_key(|i| i.start_ms);
    for i in &items {
        if i.duration_ms == 0 {
            return Err(ProjectError::EmptyItem(i.id.clone()));
        }
    }
    for w in items.windows(2) {
        if w[1].start_ms < w[0].end_ms() {
            return Err(ProjectError::Overlap(w[1].id.clone(), t.id.clone()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transform_roundtrips_and_clamps() {
        let mut p = Project::new("t", 1080, 1920, 30);
        let v1 = p.add_track(TrackKind::Video, "V1");
        let v2 = p.add_track(TrackKind::Video, "Overlay");
        let clip = || ItemKind::Clip {
            source: "a.mp4".into(),
            source_in_ms: 0,
            volume: 1.0,
        };
        p.add_item(&v1, 0, 1000, clip()).unwrap();
        let o = p.add_item(&v2, 200, 500, clip()).unwrap();
        p.set_transform(
            &o,
            Some(Transform {
                x: 2.0,
                y: -1.0,
                scale: 0.0,
                opacity: 0.5,
            }),
        )
        .unwrap();
        let t = p.overlay_video_clips()[0].transform.unwrap();
        assert_eq!((t.x, t.y, t.scale, t.opacity), (1.0, 0.0, 0.05, 0.5));
        assert_eq!(p.main_video_clips().len(), 1);
        let back = Project::from_json(&p.to_json().unwrap()).unwrap();
        assert_eq!(back, p);
        // Items without a transform don't serialise the key (old files stay identical).
        assert!(!p.to_json().unwrap().contains("\"transform\":null"));
        assert!(p.set_transform("nope", None).is_err());
    }

    #[test]
    fn replace_captions_fills_track_and_drops_bad_entries() {
        let mut p = Project::new("t", 1080, 1920, 30);
        let c = p.add_track(TrackKind::Caption, "C");
        let v = p.add_track(TrackKind::Video, "V");
        p.add_item(
            &c,
            0,
            100,
            ItemKind::Caption {
                text: "old".into(),
                style: TextStyle::default(),
            },
        )
        .unwrap();
        let n = p
            .replace_captions(
                &c,
                &[
                    (0, 900, " hello ".into()),
                    (500, 400, "overlaps".into()),
                    (1000, 0, "empty".into()),
                    (1000, 800, "world".into()),
                    (2000, 500, "  ".into()),
                ],
                &TextStyle::default(),
            )
            .unwrap();
        assert_eq!(n, 2);
        let t = p.tracks.iter().find(|t| t.id == c).unwrap();
        let texts: Vec<_> = t
            .items
            .iter()
            .map(|i| match &i.kind {
                ItemKind::Caption { text, .. } => text.as_str(),
                _ => "",
            })
            .collect();
        assert_eq!(texts, ["hello", "world"]);
        assert!(p.validate().is_ok());
        assert!(p.replace_captions(&v, &[], &TextStyle::default()).is_err());
    }

    fn clip(src: &str) -> ItemKind {
        ItemKind::Clip {
            source: src.into(),
            source_in_ms: 0,
            volume: 1.0,
        }
    }

    fn sample() -> Project {
        let mut p = Project::new("demo", 1080, 1920, 30);
        let v = p.add_track(TrackKind::Video, "V1");
        let c = p.add_track(TrackKind::Caption, "Captions");
        p.add_item(&v, 0, 5000, clip("a.mp4")).unwrap();
        p.add_item(&v, 5000, 3000, clip("b.mp4")).unwrap();
        p.add_item(
            &c,
            500,
            1500,
            ItemKind::Caption {
                text: "hello".into(),
                style: TextStyle::default(),
            },
        )
        .unwrap();
        p
    }

    #[test]
    fn json_round_trip_is_lossless() {
        let p = sample();
        let back = Project::from_json(&p.to_json().unwrap()).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn json_is_tagged_and_readable() {
        let j = sample().to_json().unwrap();
        assert!(j.contains("\"type\": \"clip\""));
        assert!(j.contains("\"kind\": \"caption\""));
    }

    #[test]
    fn duration_is_last_item_end() {
        assert_eq!(sample().duration_ms(), 8000);
        assert_eq!(Project::new("e", 1, 1, 1).duration_ms(), 0);
    }

    #[test]
    fn overlap_and_empty_items_are_rejected() {
        let mut p = sample();
        let v = p.tracks[0].id.clone();
        assert!(matches!(
            p.add_item(&v, 4000, 2000, clip("x")),
            Err(ProjectError::Overlap(..))
        ));
        assert!(matches!(
            p.add_item(&v, 9000, 0, clip("x")),
            Err(ProjectError::EmptyItem(..))
        ));
        assert_eq!(p.tracks[0].items.len(), 2, "rejected items are rolled back");
        assert!(matches!(
            p.add_item("nope", 0, 1, clip("x")),
            Err(ProjectError::NoTrack(..))
        ));
    }

    #[test]
    fn split_preserves_source_offset() {
        let mut p = sample();
        let first = p.tracks[0].items[0].id.clone();
        let right = p.split(&first, 2000).unwrap();
        let items = &p.tracks[0].items;
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].duration_ms, 2000);
        assert_eq!(items[1].id, right);
        assert_eq!(items[1].start_ms, 2000);
        assert_eq!(items[1].duration_ms, 3000);
        assert!(matches!(
            items[1].kind,
            ItemKind::Clip {
                source_in_ms: 2000,
                ..
            }
        ));
        assert_eq!(p.duration_ms(), 8000);
        assert!(p.split(&first, 0).is_err());
        assert!(p.split("missing", 10).is_err());
    }

    #[test]
    fn ids_stay_unique_after_reload() {
        let p = sample();
        let mut back = Project::from_json(&p.to_json().unwrap()).unwrap();
        let t = back.add_track(TrackKind::Audio, "A1");
        let all: Vec<&str> = back
            .tracks
            .iter()
            .map(|t| t.id.as_str())
            .chain(
                back.tracks
                    .iter()
                    .flat_map(|t| t.items.iter().map(|i| i.id.as_str())),
            )
            .collect();
        let mut dedup = all.clone();
        dedup.sort();
        dedup.dedup();
        assert_eq!(all.len(), dedup.len(), "{t}");
    }

    #[test]
    fn future_versions_and_bad_json_are_rejected() {
        let mut p = sample();
        p.version = FORMAT_VERSION + 1;
        let j = serde_json::to_string(&p).unwrap();
        assert!(matches!(
            Project::from_json(&j),
            Err(ProjectError::Version(_))
        ));
        assert!(matches!(
            Project::from_json("{"),
            Err(ProjectError::Json(_))
        ));
    }

    #[test]
    fn hand_edited_overlap_fails_validation_on_load() {
        let mut p = sample();
        p.tracks[0].items[1].start_ms = 100;
        let j = serde_json::to_string(&p).unwrap();
        assert!(matches!(
            Project::from_json(&j),
            Err(ProjectError::Overlap(..))
        ));
    }

    #[test]
    fn move_item_within_and_across_tracks() {
        let mut p = sample();
        let v = p.tracks[0].id.clone();
        let v2 = p.add_track(TrackKind::Video, "V2");
        let b = p.tracks[0].items[1].id.clone();
        p.move_item(&b, 9000, None).unwrap();
        assert_eq!(p.tracks[0].items[1].start_ms, 9000);
        p.move_item(&b, 0, Some(&v2)).unwrap();
        assert_eq!(p.tracks[0].items.len(), 1);
        assert_eq!(p.tracks.last().unwrap().items[0].id, b);
        let a = p.tracks[0].items[0].id.clone();
        // Overlap and kind mismatch are rejected without changing anything.
        let before = p.clone();
        assert!(matches!(
            p.move_item(&a, 0, Some(&v2)),
            Err(ProjectError::Overlap(..))
        ));
        let cap = p.tracks[1].id.clone();
        assert!(matches!(
            p.move_item(&a, 0, Some(&cap)),
            Err(ProjectError::KindMismatch(..))
        ));
        assert_eq!(p, before);
        assert!(p.move_item("nope", 0, Some(&v)).is_err());
    }

    #[test]
    fn trim_left_edge_advances_source_offset() {
        let mut p = sample();
        let a = p.tracks[0].items[0].id.clone();
        p.trim(&a, 1000, 3000).unwrap();
        let it = &p.tracks[0].items[0];
        assert_eq!((it.start_ms, it.duration_ms), (1000, 3000));
        assert!(matches!(
            it.kind,
            ItemKind::Clip {
                source_in_ms: 1000,
                ..
            }
        ));
        // Can't extend left past the source start, or to zero length, or into a neighbour.
        assert!(matches!(p.trim(&a, 0, 4000), Ok(())));
        assert!(matches!(
            p.trim(&a, 0, 4000).and(p.trim(&a, 0, 0)),
            Err(ProjectError::EmptyItem(_))
        ));
        let mut q = sample();
        let a2 = q.tracks[0].items[0].id.clone();
        q.tracks[0].items[0].start_ms = 500;
        assert!(matches!(
            q.trim(&a2, 0, 5000),
            Err(ProjectError::TrimBeforeSource(_))
        ));
        assert!(matches!(
            q.trim(&a2, 500, 6000),
            Err(ProjectError::Overlap(..))
        ));
    }

    #[test]
    fn delete_and_main_video_clips() {
        let mut p = sample();
        assert_eq!(p.main_video_clips().len(), 2);
        let a = p.tracks[0].items[0].id.clone();
        let gone = p.delete_item(&a).unwrap();
        assert_eq!(gone.id, a);
        assert_eq!(p.main_video_clips().len(), 1);
        assert!(p.delete_item(&a).is_err());
    }
}
