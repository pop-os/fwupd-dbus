use crypto_hash::{Algorithm, Hasher};
use hex_view::HexView;
use std::io::{self, Read};

/// Based on libfwupd/fwupd-common.c
pub fn checksum_guess_kind(checksum: &str) -> Algorithm {
    match checksum.len() {
        32 => Algorithm::MD5,
        40 => Algorithm::SHA1,
        64 => Algorithm::SHA256,
        128 => Algorithm::SHA512,
        _ => Algorithm::SHA1,
    }
}

pub fn validate_checksum<R: Read>(
    data: &mut R,
    checksum: &str,
    alg: Algorithm,
) -> io::Result<bool> {
    let mut hasher = Hasher::new(alg);
    io::copy(data, &mut hasher)?;
    let digest = format!("{:x}", HexView::from(hasher.finish().as_slice()));
    Ok(checksum == digest.as_str())
}
