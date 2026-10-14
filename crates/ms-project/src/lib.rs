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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    pub start_ms: u64,
    pub duration_ms: u64,
    #[serde(flatten)]
    pub kind: ItemKind,
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
}
