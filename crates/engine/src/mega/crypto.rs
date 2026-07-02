//! MEGA crypto helpers.
//!
//! MEGA encodes keys and handles with URL-safe base64 *without* padding. In the
//! Real-Debrid-first design we only need this for decrypting folder *metadata*
//! (node names/keys) — file *contents* arrive already-decrypted from RD, so the
//! heavier AES-CTR/MAC content pipeline is deferred until/unless a native
//! transport is added.

use aes::cipher::block_padding::NoPadding;
use aes::cipher::generic_array::GenericArray;
use aes::cipher::{BlockDecrypt, BlockDecryptMut, KeyInit, KeyIvInit};
use aes::Aes128;
use base64::Engine;

use crate::{Error, Result};

type Aes128CbcDec = cbc::Decryptor<Aes128>;

/// Decode MEGA's URL-safe, unpadded base64.
pub fn b64decode(s: &str) -> Result<Vec<u8>> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(s.trim())
        .map_err(|e| Error::Other(format!("base64 decode: {e}")))
}

/// Encode bytes as MEGA's URL-safe, unpadded base64.
pub fn b64encode(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// AES-128-ECB decrypt, used to unwrap MEGA node keys with the folder master
/// key. `data` length must be a multiple of 16 bytes.
pub fn aes_ecb_decrypt(key: &[u8; 16], data: &[u8]) -> Vec<u8> {
    let cipher = Aes128::new(GenericArray::from_slice(key));
    let mut out = data.to_vec();
    for chunk in out.chunks_mut(16) {
        cipher.decrypt_block(GenericArray::from_mut_slice(chunk));
    }
    out
}

/// Fold a 32-byte MEGA file key into its 16-byte AES key (XOR of the two halves).
/// File keys are stored as `key ^ nonce/mac`; the usable key is `k[..16] ^ k[16..]`.
pub fn unpack_file_key(k32: &[u8]) -> [u8; 16] {
    let mut out = [0u8; 16];
    for i in 0..16 {
        out[i] = k32[i] ^ k32[i + 16];
    }
    out
}

/// Decrypt a node's `a` (attributes) blob and return the inner JSON string.
///
/// MEGA attributes are AES-128-CBC (zero IV, no padding), and the plaintext is
/// `"MEGA"` followed by a null-padded JSON object such as `{"n":"filename"}`.
pub fn decrypt_attributes(key: &[u8; 16], attr_b64: &str) -> Result<String> {
    let mut data = b64decode(attr_b64)?;
    if data.is_empty() || data.len() % 16 != 0 {
        return Err(Error::Other("attribute blob not block-aligned".into()));
    }

    let key_ga = GenericArray::from_slice(key);
    let iv = [0u8; 16];
    let iv_ga = GenericArray::from_slice(&iv);

    let plain = Aes128CbcDec::new(key_ga, iv_ga)
        .decrypt_padded_mut::<NoPadding>(&mut data)
        .map_err(|e| Error::Other(format!("cbc decrypt: {e}")))?;

    let plain = plain.strip_prefix(b"MEGA").unwrap_or(plain);
    let end = plain.iter().position(|&b| b == 0).unwrap_or(plain.len());
    Ok(String::from_utf8_lossy(&plain[..end]).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_roundtrip() {
        let data = b"\x00\x01\x02\xff\xfe mega";
        let encoded = b64encode(data);
        assert!(!encoded.contains('='), "MEGA base64 is unpadded");
        assert_eq!(b64decode(&encoded).unwrap(), data);
    }

    #[test]
    fn aes_ecb_matches_fips197_vector() {
        // FIPS-197 AES-128 known-answer test.
        let key: [u8; 16] = [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
            0x0e, 0x0f,
        ];
        let ciphertext: [u8; 16] = [
            0x69, 0xc4, 0xe0, 0xd8, 0x6a, 0x7b, 0x04, 0x30, 0xd8, 0xcd, 0xb7, 0x80, 0x70, 0xb4,
            0xc5, 0x5a,
        ];
        let expected: [u8; 16] = [
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
            0xee, 0xff,
        ];
        assert_eq!(aes_ecb_decrypt(&key, &ciphertext), expected);
    }

    #[test]
    fn unpack_file_key_xors_halves() {
        let mut k = [0u8; 32];
        k[0] = 0xaa;
        k[16] = 0x0f;
        k[31] = 0x77;
        let unpacked = unpack_file_key(&k);
        assert_eq!(unpacked[0], 0xaa ^ 0x0f);
        assert_eq!(unpacked[15], 0x77);
    }
}
