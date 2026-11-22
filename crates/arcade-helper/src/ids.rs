//! Fixed names. The helper answers only the Arcade browser extension, identified by these IDs.

/// The helper's Link id.
pub const HELPER_ID: &str = "arcade.helper";

/// The native messaging host name both browser families look up.
pub const HOST_NAME: &str = "dev.arcade.helper";

/// The Arcade extension's ID in Chromium browsers (Chrome, Edge, Brave, Vivaldi, Chromium). An unpacked extension
/// gets this ID from the `key` in its manifest ([`CHROME_EXTENSION_KEY`]), so it is the same on every computer.
pub const CHROME_EXTENSION_ID: &str = "dbghmpdffbagedciechjkmcmjahcdmad";

/// The public key (base64 DER) that goes into the extension's `manifest.json` as `key`. Only the public half
/// exists: an unpacked extension needs nothing else.
pub const CHROME_EXTENSION_KEY: &str = "MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAlLor9dPJ2mBjMJesrDZ0ggDEh50Edz0kl/xEuLb4yLxNy5IFn1pD7xs0yaaUjFijeJDOA93T+eyfZjVCp1m8C6D8UUdLODZNz8FXd69gIdqnZ91Ctdqj+NP2l/vG7XOr9ceg8L/bPSpvMsFyolQTQ+e6FuR/Cr/KLmaBpNlvXBorZVlJT+8ZZW2frjbIutXqXAbgkCy7ejaqtO86hzVMFF68qOh4vPQQa5BvHbHNbgRjFy0/3dIGi6yDqEhIwMJLdG8jXTpI4AvUOeCmCo5b7UItAPfo+MKWr+5H+5qKuZtUj0AIVKwXejsIEf/v0jNA9qc5fbkSSkMB6TB/N7DoMwIDAQAB";

/// The Arcade extension's add-on ID in Firefox-family browsers (Firefox, Zen, LibreWolf, Floorp).
pub const FIREFOX_ADDON_ID: &str = "{77b33901-3817-4946-8df0-1914a2850188}";

/// The Chrome origin as the browser passes it to the host.
pub fn chrome_origin() -> String {
    format!("chrome-extension://{CHROME_EXTENSION_ID}/")
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_chrome_id_is_derived_from_the_key() {
        // Chrome: the first 32 hex digits of SHA-256(DER public key), each mapped 0-f → a-p.
        use base64::Engine as _;
        let der = base64::engine::general_purpose::STANDARD.decode(super::CHROME_EXTENSION_KEY).unwrap();
        assert_eq!(der.len(), 294, "an RSA-2048 SubjectPublicKeyInfo");
        use sha2::Digest as _;
        let hash = sha2::Sha256::digest(&der);
        let id: String = hash[..16].iter().flat_map(|b| [b >> 4, b & 15]).map(|n| (b'a' + n) as char).collect();
        assert_eq!(id, super::CHROME_EXTENSION_ID);
    }
}
