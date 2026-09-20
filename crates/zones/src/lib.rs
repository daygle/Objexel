use anyhow::{bail, Result};
use chrono::{DateTime, Duration, Utc};
use objexel_common::{Track, Zone, ZoneEvent, ZoneEventType};
use std::collections::HashMap;
use uuid::Uuid;

pub fn validate_polygon(polygon: &[objexel_common::PolygonPoint]) -> Result<()> {
    if polygon.len() < 3 { bail!("a zone polygon requires at least three points"); }
    if polygon.iter().any(|point| !(0.0..=1.0).contains(&point.x) || !(0.0..=1.0).contains(&point.y)) { bail!("polygon coordinates must be normalized between 0 and 1"); }
    Ok(())
}

#[derive(Debug, Clone)]
struct Presence { entered_at: DateTime<Utc>, inside: bool }

#[derive(Default)]
pub struct ZoneEvaluator { presence: HashMap<(Uuid, Uuid), Presence> }

impl ZoneEvaluator {
    pub fn evaluate(&mut self, zones: &[Zone], tracks: &[Track], now: DateTime<Utc>) -> Vec<ZoneEvent> {
        let mut events = Vec::new();
        for zone in zones.iter().filter(|zone| zone.enabled) {
            for track in tracks.iter().filter(|track| track.camera_id == zone.camera_id) {
                let position = track.movement_path.last().map(|box_| (box_.x + box_.width / 2.0, box_.y + box_.height / 2.0));
                let inside = position.map(|point| point_in_polygon(point, &zone.polygon_coordinates)).unwrap_or(false);
                let key = (zone.id, track.id);
                let previous = self.presence.get(&key).cloned();
                match (previous, inside) {
                    (None, true) => {
                        self.presence.insert(key, Presence { entered_at: now, inside: true });
                        events.push(event(zone, track, ZoneEventType::Entered, now, None));
                    }
                    (Some(previous), false) if previous.inside => {
                        self.presence.remove(&key);
                        events.push(event(zone, track, ZoneEventType::Exited, now, Some((now - previous.entered_at).num_milliseconds())));
                    }
                    (Some(_), true) => {}
                    (None, false) => {}
                }
            }
        }
        self.presence.retain(|(_, track_id), presence| tracks.iter().any(|track| track.id == *track_id) && presence.inside);
        events
    }

    pub fn is_inside(&self, zone_id: Uuid, track_id: Uuid) -> bool { self.presence.get(&(zone_id, track_id)).map(|presence| presence.inside).unwrap_or(false) }
}

fn event(zone: &Zone, track: &Track, event_type: ZoneEventType, occurred_at: DateTime<Utc>, duration_ms: Option<i64>) -> ZoneEvent {
    ZoneEvent { id: Uuid::new_v4(), zone_id: zone.id, camera_id: zone.camera_id, track_id: track.id, event_type, occurred_at, duration_ms }
}

pub fn point_in_polygon(point: (f32, f32), polygon: &[objexel_common::PolygonPoint]) -> bool {
    if polygon.len() < 3 { return false; }
    let (x, y) = point;
    let mut inside = false;
    let mut previous = polygon.len() - 1;
    for current in 0..polygon.len() {
        let a = &polygon[current];
        let b = &polygon[previous];
        if ((a.y > y) != (b.y > y)) && (x < (b.x - a.x) * (y - a.y) / (b.y - a.y) + a.x) { inside = !inside; }
        previous = current;
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;
    use objexel_common::{BoundingBox, PolygonPoint};
    #[test]
    fn point_in_square_is_detected() {
        let square = vec![PolygonPoint { x: 0.1, y: 0.1 }, PolygonPoint { x: 0.9, y: 0.1 }, PolygonPoint { x: 0.9, y: 0.9 }, PolygonPoint { x: 0.1, y: 0.9 }];
        assert!(point_in_polygon((0.5, 0.5), &square));
        assert!(!point_in_polygon((0.95, 0.5), &square));
    }

    #[test]
    fn entry_and_exit_are_emitted() {
        let now = Utc::now();
        let zone = Zone { id: Uuid::new_v4(), camera_id: Uuid::new_v4(), name: "yard".into(), polygon_coordinates: vec![PolygonPoint { x: 0.0, y: 0.0 }, PolygonPoint { x: 1.0, y: 0.0 }, PolygonPoint { x: 1.0, y: 1.0 }], colour: "#fff".into(), enabled: true, created_at: now };
        let mut evaluator = ZoneEvaluator::default();
        let track = Track { id: Uuid::new_v4(), camera_id: zone.camera_id, object_class: "cat".into(), first_seen: now, last_seen: now, duration_ms: 0, movement_path: vec![BoundingBox { x: 0.2, y: 0.2, width: 0.1, height: 0.1 }] };
        assert_eq!(evaluator.evaluate(&[zone.clone()], &[track.clone()], now)[0].event_type, ZoneEventType::Entered);
        let outside = Track { movement_path: vec![BoundingBox { x: 2.0, y: 2.0, width: 0.1, height: 0.1 }], ..track };
        assert_eq!(evaluator.evaluate(&[zone], &[outside], now + Duration::seconds(2))[0].event_type, ZoneEventType::Exited);
    }
}
