use anyhow::{anyhow, Result};
use base64::Engine;
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
    pub last_verified: Option<Instant>,
}

impl Token {
    pub fn new(token: String) -> Result<Token> {
        base64::engine::general_purpose::STANDARD.decode(&token)?;

        let token = Token {
            string: token,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Minimal fake of the Twinkly local HTTP API. Records the requests it
    /// receives as "METHOD /path body".
    async fn fake_twinkly() -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let requests = Arc::new(Mutex::new(vec![]));

        let log = requests.clone();
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let log = log.clone();

                tokio::spawn(async move {
                    let mut buf = Vec::new();
                    let mut chunk = [0; 1024];
                    let (head, body) = loop {
                        let n = socket.read(&mut chunk).await.unwrap();
                        buf.extend_from_slice(&chunk[..n]);
                        let text = String::from_utf8_lossy(&buf).to_string();
                        if let Some((head, body)) = text.split_once("\r\n\r\n") {
                            let len = head
                                .lines()
                                .find_map(|l| {
                                    l.to_ascii_lowercase()
                                        .strip_prefix("content-length: ")
                                        .map(|v| v.trim().parse::<usize>().unwrap())
                                })
                                .unwrap_or(0);
                            if body.len() >= len {
                                break (head.to_string(), body.to_string());
                            }
                        }
                    };

                    let request_line = head.lines().next().unwrap();
                    let mut parts = request_line.split(' ');
                    let (method, path) = (parts.next().unwrap(), parts.next().unwrap());
                    log.lock()
                        .unwrap()
                        .push(format!("{method} {path} {body}").trim().to_string());

                    let response = match (method, path) {
                        ("POST", "/xled/v1/login") => {
                            r#"{"authentication_token":"dG9rZW4=","challenge-response":"x","code":1000}"#
                        }
                        ("GET", "/xled/v1/led/mode") => r#"{"mode":"movie","code":1000}"#,
                        _ => r#"{"code":1000}"#,
                    };

                    let reply = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        response.len(),
                        response
                    );
                    socket.write_all(reply.as_bytes()).await.unwrap();
                });
            }
        });

        (addr, requests)
    }

    #[test]
    fn token_must_be_base64() {
        assert!(Token::new("dG9rZW4=".to_string()).is_ok());
        assert!(Token::new("not base64!".to_string()).is_err());
    }

    #[tokio::test]
    async fn logs_in_and_reads_mode() {
        let (addr, requests) = fake_twinkly().await;
        let api = TwinklyApi::new(addr);

        assert_eq!(api.get_mode().await.unwrap(), "movie");

        let requests = requests.lock().unwrap().clone();
        assert!(requests[0].starts_with("POST /xled/v1/login {\"challenge\":"));
        assert_eq!(requests[1], "GET /xled/v1/verify");
        assert_eq!(requests[2], "GET /xled/v1/led/mode");
    }

    #[tokio::test]
    async fn sets_mode() {
        let (addr, requests) = fake_twinkly().await;
        let api = TwinklyApi::new(addr);

        api.set_mode("off".to_string()).await.unwrap();

        let requests = requests.lock().unwrap().clone();
        assert_eq!(
            requests.last().unwrap(),
            r#"POST /xled/v1/led/mode {"mode":"off"}"#
        );
    }
}
