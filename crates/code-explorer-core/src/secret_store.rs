//! Small cross-platform helper for secrets persisted by Code Explorer.
//!
//! On Windows, payloads are protected with DPAPI so the cached ChatGPT OAuth
//! tokens are bound to the current Windows user. On other platforms this module
//! deliberately leaves bytes unchanged; callers should still set restrictive
//! file permissions.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SecretStoreError {
    #[cfg(windows)]
    #[error("Windows DPAPI failed: {0}")]
    WindowsDpapi(#[from] std::io::Error),
}

#[cfg(windows)]
const DPAPI_MAGIC: &[u8] = b"CODE-EXPLORER-DPAPI-V1\n";

/// Encode a secret payload for disk storage.
pub fn encode_secret_for_storage(plaintext: &[u8]) -> Result<Vec<u8>, SecretStoreError> {
    #[cfg(windows)]
    {
        let encrypted = windows_dpapi::protect(plaintext)?;
        let mut stored = Vec::with_capacity(DPAPI_MAGIC.len() + encrypted.len());
        stored.extend_from_slice(DPAPI_MAGIC);
        stored.extend_from_slice(&encrypted);
        Ok(stored)
    }

    #[cfg(not(windows))]
    {
        Ok(plaintext.to_vec())
    }
}

/// Decode a secret payload read from disk.
///
/// Windows keeps backward compatibility with the old plaintext JSON file:
/// payloads without the DPAPI magic prefix are returned unchanged so callers can
/// parse and immediately re-save them encrypted.
pub fn decode_secret_from_storage(stored: &[u8]) -> Result<Vec<u8>, SecretStoreError> {
    #[cfg(windows)]
    {
        if let Some(encrypted) = stored.strip_prefix(DPAPI_MAGIC) {
            return windows_dpapi::unprotect(encrypted);
        }
        Ok(stored.to_vec())
    }

    #[cfg(not(windows))]
    {
        Ok(stored.to_vec())
    }
}

/// Whether a loaded payload should be rewritten through
/// [`encode_secret_for_storage`] after successful parsing.
pub fn secret_payload_needs_migration(stored: &[u8]) -> bool {
    #[cfg(windows)]
    {
        !stored.starts_with(DPAPI_MAGIC)
    }

    #[cfg(not(windows))]
    {
        let _ = stored;
        false
    }
}

#[cfg(windows)]
mod windows_dpapi {
    use super::SecretStoreError;
    use std::ffi::c_void;
    use std::ptr;
    use std::slice;

    #[repr(C)]
    struct DataBlob {
        cb_data: u32,
        pb_data: *mut u8,
    }

    const CRYPTPROTECT_UI_FORBIDDEN: u32 = 0x1;

    #[link(name = "Crypt32")]
    extern "system" {
        fn CryptProtectData(
            p_data_in: *mut DataBlob,
            sz_data_descr: *const u16,
            p_optional_entropy: *mut DataBlob,
            pv_reserved: *mut c_void,
            p_prompt_struct: *mut c_void,
            dw_flags: u32,
            p_data_out: *mut DataBlob,
        ) -> i32;

        fn CryptUnprotectData(
            p_data_in: *mut DataBlob,
            ppsz_data_descr: *mut *mut u16,
            p_optional_entropy: *mut DataBlob,
            pv_reserved: *mut c_void,
            p_prompt_struct: *mut c_void,
            dw_flags: u32,
            p_data_out: *mut DataBlob,
        ) -> i32;
    }

    #[link(name = "Kernel32")]
    extern "system" {
        fn LocalFree(h_mem: *mut c_void) -> *mut c_void;
    }

    pub fn protect(plaintext: &[u8]) -> Result<Vec<u8>, SecretStoreError> {
        let mut input = blob_from_slice(plaintext);
        let mut output = empty_blob();
        let ok = unsafe {
            CryptProtectData(
                &mut input,
                ptr::null(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        blob_result(ok, output)
    }

    pub fn unprotect(encrypted: &[u8]) -> Result<Vec<u8>, SecretStoreError> {
        let mut input = blob_from_slice(encrypted);
        let mut output = empty_blob();
        let ok = unsafe {
            CryptUnprotectData(
                &mut input,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        blob_result(ok, output)
    }

    fn blob_from_slice(bytes: &[u8]) -> DataBlob {
        DataBlob {
            cb_data: bytes.len().try_into().unwrap_or(u32::MAX),
            pb_data: if bytes.is_empty() {
                ptr::null_mut()
            } else {
                bytes.as_ptr() as *mut u8
            },
        }
    }

    fn empty_blob() -> DataBlob {
        DataBlob {
            cb_data: 0,
            pb_data: ptr::null_mut(),
        }
    }

    fn blob_result(ok: i32, output: DataBlob) -> Result<Vec<u8>, SecretStoreError> {
        if ok == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let bytes = if output.pb_data.is_null() || output.cb_data == 0 {
            Vec::new()
        } else {
            unsafe { slice::from_raw_parts(output.pb_data, output.cb_data as usize).to_vec() }
        };
        if !output.pb_data.is_null() {
            unsafe {
                LocalFree(output.pb_data as *mut c_void);
            }
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_roundtrip_preserves_payload() {
        let payload = br#"{"access_token":"secret-token"}"#;
        let stored = encode_secret_for_storage(payload).unwrap();
        let decoded = decode_secret_from_storage(&stored).unwrap();
        assert_eq!(decoded, payload);
    }

    #[test]
    fn plaintext_payloads_are_read_for_migration() {
        let payload = br#"{"legacy":"plaintext"}"#;
        let decoded = decode_secret_from_storage(payload).unwrap();
        assert_eq!(decoded, payload);
    }

    #[test]
    fn only_windows_plaintext_payloads_need_migration() {
        let payload = br#"{"legacy":"plaintext"}"#;
        assert_eq!(secret_payload_needs_migration(payload), cfg!(windows));
    }

    #[cfg(windows)]
    #[test]
    fn windows_storage_is_not_plaintext() {
        let payload = br#"{"access_token":"secret-token"}"#;
        let stored = encode_secret_for_storage(payload).unwrap();
        assert_ne!(stored, payload);
        assert!(stored.starts_with(DPAPI_MAGIC));
    }
}
