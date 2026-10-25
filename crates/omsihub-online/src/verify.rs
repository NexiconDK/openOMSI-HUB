//! (NexiconDK) Who an access token is, for a server of the game's own that is not Supabase:
//! Bus Company Online's main server (`omsi_launcher_lib::company::server_api`) is handed a
//! player's access token once, at his login from the launcher, and asks the auth whose it is
//! (`GET /auth/v1/user`, which refuses a token that is spoilt, expired or signed out) and the
//! database for his display name (`profiles`, with the token itself: the player may read his own
//! row). The token is not kept, and nothing here logs.

use crate::codes::{self, Error};
use crate::http::{encode, Body, Http};
use crate::Endpoints;
use serde_json::Value;

/// An account the auth vouched for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Verified {
    pub user_id: String,
    /// Its display name (empty: no profile yet).
    pub display_name: String,
}

/// Whose `token` is (an access token of the project's): its account, or `SignedOut` when the
/// auth does not take it, `Offline` when it was not reached.
pub fn verify_access(ep: &Endpoints, token: &str) -> Result<Verified, Error> {
    let token = token.trim();
    if token.is_empty() || token.len() > 8192 {
        return Err(Error::SignedOut);
    }
    let http = Http::new(!ep.plain);
    let r = http.send("GET", &ep.auth("/user"), &ep.key, Some(token), Body::None, &[], false, false)?;
    if !r.ok() {
        return Err(if codes::is_unauthorized(r.status, &r.body) || matches!(r.status, 400 | 403 | 404) { Error::SignedOut } else { Error::Code("server") });
    }
    let user_id = r.body.get("id").and_then(Value::as_str).unwrap_or("").to_string();
    if user_id.is_empty() {
        return Err(Error::Code("server"));
    }
    // (the display name: the profile's, else the one the sign-up put in the account)
    let path = format!("/profiles?select=display_name&user_id=eq.{}&limit=1", encode(&user_id));
    let profile = http.send("GET", &ep.rest(&path), &ep.key, Some(token), Body::None, &[], false, false).ok().filter(|p| p.ok()).and_then(|p| p.body.as_array().and_then(|a| a.first()).and_then(|row| row.get("display_name")).and_then(Value::as_str).map(str::to_string));
    let display_name = profile.or_else(|| r.body.pointer("/user_metadata/display_name").and_then(Value::as_str).map(str::to_string)).unwrap_or_default().trim().to_string();
    Ok(Verified { user_id, display_name })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::Mock;
    use serde_json::json;

    #[test]
    fn a_token_is_its_account_or_none() {
        let m = Mock::start(|r| match (r.method.as_str(), r.path.split('?').next().unwrap_or(""), r.bearer()) {
            ("GET", "/auth/v1/user", Some("good")) => (200, vec![], json!({ "id": "u-1", "email": "a@b.example", "user_metadata": { "display_name": "Signed Up" } })),
            ("GET", "/auth/v1/user", Some("noprofile")) => (200, vec![], json!({ "id": "u-2", "user_metadata": { "display_name": "From Sign-up" } })),
            ("GET", "/auth/v1/user", _) => (401, vec![], json!({ "code": 401, "error_code": "bad_jwt" })),
            ("GET", "/rest/v1/profiles", Some("good")) => (200, vec![], json!([{ "display_name": "Mathias" }])),
            ("GET", "/rest/v1/profiles", _) => (200, vec![], json!([])),
            _ => (404, vec![], json!({})),
        });
        let ep = Endpoints::local(&m.base, "sb_publishable_test").unwrap();
        assert_eq!(verify_access(&ep, "good").unwrap(), Verified { user_id: "u-1".into(), display_name: "Mathias".into() });
        assert_eq!(verify_access(&ep, "noprofile").unwrap().display_name, "From Sign-up");
        assert_eq!(verify_access(&ep, "stolen").unwrap_err(), Error::SignedOut);
        assert_eq!(verify_access(&ep, " ").unwrap_err(), Error::SignedOut);
        // (every request with the project's key and the token, the profile asked for its user)
        let reqs = m.requests();
        assert!(reqs.iter().all(|r| r.header("apikey") == Some("sb_publishable_test")));
        assert!(reqs.iter().any(|r| r.path.contains("user_id=eq.u-1")));
        // (nobody there)
        let gone = Endpoints::local("http://127.0.0.1:9", "k").unwrap();
        assert_eq!(verify_access(&gone, "good").unwrap_err(), Error::Offline);
    }
}
