use argon2::{password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString}, Argon2};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role { Administrator, Operator, Viewer }

impl Role {
    pub fn as_str(self) -> &'static str { match self { Self::Administrator => "administrator", Self::Operator => "operator", Self::Viewer => "viewer" } }
    pub fn can_manage_users(self) -> bool { matches!(self, Self::Administrator) }
    pub fn can_write(self) -> bool { !matches!(self, Self::Viewer) }
}

impl TryFrom<&str> for Role {
    type Error = anyhow::Error;
    fn try_from(value: &str) -> Result<Self, Self::Error> { match value { "administrator" => Ok(Self::Administrator), "operator" => Ok(Self::Operator), "viewer" => Ok(Self::Viewer), _ => anyhow::bail!("unknown role") } }
}

#[derive(Debug, Clone)]
pub struct SessionUser { pub session_id: Uuid, pub user: User, pub csrf_token_hash: String }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub email: Option<String>,
    pub role: Role,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateUser { pub username: String, pub email: Option<String>, pub password: String, pub role: Role }

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateUser { pub email: Option<String>, pub role: Option<Role>, pub enabled: Option<bool>, pub password: Option<String> }

#[derive(Debug, Clone, Deserialize)]
pub struct LoginRequest { pub username: String, pub password: String }

#[derive(Debug, Clone, Serialize)]
pub struct AuthResponse { pub user: User, pub csrf_token: String }

pub fn validate_password(password: &str) -> anyhow::Result<()> {
    if password.chars().count() < 12 { anyhow::bail!("password must contain at least 12 characters"); }
    if password.chars().count() > 256 { anyhow::bail!("password is too long"); }
    Ok(())
}

pub fn hash_password(password: &str) -> anyhow::Result<String> {
    validate_password(password)?;
    Ok(Argon2::default().hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng))?.to_string())
}

pub fn verify_password(password: &str, encoded: &str) -> bool {
    PasswordHash::new(encoded).map(|hash| Argon2::default().verify_password(password.as_bytes(), &hash).is_ok()).unwrap_or(false)
}

pub fn generate_token() -> String {
    let bytes: [u8; 32] = rand::random();
    hex::encode(bytes)
}

pub fn digest_token(token: &str) -> String { hex::encode(Sha256::digest(token.as_bytes())) }

pub fn generate_csrf_token() -> String { generate_token() }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn argon2id_round_trip() {
        let password: String = std::iter::repeat_n('a', 32).collect();
        let wrong_password: String = std::iter::repeat_n('b', 32).collect();
        let hash = hash_password(&password).unwrap();
        assert!(verify_password(&password, &hash));
        assert!(!verify_password(&wrong_password, &hash));
    }
    #[test]
    fn tokens_are_unpredictable_and_digestable() { let token = generate_token(); assert_eq!(token.len(), 64); assert_ne!(token, generate_token()); assert_eq!(digest_token(&token).len(), 64); }
    #[test]
    fn roles_have_expected_permissions() { assert!(Role::Administrator.can_manage_users()); assert!(Role::Operator.can_write()); assert!(!Role::Viewer.can_write()); }
}
