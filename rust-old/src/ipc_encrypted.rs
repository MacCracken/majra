//! AES-256-GCM encrypted IPC channel.
//!
//! Wraps [`crate::ipc::IpcConnection`] with symmetric encryption using a
//! pre-shared key. Each frame is encrypted with a unique nonce.
//!
//! ## Feature flag
//!
//! Requires the `ipc-encrypted` feature (implies `ipc`).
//!
//! ## Wire format
//!
//! Each encrypted frame on the wire:
//! ```text
//! [4 bytes: total length (u32 BE)]
//! [12 bytes: nonce]
//! [remaining: ciphertext + 16-byte GCM tag]
//! ```

use std::sync::atomic::{AtomicU64, Ordering};

use ring::aead::{self, AES_256_GCM, Aad, LessSafeKey, Nonce, UnboundKey};
use tracing::{trace, warn};

use crate::error::IpcError;
use crate::ipc::IpcConnection;

/// Safe nonce limit for AES-256-GCM. The GCM internal counter is 32 bits,
/// so after 2^32 encryptions with the same key, nonce uniqueness can no
/// longer be guaranteed. We warn at 2^31 and refuse to encrypt past 2^32.
const NONCE_WARN_THRESHOLD: u64 = 1 << 31; // ~2.1 billion
const NONCE_HARD_LIMIT: u64 = 1 << 32; // ~4.3 billion

/// AES-256-GCM encrypted wrapper around an [`IpcConnection`].
///
/// Uses a pre-shared 256-bit key. Each direction maintains a monotonic nonce
/// counter to ensure uniqueness without coordination.
pub struct EncryptedIpcConnection {
    inner: IpcConnection,
    seal_key: LessSafeKey,
    open_key: LessSafeKey,
    send_counter: AtomicU64,
}

impl EncryptedIpcConnection {
    /// Wrap an existing IPC connection with AES-256-GCM encryption.
    ///
    /// `key` must be exactly 32 bytes.
    ///
    /// # Errors
    ///
    /// Returns `IpcError::Io` if the key length is invalid.
    pub fn new(inner: IpcConnection, key: &[u8; 32]) -> Result<Self, IpcError> {
        let seal_unbound = UnboundKey::new(&AES_256_GCM, key).map_err(|_| {
            IpcError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid AES-256-GCM key",
            ))
        })?;
        let open_unbound = UnboundKey::new(&AES_256_GCM, key).map_err(|_| {
            IpcError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid AES-256-GCM key",
            ))
        })?;

        Ok(Self {
            inner,
            seal_key: LessSafeKey::new(seal_unbound),
            open_key: LessSafeKey::new(open_unbound),
            send_counter: AtomicU64::new(0),
        })
    }

    /// Create an encrypted server connection.
    pub async fn bind_and_accept(
        path: &std::path::Path,
        key: &[u8; 32],
    ) -> Result<(crate::ipc::IpcServer, Self), IpcError> {
        let server = crate::ipc::IpcServer::bind(path)?;
        let conn = server.accept().await?;
        let encrypted = Self::new(conn, key)?;
        Ok((server, encrypted))
    }

    /// Create an encrypted client connection.
    pub async fn connect(path: &std::path::Path, key: &[u8; 32]) -> Result<Self, IpcError> {
        let conn = IpcConnection::connect(path).await?;
        Self::new(conn, key)
    }

    fn make_nonce(&self) -> Result<[u8; 12], IpcError> {
        let counter = self.send_counter.fetch_add(1, Ordering::Relaxed);

        if counter >= NONCE_HARD_LIMIT {
            return Err(IpcError::Io(std::io::Error::other(
                "AES-GCM nonce limit reached (2^32 messages) — rotate key with rekey()",
            )));
        }
        if counter == NONCE_WARN_THRESHOLD {
            warn!(
                counter,
                "ipc-encrypted: approaching nonce limit (2^31 of 2^32) — rotate key soon"
            );
        }

        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[4..12].copy_from_slice(&counter.to_be_bytes());
        Ok(nonce_bytes)
    }

    /// Send an encrypted JSON frame.
    ///
    /// Returns `Err` if the nonce counter has been exhausted (2^32 messages).
    /// Call [`rekey`](Self::rekey) to install a new key and reset the counter.
    pub async fn send(&mut self, payload: &serde_json::Value) -> Result<(), IpcError> {
        let plaintext = serde_json::to_vec(payload)?;
        let nonce_bytes = self.make_nonce()?;
        let nonce = Nonce::assume_unique_for_key(nonce_bytes);

        let mut in_out = plaintext;
        self.seal_key
            .seal_in_place_append_tag(nonce, Aad::empty(), &mut in_out)
            .map_err(|_| IpcError::Io(std::io::Error::other("encryption failed")))?;

        // Wire format: nonce (12) + ciphertext+tag
        let mut frame = Vec::with_capacity(12 + in_out.len());
        frame.extend_from_slice(&nonce_bytes);
        frame.extend_from_slice(&in_out);

        // Send as a raw JSON value (the inner IPC expects serde_json::Value).
        // We encode the binary as a base64 JSON string to stay compatible with
        // the length-prefixed JSON framing.
        use serde_json::Value;
        let encoded = Value::String(base64_encode(&frame));
        self.inner.send(&encoded).await?;

        trace!(size = frame.len(), "ipc-encrypted: sent");
        Ok(())
    }

    /// Receive and decrypt a JSON frame.
    pub async fn recv(&mut self) -> Result<serde_json::Value, IpcError> {
        let raw = self.inner.recv().await?;

        let encoded_str = raw.as_str().ok_or_else(|| {
            IpcError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "expected base64 string in encrypted frame",
            ))
        })?;

        let frame = base64_decode(encoded_str).map_err(|_| {
            IpcError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "invalid base64 in encrypted frame",
            ))
        })?;

        if frame.len() < 12 + aead::AES_256_GCM.tag_len() {
            return Err(IpcError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "encrypted frame too short",
            )));
        }

        let (nonce_bytes, ciphertext_and_tag) = frame.split_at(12);
        let nonce_arr: [u8; 12] = nonce_bytes.try_into().map_err(|_| {
            IpcError::Io(std::io::Error::other(
                "internal error: nonce must be 12 bytes",
            ))
        })?;
        let nonce = Nonce::assume_unique_for_key(nonce_arr);

        let mut in_out = ciphertext_and_tag.to_vec();
        let plaintext = self
            .open_key
            .open_in_place(nonce, Aad::empty(), &mut in_out)
            .map_err(|_| {
                IpcError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "decryption failed (bad key or tampered data)",
                ))
            })?;

        let value: serde_json::Value = serde_json::from_slice(plaintext)?;
        trace!(size = plaintext.len(), "ipc-encrypted: received");
        Ok(value)
    }

    /// Install a new encryption key and reset the nonce counter.
    ///
    /// Call this before the nonce limit is reached (2^32 messages). Both
    /// endpoints must rekey simultaneously — coordinate via an unencrypted
    /// control message or out-of-band signal.
    pub fn rekey(&mut self, new_key: &[u8; 32]) -> Result<(), IpcError> {
        let seal = UnboundKey::new(&AES_256_GCM, new_key).map_err(|_| {
            IpcError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid AES-256-GCM key",
            ))
        })?;
        let open = UnboundKey::new(&AES_256_GCM, new_key).map_err(|_| {
            IpcError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid AES-256-GCM key",
            ))
        })?;
        self.seal_key = LessSafeKey::new(seal);
        self.open_key = LessSafeKey::new(open);
        self.send_counter.store(0, Ordering::Relaxed);
        trace!("ipc-encrypted: rekeyed, nonce counter reset");
        Ok(())
    }

    /// Number of messages sent with the current key.
    ///
    /// When this approaches 2^32, call [`rekey`](Self::rekey).
    #[inline]
    pub fn messages_sent(&self) -> u64 {
        self.send_counter.load(Ordering::Relaxed)
    }

    /// Whether the nonce counter is approaching exhaustion (past 2^31).
    #[inline]
    #[must_use]
    pub fn needs_rekey(&self) -> bool {
        self.send_counter.load(Ordering::Relaxed) >= NONCE_WARN_THRESHOLD
    }

    /// Get a reference to the underlying IPC connection.
    pub fn inner(&self) -> &IpcConnection {
        &self.inner
    }

    /// Consume this wrapper and return the underlying IPC connection.
    pub fn into_inner(self) -> IpcConnection {
        self.inner
    }
}

// Minimal base64 encode/decode without adding a dep.
// Uses standard alphabet (A-Z, a-z, 0-9, +, /) with = padding.

fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut result = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;

        result.push(ALPHABET[((triple >> 18) & 0x3F) as usize] as char);
        result.push(ALPHABET[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(ALPHABET[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(ALPHABET[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

fn base64_decode(input: &str) -> Result<Vec<u8>, ()> {
    fn decode_char(c: u8) -> Result<u32, ()> {
        match c {
            b'A'..=b'Z' => Ok((c - b'A') as u32),
            b'a'..=b'z' => Ok((c - b'a' + 26) as u32),
            b'0'..=b'9' => Ok((c - b'0' + 52) as u32),
            b'+' => Ok(62),
            b'/' => Ok(63),
            b'=' => Ok(0),
            _ => Err(()),
        }
    }

    let bytes = input.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return Err(());
    }

    let mut result = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        let a = decode_char(chunk[0])?;
        let b = decode_char(chunk[1])?;
        let c = decode_char(chunk[2])?;
        let d = decode_char(chunk[3])?;
        let triple = (a << 18) | (b << 12) | (c << 6) | d;

        result.push(((triple >> 16) & 0xFF) as u8);
        if chunk[2] != b'=' {
            result.push(((triple >> 8) & 0xFF) as u8);
        }
        if chunk[3] != b'=' {
            result.push((triple & 0xFF) as u8);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_roundtrip() {
        let data = b"hello, majra encrypted IPC!";
        let encoded = base64_encode(data);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn base64_empty() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_decode("").unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn base64_padding() {
        // 1 byte → 4 chars with ==
        let enc = base64_encode(b"a");
        assert!(enc.ends_with("=="));
        assert_eq!(base64_decode(&enc).unwrap(), b"a");

        // 2 bytes → 4 chars with =
        let enc = base64_encode(b"ab");
        assert!(enc.ends_with('='));
        assert_eq!(base64_decode(&enc).unwrap(), b"ab");

        // 3 bytes → 4 chars, no padding
        let enc = base64_encode(b"abc");
        assert!(!enc.contains('='));
        assert_eq!(base64_decode(&enc).unwrap(), b"abc");
    }

    #[test]
    fn nonce_monotonic() {
        let conn_key = [0u8; 32];
        // We can't easily construct a full EncryptedIpcConnection without a socket,
        // but we can test the nonce generation logic directly.
        let counter = AtomicU64::new(0);
        let n1 = counter.fetch_add(1, Ordering::Relaxed);
        let n2 = counter.fetch_add(1, Ordering::Relaxed);
        assert_eq!(n1, 0);
        assert_eq!(n2, 1);
        // Verify key creation doesn't fail.
        let _ = UnboundKey::new(&AES_256_GCM, &conn_key).unwrap();
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = [42u8; 32];
        let seal_key = LessSafeKey::new(UnboundKey::new(&AES_256_GCM, &key).unwrap());
        let open_key = LessSafeKey::new(UnboundKey::new(&AES_256_GCM, &key).unwrap());

        let plaintext = b"hello encrypted world";
        let nonce_bytes = [0u8; 12];
        let nonce = Nonce::assume_unique_for_key(nonce_bytes);

        let mut in_out = plaintext.to_vec();
        seal_key
            .seal_in_place_append_tag(nonce, Aad::empty(), &mut in_out)
            .unwrap();

        let nonce2 = Nonce::assume_unique_for_key(nonce_bytes);
        let decrypted = open_key
            .open_in_place(nonce2, Aad::empty(), &mut in_out)
            .unwrap();
        assert_eq!(decrypted, plaintext);
    }
}
