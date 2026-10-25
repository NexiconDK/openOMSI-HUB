//! Plain HTTP for the launchers beside the game (Omsi-Hub's "Play together" of the bus company:
//! a friend's launcher talks to the boss's over the LAN, a VPN or the boss's Cloudflare tunnel):
//! a JSON body posted and the answer's text, and random bytes for a secret. The launcher's
//! crates take the HTTP client and the system's randomness from here, so that they need no
//! crates of their own for it.

use std::time::Duration;

/// Post `body` (JSON) to `url` (`http://…` or `https://…`) and give the answer's text, within
/// `timeout`. An answer that is not 200 is an error with its status.
pub fn post_json(url: &str, body: &str, timeout: Duration) -> Result<String, String> {
    let agent = ureq::AgentBuilder::new().timeout(timeout).user_agent("openOMSI").build();
    match agent.post(url).set("Content-Type", "application/json").send_string(body) {
        Ok(r) => r.into_string().map_err(|e| e.to_string()),
        Err(ureq::Error::Status(code, _)) => Err(format!("HTTP {code}")),
        Err(e) => Err(e.to_string()),
    }
}

/// `N` bytes from the system's secure random source (a session's secret).
pub fn random_bytes<const N: usize>() -> [u8; N] {
    use ring::rand::SecureRandom;
    let mut out = [0u8; N];
    if ring::rand::SystemRandom::new().fill(&mut out).is_err() {
        // (no secure source: the hasher's random keys and the clock, as the session ids)
        for chunk in out.chunks_mut(6) {
            let v = crate::random_session_id().to_be_bytes();
            chunk.copy_from_slice(&v[2..2 + chunk.len()]);
        }
    }
    out
}

/// HMAC-SHA256 of `msg` under `key` (Bus Company Online's tickets: `omsi_launcher_lib`'s
/// `company::online::ticket`).
pub fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    let tag = ring::hmac::sign(&ring::hmac::Key::new(ring::hmac::HMAC_SHA256, key), msg);
    let mut out = [0u8; 32];
    out.copy_from_slice(tag.as_ref());
    out
}

/// Whether `tag` is the HMAC-SHA256 of `msg` under `key` (compared in constant time).
pub fn hmac_sha256_ok(key: &[u8], msg: &[u8], tag: &[u8]) -> bool {
    ring::hmac::verify(&ring::hmac::Key::new(ring::hmac::HMAC_SHA256, key), msg, tag).is_ok()
}

/// SHA-256 of `bytes` (a key kept as its hash).
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    use sha2::Digest;
    sha2::Sha256::digest(bytes).into()
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_tag_is_checked() {
        let tag = super::hmac_sha256(b"key", b"message");
        assert!(super::hmac_sha256_ok(b"key", b"message", &tag));
        assert!(!super::hmac_sha256_ok(b"key", b"messagf", &tag));
        assert!(!super::hmac_sha256_ok(b"kez", b"message", &tag));
        assert_eq!(super::sha256(b"abc")[..4], [0xba, 0x78, 0x16, 0xbf]);
    }

    #[test]
    fn random_bytes_differ() {
        let a: [u8; 16] = super::random_bytes();
        let b: [u8; 16] = super::random_bytes();
        assert_ne!(a, b);
    }
}
