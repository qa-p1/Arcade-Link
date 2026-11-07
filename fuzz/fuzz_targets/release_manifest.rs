#![no_main]

use arcade_link::release::{verify_signature, PublicKey, ReleaseManifest};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = ReleaseManifest::from_json(data);
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = PublicKey::parse(text);
        let key = PublicKey::parse("RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3").ok();
        let _ = verify_signature(data, text, key.as_slice());
    }
});
