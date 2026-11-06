use crate::sync::oauth::{Client, Tokens};
use crate::sync::remote::RemoteError;
use crate::sync::state::millis;
use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

const MARGIN_MS: u64 = 60_000;

pub struct Account {
    client: Client,
    agent: ureq::Agent,
    path: PathBuf,
    tokens: Mutex<Tokens>,
}

impl Clone for Account {
    fn clone(&self) -> Self {
        let tokens = self
            .tokens
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        Self {
            client: self.client,
            agent: self.agent.clone(),
            path: self.path.clone(),
            tokens: Mutex::new(tokens),
        }
    }
}

impl Account {
    pub fn new(client: Client, agent: ureq::Agent, path: PathBuf, tokens: Tokens) -> io::Result<Self> {
        save(&path, &tokens)?;
        Ok(Self { client, agent, path, tokens: Mutex::new(tokens) })
    }

    pub fn open(client: Client, agent: ureq::Agent, path: PathBuf) -> io::Result<Option<Self>> {
        let tokens = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        Ok(Some(Self { client, agent, path, tokens: Mutex::new(tokens) }))
    }

    pub fn token(&self) -> Result<String, RemoteError> {
        let mut tokens = self.tokens.lock().unwrap_or_else(|error| error.into_inner());
        if tokens.expires > millis(SystemTime::now()) + MARGIN_MS {
            return Ok(tokens.access.clone());
        }
        self.refresh(&mut tokens)
    }

    pub fn renew(&self) -> Result<String, RemoteError> {
        let mut tokens = self.tokens.lock().unwrap_or_else(|error| error.into_inner());
        self.refresh(&mut tokens)
    }

    fn refresh(&self, tokens: &mut Tokens) -> Result<String, RemoteError> {
        *tokens = self.client.refresh(&self.agent, &tokens.refresh)?;
        save(&self.path, tokens)?;
        Ok(tokens.access.clone())
    }
}

fn save(path: &Path, tokens: &Tokens) -> io::Result<()> {
    let json = serde_json::to_string(tokens)?;
    crate::sync::write::write_bytes(path, json.as_bytes())
}
