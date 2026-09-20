use chrono::{DateTime, Utc};
use objexel_common::{Identity, Track};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ObjectSignature { pub width: f32, pub height: f32, pub displacement: f32 }

#[derive(Debug, Clone, Copy)]
pub struct IdentityConfig { pub match_threshold: f32 }

impl Default for IdentityConfig { fn default() -> Self { Self { match_threshold: 0.72 } } }

pub fn signature(track: &Track) -> ObjectSignature {
    let (width, height) = if track.movement_path.is_empty() { (0.0, 0.0) } else {
        let (w, h) = track.movement_path.iter().fold((0.0, 0.0), |(w, h), box_| (w + box_.width, h + box_.height));
        (w / track.movement_path.len() as f32, h / track.movement_path.len() as f32)
    };
    let displacement = track.movement_path.first().zip(track.movement_path.last()).map(|(a, b)| {
        let ax = a.x + a.width / 2.0; let ay = a.y + a.height / 2.0;
        let bx = b.x + b.width / 2.0; let by = b.y + b.height / 2.0;
        ((bx - ax).powi(2) + (by - ay).powi(2)).sqrt()
    }).unwrap_or(0.0);
    ObjectSignature { width, height, displacement }
}

pub fn similarity(left: &ObjectSignature, right: &ObjectSignature) -> f32 {
    let size = 1.0 - ((left.width - right.width).abs() + (left.height - right.height).abs()).min(1.0);
    let movement = 1.0 - (left.displacement - right.displacement).abs().min(1.0);
    (size * 0.7 + movement * 0.3).clamp(0.0, 1.0)
}

pub fn familiarity(sightings: i64) -> &'static str { match sightings { 0..=1 => "new", 2..=5 => "familiar", 6..=19 => "frequent", _ => "very_frequent" } }
pub fn score(sightings: i64) -> f32 { (sightings as f32 / 20.0).min(1.0) }
pub fn new_identity(track: &Track, now: DateTime<Utc>) -> Identity { Identity { id: Uuid::new_v4(), object_class: track.object_class.clone(), display_name: None, familiarity_score: 0.05, familiarity: "new".into(), first_seen: now, last_seen: now, sightings: 1 } }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn familiarity_progresses() { assert_eq!(familiarity(1), "new"); assert_eq!(familiarity(6), "frequent"); assert_eq!(familiarity(20), "very_frequent"); }
}
