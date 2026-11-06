use super::{read_request, reply, ADDRESS};
use crate::sync::remote::RemoteError;
use std::collections::HashMap;
use std::io::ErrorKind;
use std::net::TcpListener;
use url::Url;

pub(super) fn poll(listener: &TcpListener, state: &str) -> Result<Option<String>, RemoteError> {
    match listener.accept() {
        Ok((mut stream, _)) => {
            let request = match read_request(&mut stream) {
                Ok(request) => request,
                Err(error)
                    if error.kind() == ErrorKind::WouldBlock || error.kind() == ErrorKind::TimedOut =>
                {
                    return Ok(None);
                }
                Err(error) => return Err(error.into()),
            };
            let target = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/");
            let query: HashMap<String, String> = Url::parse(&format!("http://{ADDRESS}{target}"))
                .map(|url| url.query_pairs().into_owned().collect())
                .unwrap_or_default();
            let (message, outcome) = match (query.get("code"), query.get("error")) {
                (Some(code), _) if query.get("state").map(String::as_str) == Some(state) => {
                    ("Sign-in complete. You can close this tab.", Ok(Some(code.clone())))
                }
                (Some(_), _) => (
                    "Sign-in failed. Return to Vibecodium.",
                    Err(RemoteError::Failed("sign-in response did not match the request".into())),
                ),
                (None, Some(error)) => (
                    "Sign-in failed. Return to Vibecodium.",
                    Err(RemoteError::Failed(format!("cloud refused sign-in: {error}"))),
                ),
                (None, None) => ("", Ok(None)),
            };
            let _ = reply(&mut stream, message);
            outcome
        }
        Err(error) if error.kind() == ErrorKind::WouldBlock => Ok(None),
        Err(error) => Err(error.into()),
    }
}
