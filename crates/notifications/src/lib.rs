use anyhow::{anyhow, Context, Result};
use lettre::{message::header::ContentType, AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use objexel_common::{Event, NotificationProvider, NotificationTemplate};
use reqwest::Client;
use rumqttc::{AsyncClient, MqttOptions, QoS};
use serde_json::{json, Value};
use std::time::Duration;
use uuid::Uuid;

#[derive(Clone, Default)]
pub struct NotificationService { http: Client }

impl NotificationService {
    pub fn new() -> Self {
        let http = Client::builder().connect_timeout(Duration::from_secs(5)).timeout(Duration::from_secs(20)).build().unwrap_or_default();
        Self { http }
    }

    /// Validate provider configuration before it is persisted or tested.
    pub fn validate_provider(provider: &NotificationProvider) -> Result<()> {
        match provider.provider_type.as_str() {
            "internal" => Ok(()),
            "email" => {
                let config = &provider.configuration;
                let host = required_string(config, "host")?;
                if host.contains(char::is_whitespace) { return Err(anyhow!("email host cannot contain whitespace")); }
                let port = config.get("port").and_then(Value::as_u64).unwrap_or(587);
                if !(1..=65_535).contains(&port) { return Err(anyhow!("email port must be between 1 and 65535")); }
                required_string(config, "from")?.parse::<lettre::Address>().context("email from address is invalid")?;
                required_string(config, "to")?.parse::<lettre::Address>().context("email to address is invalid")?;
                let username = config.get("username").and_then(Value::as_str);
                let password = config.get("password").and_then(Value::as_str);
                if username.is_some() != password.is_some() { return Err(anyhow!("email username and password must be supplied together")); }
                Ok(())
            }
            "webhook" => { let url = required_string(&provider.configuration, "url")?; reqwest::Url::parse(url).context("webhook URL is invalid")?; Ok(()) }
            "mqtt" => { required_string(&provider.configuration, "host")?; required_string(&provider.configuration, "topic")?; Ok(()) }
            other => Err(anyhow!("unsupported notification provider: {other}")),
        }
    }

    pub async fn send(&self, provider: &NotificationProvider, template: Option<&NotificationTemplate>, event: &Event) -> Result<()> {
        Self::validate_provider(provider)?;
        match provider.provider_type.as_str() {
            "internal" => Ok(()),
            "webhook" => self.send_webhook(provider, template, event).await,
            "email" => self.send_email(provider, template, event).await,
            "mqtt" => self.send_mqtt(provider, template, event).await,
            other => Err(anyhow!("unsupported notification provider: {other}")),
        }
    }

    async fn send_webhook(&self, provider: &NotificationProvider, template: Option<&NotificationTemplate>, event: &Event) -> Result<()> {
        let config = &provider.configuration;
        let url = required_string(config, "url")?;
        let mut request = self.http.post(url).json(&json!({ "event": event, "title": template.map(|t| t.subject.clone()), "body": template.map(|t| t.body.clone()) }));
        if let Some(headers) = config.get("headers").and_then(Value::as_object) {
            for (name, value) in headers { if let Some(value) = value.as_str() { request = request.header(name, value); } }
        }
        let response = request.send().await.context("send webhook")?;
        if !response.status().is_success() { return Err(anyhow!("webhook returned {}", response.status())); }
        Ok(())
    }

    async fn send_email(&self, provider: &NotificationProvider, template: Option<&NotificationTemplate>, event: &Event) -> Result<()> {
        Self::validate_provider(provider)?;
        let config = &provider.configuration;
        let from = required_string(config, "from")?;
        let to = required_string(config, "to")?;
        let subject = template.map(|t| t.subject.as_str()).unwrap_or(&event.event_type);
        let body = template.map(|t| t.body.as_str()).unwrap_or(&event.summary);
        let content_type = if template.map(|t| t.html).unwrap_or(false) { ContentType::TEXT_HTML } else { ContentType::TEXT_PLAIN };
        let message = Message::builder().from(from.parse()?).to(to.parse()?).subject(subject).header(content_type).body(body.to_owned())?;
        let host = required_string(config, "host")?;
        let port = config.get("port").and_then(Value::as_u64).unwrap_or(587) as u16;
        let username = config.get("username").and_then(Value::as_str);
        let password = config.get("password").and_then(Value::as_str);
        let mut last_error = None;
        for attempt in 0..3 {
            let transport = match (username, password) {
                (Some(username), Some(password)) => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)?.port(port).credentials(lettre::transport::smtp::authentication::Credentials::new(username.into(), password.into())).build(),
                _ => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host).port(port).build(),
            };
            match tokio::time::timeout(Duration::from_secs(20), transport.send(message.clone())).await {
                Ok(Ok(_)) => return Ok(()),
                Ok(Err(error)) => last_error = Some(error.to_string()),
                Err(_) => last_error = Some("SMTP send timed out after 20 seconds".into()),
            }
            if attempt < 2 { tokio::time::sleep(Duration::from_millis(250 * (1 << attempt))).await; }
        }
        Err(anyhow!("SMTP delivery failed after 3 attempts: {}", last_error.unwrap_or_else(|| "unknown error".into())))
    }

    async fn send_mqtt(&self, provider: &NotificationProvider, template: Option<&NotificationTemplate>, event: &Event) -> Result<()> {
        let config = &provider.configuration;
        let host = required_string(config, "host")?;
        let port = config.get("port").and_then(Value::as_u64).unwrap_or(1883) as u16;
        let topic = required_string(config, "topic")?;
        let client_id = format!("objexel-{}", Uuid::new_v4());
        let mut options = MqttOptions::new(client_id, host, port);
        options.set_keep_alive(Duration::from_secs(30));
        if let (Some(username), Some(password)) = (config.get("username").and_then(Value::as_str), config.get("password").and_then(Value::as_str)) { options.set_credentials(username, password); }
        let (client, mut event_loop) = AsyncClient::new(options, 10);
        let payload = serde_json::to_vec(&json!({ "event": event, "title": template.map(|t| t.subject.clone()), "body": template.map(|t| t.body.clone()) }))?;
        let qos = match config.get("qos").and_then(Value::as_u64).unwrap_or(0) { 2 => QoS::ExactlyOnce, 1 => QoS::AtLeastOnce, _ => QoS::AtMostOnce };
        client.publish(topic, qos, config.get("retain").and_then(Value::as_bool).unwrap_or(false), payload).await?;
        let _ = tokio::time::timeout(Duration::from_secs(5), event_loop.poll()).await;
        Ok(())
    }
}

fn required_string<'a>(config: &'a Value, key: &str) -> Result<&'a str> { config.get(key).and_then(Value::as_str).filter(|value| !value.trim().is_empty()).with_context(|| format!("notification configuration requires {key}")) }

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn provider(provider_type: &str, configuration: Value) -> NotificationProvider {
        NotificationProvider { id: Uuid::new_v4(), provider_type: provider_type.into(), enabled: true, configuration, created_at: Utc::now() }
    }

    #[test]
    fn validates_email_configuration() {
        let valid = provider("email", json!({"host":"smtp.example.com","port":587,"from":"alerts@example.com","to":"user@example.com","username":"alerts","password":"secret"}));
        assert!(NotificationService::validate_provider(&valid).is_ok());
        let invalid = provider("email", json!({"host":"smtp.example.com","from":"not-an-address","to":"user@example.com"}));
        assert!(NotificationService::validate_provider(&invalid).is_err());
    }

    #[test]
    fn rejects_unknown_provider_types() {
        assert!(NotificationService::validate_provider(&provider("carrier_pigeon", json!({}))).is_err());
    }
}
