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
    pub fn new() -> Self { Self { http: Client::new() } }

    pub async fn send(&self, provider: &NotificationProvider, template: Option<&NotificationTemplate>, event: &Event) -> Result<()> {
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
        let transport = match (username, password) {
            (Some(username), Some(password)) => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)?.port(port).credentials(lettre::transport::smtp::authentication::Credentials::new(username.into(), password.into())).build(),
            _ => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host).port(port).build(),
        };
        transport.send(message).await.context("send email")?;
        Ok(())
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

fn required_string(config: &Value, key: &str) -> Result<&str> { config.get(key).and_then(Value::as_str).filter(|value| !value.trim().is_empty()).with_context(|| format!("notification configuration requires {key}")) }
