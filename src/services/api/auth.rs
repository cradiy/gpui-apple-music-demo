use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UserIdentity {
    pub id: String,
    pub username: String,
    pub disabled: bool,
    #[serde(default)]
    pub avatar_url: Option<String>,
}
