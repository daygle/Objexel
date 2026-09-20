use objexel_common::{Behaviour, Identity};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventPriority { Informational, Low, Medium, High, Critical }

impl EventPriority {
    pub fn as_str(self) -> &'static str { match self { Self::Informational => "informational", Self::Low => "low", Self::Medium => "medium", Self::High => "high", Self::Critical => "critical" } }
    pub fn rank(self) -> u8 { match self { Self::Informational => 0, Self::Low => 1, Self::Medium => 2, Self::High => 3, Self::Critical => 4 } }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AdaptiveAssessment { pub familiarity_score: f32, pub anomaly_score: f32, pub priority_score: f32, pub familiarity_level: &'static str, pub behaviour_level: &'static str, pub priority: EventPriority }

pub fn assess(identity: &Identity, behaviour: Option<&Behaviour>, confidence: f32) -> AdaptiveAssessment {
    let familiarity_score = identity.familiarity_score.clamp(0.0, 1.0);
    let behaviour_level = match behaviour.map(|item| item.behaviour_type.as_str()) { Some("route_following") => "expected", Some("stationary_object") => "unusual", Some("loitering") => "rare", Some(_) => "anomalous", None => "expected" };
    let behaviour_anomaly = match behaviour_level { "expected" => 0.05, "unusual" => 0.35, "rare" => 0.65, _ => 0.9 };
    let anomaly_score = (behaviour_anomaly * 0.65 + (1.0 - familiarity_score) * 0.35).clamp(0.0, 1.0);
    let priority_score = (anomaly_score * 0.7 + confidence.clamp(0.0, 1.0) * 0.3).clamp(0.0, 1.0);
    let priority = if priority_score >= 0.9 { EventPriority::Critical } else if priority_score >= 0.7 { EventPriority::High } else if priority_score >= 0.45 { EventPriority::Medium } else if priority_score >= 0.2 { EventPriority::Low } else { EventPriority::Informational };
    let familiarity_level = if familiarity_score >= 0.8 { "highly_familiar" } else if familiarity_score >= 0.45 { "frequent" } else if familiarity_score >= 0.1 { "familiar" } else { "new" };
    AdaptiveAssessment { familiarity_score, anomaly_score, priority_score, familiarity_level, behaviour_level, priority }
}

pub fn priority_meets(actual: EventPriority, minimum: &str) -> bool { let required = match minimum { "critical" => EventPriority::Critical, "high" => EventPriority::High, "medium" => EventPriority::Medium, "low" => EventPriority::Low, _ => EventPriority::Informational }; actual.rank() >= required.rank() }

#[cfg(test)]
mod tests { use super::*; use objexel_common::{Identity, Behaviour}; use chrono::Utc; use uuid::Uuid;
    fn identity(score: f32) -> Identity { Identity { id: Uuid::new_v4(), object_class: "cat".into(), display_name: None, familiarity_score: score, familiarity: "new".into(), first_seen: Utc::now(), last_seen: Utc::now(), sightings: 1 } }
    #[test] fn unknown_loitering_is_high_priority() { let a = assess(&identity(0.0), None, 0.95); assert!(a.priority.rank() >= EventPriority::Medium.rank()); }
}
