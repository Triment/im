//! Self-signed certificate management for development / pinned deployments.
//! Clients pin `cert.der` (see `xmp-client`), so no CA is required.

use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use std::path::Path;

pub fn load_or_create(dir: &Path, names: Vec<String>) -> anyhow::Result<(CertificateDer<'static>, PrivatePkcs8KeyDer<'static>)> {
    let (c, k) = (dir.join("cert.der"), dir.join("key.der"));
    if c.exists() && k.exists() {
        return Ok((CertificateDer::from(std::fs::read(c)?), PrivatePkcs8KeyDer::from(std::fs::read(k)?)));
    }
    let ck = rcgen::generate_simple_self_signed(names)?;
    let cert = ck.cert.der().to_vec();
    let key = ck.key_pair.serialize_der();
    std::fs::write(&c, &cert)?;
    std::fs::write(&k, &key)?;
    Ok((CertificateDer::from(cert), PrivatePkcs8KeyDer::from(key)))
}
