use anyhow::{anyhow, Result};
use serde::Deserialize;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;

#[derive(Debug, Deserialize)]
struct LoginResponse {
    authentication_token: String,
}

#[derive(Debug, Deserialize)]
pub struct VerifyResponse {
    pub code: u32,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub string: String,
    pub binary: Vec<u8>,
    pub last_verified: Option<Instant>,
}

impl Token {
    pub fn new(token: String) -> Result<Token> {
        let binary = base64::decode(token.clone())?;
        let token = Token {
            string: token,
            binary,
            last_verified: None,
        };

        Ok(token)
    }
}

#[derive(Clone, Default)]
pub struct TwinklyApi {
    addr: String,
    token: Arc<RwLock<Option<Token>>>,
}

impl TwinklyApi {
    pub fn new(addr: String) -> TwinklyApi {
        TwinklyApi {
            addr,
            ..Default::default()
        }
    }

    pub async fn login(&self) -> Result<Token> {
        let addr = self.addr.clone();

        // Start by generating a new auth token
        let mut body = HashMap::new();

        // Just use a hardcoded challenge for now
        body.insert("challenge", "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=");

        let client = reqwest::Client::new();
        let resp = client
            .post(format!("http://{addr}/xled/v1/login"))
            .json(&body)
            .send()
            .await?
            .json::<LoginResponse>()
            .await?;

        let token = Token::new(resp.authentication_token)?;

        {
            let mut guard = self.token.write().await;
            *guard = Some(token.clone());
        }

        // Next we need to call this verify endpoint to make the token valid
        self.verify(token.clone()).await?;

        Ok(token)
    }

    pub async fn get_token(&self) -> Result<Token> {
        let token = self.token.read().await.clone();

        if let Some(token) = token {
            let valid = self.verify(token.clone()).await.is_ok();

            if !valid {
                let token = self.login().await?;
                Ok(token)
            } else {
                Ok(token)
            }
        } else {
            let token = self.login().await?;
            Ok(token)
        }
    }

    pub async fn verify(&self, token: Token) -> Result<()> {
        // Skip verify call if it has been successfully called within 1 sec
        if let Some(last_verified) = token.last_verified {
            if last_verified.elapsed() <= Duration::from_secs(10) {
                return Ok(());
            }
        }

        let mut token = token.clone();
        token.last_verified = Some(Instant::now());

        {
            let mut guard = self.token.write().await;
            *guard = Some(token.clone());
        }

        let addr = self.addr.clone();

        let client = reqwest::Client::new();
        let resp = client
            .get(format!("http://{addr}/xled/v1/verify"))
            .header("X-Auth-Token", token.string.clone())
            .send()
            .await?
            .json::<VerifyResponse>()
            .await?;

        if resp.code == 1000 {
            Ok(())
        } else {
            let code = resp.code;
            Err(anyhow!("verify returned error status code {code}"))
        }
    }

    pub async fn get_mode(&self) -> Result<String> {
        // Get device mode
        let addr = self.addr.clone();
        let token = self.get_token().await?;
        let client = reqwest::Client::new();
        let resp = client
            .get(format!("http://{addr}/xled/v1/led/mode"))
            .header("X-Auth-Token", token.string)
            .send()
            .await?
            .json::<HashMap<String, serde_json::Value>>()
            .await?;

        let mode: &str = resp.get("mode").unwrap().as_str().unwrap();

        Ok(mode.to_owned())
    }

    pub async fn set_mode(&self, mode: String) -> Result<()> {
        let addr = self.addr.clone();
        let token = self.get_token().await?;
        let client = reqwest::Client::new();
        let mut body = HashMap::new();
        body.insert("mode", mode);
        client
            .post(format!("http://{addr}/xled/v1/led/mode"))
            .header("X-Auth-Token", token.string)
            .json(&body)
            .send()
            .await?
            .json::<HashMap<String, serde_json::Value>>()
            .await?;

        Ok(())
    }
}
