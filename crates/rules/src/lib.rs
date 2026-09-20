use chrono::{DateTime, Duration, Utc};
use objexel_common::{Event, EventSeverity, Observation, Rule};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ObservationContext<'a> {
    pub observation: &'a Observation,
    pub object_class: Option<&'a str>,
    pub zone_id: Option<Uuid>,
    pub confidence: Option<f32>,
    pub duration_ms: Option<i64>,
}

#[derive(Default)]
pub struct RuleEngine {
    last_emitted: HashMap<Uuid, DateTime<Utc>>,
}

impl RuleEngine {
    pub fn evaluate(&mut self, rules: &[Rule], context: ObservationContext<'_>, now: DateTime<Utc>) -> Vec<Event> {
        rules.iter().filter(|rule| rule.enabled && self.matches(rule, &context) && self.available(rule, now)).map(|rule| {
            self.last_emitted.insert(rule.id, now);
            Event { id: Uuid::new_v4(), rule_id: rule.id, camera_id: context.observation.camera_id, track_id: Some(context.observation.track_id), observation_id: Some(context.observation.id), event_type: context.observation.observation_type.clone(), summary: format!("{}: {}", rule.name, context.observation.summary), severity: rule.severity.clone(), created_at: now }
        }).collect()
    }

    fn matches(&self, rule: &Rule, context: &ObservationContext<'_>) -> bool {
        !rule.conditions.is_empty() && rule.conditions.iter().all(|condition|
            condition.object_class.as_deref().map(|value| context.object_class == Some(value)).unwrap_or(true)
            && condition.zone_id.map(|value| context.zone_id == Some(value)).unwrap_or(true)
            && condition.observation_type.as_deref().map(|value| context.observation.observation_type == value).unwrap_or(true)
            && condition.confidence_threshold.map(|value| context.confidence.unwrap_or(0.0) >= value).unwrap_or(true)
            && condition.minimum_duration_ms.map(|value| context.duration_ms.unwrap_or(0) >= value).unwrap_or(true)
        )
    }

    fn available(&self, rule: &Rule, now: DateTime<Utc>) -> bool {
        let window = rule.cooldown_seconds.max(rule.suppression_seconds);
        self.last_emitted.get(&rule.id).map(|last| now - *last >= Duration::seconds(window)).unwrap_or(true)
    }
}

pub fn severity_name(severity: &EventSeverity) -> &'static str { match severity { EventSeverity::Info => "info", EventSeverity::Warning => "warning", EventSeverity::Critical => "critical" } }

#[cfg(test)]
mod tests {
    use super::*;
    use objexel_common::{RuleCondition, RuleConditionInput};
    #[test]
    fn matching_rule_is_suppressed_until_cooldown() {
        let now = Utc::now();
        let observation = Observation { id: Uuid::new_v4(), camera_id: Uuid::new_v4(), track_id: Uuid::new_v4(), observation_type: "zone_entered".into(), summary: "Cat entered backyard".into(), created_at: now };
        let rule = Rule { id: Uuid::new_v4(), name: "Cat in backyard".into(), enabled: true, description: String::new(), cooldown_seconds: 30, suppression_seconds: 0, severity: EventSeverity::Info, conditions: vec![RuleCondition { id: Uuid::new_v4(), rule_id: Uuid::new_v4(), object_class: Some("cat".into()), zone_id: None, observation_type: Some("zone_entered".into()), confidence_threshold: None, minimum_duration_ms: None }], created_at: now, updated_at: now };
        let mut engine = RuleEngine::default();
        let context = ObservationContext { observation: &observation, object_class: Some("cat"), zone_id: None, confidence: Some(0.9), duration_ms: None };
        assert_eq!(engine.evaluate(&[rule.clone()], context.clone(), now).len(), 1);
        assert!(engine.evaluate(&[rule], context, now + Duration::seconds(1)).is_empty());
    }

    #[test]
    fn unused_input_type_is_constructible() { let _ = RuleConditionInput { object_class: None, zone_id: None, observation_type: None, confidence_threshold: None, minimum_duration_ms: None }; }
}
