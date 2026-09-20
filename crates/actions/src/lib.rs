use anyhow::{anyhow, Result};
use chrono::Utc;
use objexel_common::{Action, ActionExecution, Event, NotificationProvider, NotificationTemplate};
use objexel_notifications::NotificationService;
use std::time::Instant;
use uuid::Uuid;

#[derive(Clone, Default)]
pub struct ActionDispatcher { notifications: NotificationService }

impl ActionDispatcher {
    pub fn new() -> Self { Self { notifications: NotificationService::new() } }

    pub async fn execute(&self, action: &Action, provider: Option<&NotificationProvider>, template: Option<&NotificationTemplate>, event: &Event) -> ActionExecution {
        let started = Instant::now();
        let result = match action.action_type.as_str() {
            // Internal notifications are persisted by the event pipeline and do not
            // require an external provider configuration.
            "internal" if action.enabled => Ok(()),
            "internal" => Err(anyhow!("action is disabled")),
            "notification" | "email" | "webhook" | "mqtt" => match provider {
                Some(provider) if provider.enabled && action.enabled => self.notifications.send(provider, template, event).await,
                Some(_) => Err(anyhow!("notification provider is disabled")),
                None => Err(anyhow!("action has no notification provider")),
            },
            other => Err(anyhow!("unsupported action type: {other}")),
        };
        let status = if result.is_ok() { "success" } else { "failed" };
        let error_message = result.err().map(|error| error.to_string());
        tracing::info!(action_id = %action.id, event_id = %event.id, status, "action execution finished");
        ActionExecution { id: Uuid::new_v4(), action_id: action.id, event_id: event.id, status: status.into(), execution_time_ms: Some(started.elapsed().as_millis() as i64), error_message, created_at: Utc::now() }
    }
}
