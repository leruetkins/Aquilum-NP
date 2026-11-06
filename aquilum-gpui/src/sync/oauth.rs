use crate::sync::remote::RemoteError;
use crate::sync::state::millis;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};
use url::Url;

mod loopback;

/// Loopback address for the OAuth redirect. Override at build time with
/// `VIBECODIUM_DROPBOX_REDIRECT_ADDRESS=127.0.0.1:47219` (or
/// `localhost:47219`) and register the exact `http://<address>/` URI in the
/// Dropbox app console.
pub const ADDRESS: &str = match option_env!("VIBECODIUM_DROPBOX_REDIRECT_ADDRESS") {
    Some(address) => address,
    None => "127.0.0.1:47219",
};

#[derive(Clone, Copy)]
pub struct Client {
    pub authorize: &'static str,
    pub token: &'static str,
    pub id: &'static str,
    pub secret: &'static str,
    pub params: &'static [(&'static str, &'static str)],
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Tokens {
    pub access: String,
    pub refresh: String,
    pub expires: u64,
}

#[derive(Deserialize)]
struct Grant {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
}

pub struct Login {
    pub url: String,
    client: Client,
    listener: TcpListener,
    verifier: String,
    state: String,
}

impl Client {
    pub fn login(&self) -> io::Result<Login> {
        let (verifier, state) = (random()?, random()?);
        let listener = TcpListener::bind(ADDRESS)?;
        listener.set_nonblocking(true)?;
        let url = self.loopback_url(&verifier, &state)?;
        Ok(Login { url, client: *self, listener, verifier, state })
    }

    fn loopback_url(&self, verifier: &str, state: &str) -> io::Result<String> {
        let mut url = Url::parse(self.authorize).map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        url.query_pairs_mut()
            .extend_pairs([
                ("response_type", "code"),
                ("client_id", self.id),
                ("redirect_uri", &redirect()),
                ("code_challenge", &challenge(verifier)),
                ("code_challenge_method", "S256"),
                ("state", state),
            ])
            .extend_pairs(self.params);
        Ok(url.into())
    }

    pub fn refresh(&self, agent: &ureq::Agent, refresh: &str) -> Result<Tokens, RemoteError> {
        self.grant(agent, &[("grant_type", "refresh_token"), ("refresh_token", refresh)], refresh)
    }

    fn grant(&self, agent: &ureq::Agent, form: &[(&str, &str)], refresh: &str) -> Result<Tokens, RemoteError> {
        let mut form = form.to_vec();
        form.push(("client_id", self.id));
        if !self.secret.is_empty() {
            form.push(("client_secret", self.secret));
        }
        let grant: Grant = match agent.post(self.token).send_form(&form) {
            Ok(response) => serde_json::from_reader(response.into_reader()).map_err(io::Error::from)?,
            Err(ureq::Error::Status(400 | 401, _)) => return Err(RemoteError::Auth),
            Err(ureq::Error::Transport(transport)) => return Err(RemoteError::transport(transport)),
            Err(error) => return Err(RemoteError::Failed(error.to_string())),
        };
        Ok(Tokens {
            access: grant.access_token,
            refresh: grant.refresh_token.unwrap_or_else(|| refresh.to_owned()),
            expires: millis(SystemTime::now()) + grant.expires_in.unwrap_or(3600) * 1000,
        })
    }
}

impl Login {
    pub fn finish(self, agent: &ureq::Agent, cancel: &AtomicBool) -> Result<Tokens, RemoteError> {
        let code = self.code(cancel)?;
        let redirect = redirect();
        let form = vec![
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("code_verifier", self.verifier.as_str()),
            ("redirect_uri", redirect.as_str()),
        ];
        self.client.grant(agent, &form, "")
    }

    fn code(&self, cancel: &AtomicBool) -> Result<String, RemoteError> {
        while !cancel.load(Ordering::Relaxed) {
            if let Some(code) = loopback::poll(&self.listener, &self.state)? {
                return Ok(code);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Err(RemoteError::Failed("sign-in cancelled".into()))
    }
}

fn redirect() -> String {
    format!("http://{ADDRESS}/")
}

fn random() -> io::Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).map_err(|error| io::Error::other(error.to_string()))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier))
}

/// Keeps the request/reply helpers shared with the loopback listener.
pub(crate) fn read_request(stream: &mut TcpStream) -> io::Result<String> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    let mut buffer = [0u8; 4096];
    let mut value = Vec::new();
    loop {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        value.extend_from_slice(&buffer[..read]);
        if value.windows(4).any(|window| window == b"\r\n\r\n") || value.len() > 16 * 1024 {
            break;
        }
    }
    Ok(String::from_utf8_lossy(&value).into_owned())
}

pub(crate) fn reply(stream: &mut TcpStream, message: &str) -> io::Result<()> {
    let body = message.as_bytes();
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        message
    )?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::challenge;

    #[test]
    fn challenge_matches_rfc_7636() {
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }
}
