//! Windows DPAPI (Data Protection API) wrapper.
//!
//! `CryptProtectData`/`CryptUnprotectData` encrypt a blob using a key derived
//! from the current Windows user's login credentials. There is no key of our
//! own to manage or leak: only the same Windows account, on the same machine
//! (by default), can ever decrypt the result. This is the same mechanism
//! Chrome/Edge use to protect saved passwords and cookies, and it is what
//! `account.rs` uses to encrypt the stored Microsoft/Minecraft tokens.
//!
//! Implemented as raw FFI against crypt32.dll/kernel32.dll instead of a
//! wrapper crate: this C ABI has been stable since Windows 2000, which avoids
//! pulling in and pinning a large generated bindings crate for two function
//! calls.
//!
//! On non-Windows targets both functions always return `None`; callers must
//! treat that as "encryption unavailable" and fall back to a plain-text path
//! rather than treating it as fatal.

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;

    #[repr(C)]
    struct DataBlob {
        cb_data: u32,
        pb_data: *mut u8,
    }

    // wincrypt.h: CRYPTPROTECT_UI_FORBIDDEN. Never allow DPAPI to pop a
    // Windows UI prompt from inside a launcher call.
    const CRYPTPROTECT_UI_FORBIDDEN: u32 = 0x1;

    #[link(name = "crypt32")]
    extern "system" {
        fn CryptProtectData(
            p_data_in: *const DataBlob,
            sz_data_descr: *const u16,
            p_optional_entropy: *const DataBlob,
            pv_reserved: *const c_void,
            p_prompt_struct: *const c_void,
            dw_flags: u32,
            p_data_out: *mut DataBlob,
        ) -> i32;

        fn CryptUnprotectData(
            p_data_in: *const DataBlob,
            pp_data_descr: *mut *mut u16,
            p_optional_entropy: *const DataBlob,
            pv_reserved: *const c_void,
            p_prompt_struct: *const c_void,
            dw_flags: u32,
            p_data_out: *mut DataBlob,
        ) -> i32;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn LocalFree(h_mem: *mut c_void) -> *mut c_void;
    }

    /// Encrypts `plaintext` for the current Windows user profile. Returns
    /// `None` if DPAPI is unavailable for any reason -- callers must have a
    /// plain-text fallback and must never treat this as fatal.
    pub fn protect(plaintext: &[u8]) -> Option<Vec<u8>> {
        unsafe {
            let mut input = DataBlob {
                cb_data: plaintext.len() as u32,
                pb_data: plaintext.as_ptr() as *mut u8,
            };
            let mut output = DataBlob {
                cb_data: 0,
                pb_data: std::ptr::null_mut(),
            };

            let ok = CryptProtectData(
                &mut input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            );

            if ok == 0 || output.pb_data.is_null() {
                return None;
            }

            let bytes =
                std::slice::from_raw_parts(output.pb_data, output.cb_data as usize).to_vec();
            LocalFree(output.pb_data as *mut c_void);
            Some(bytes)
        }
    }

    /// Reverses [`protect`]. Returns `None` if the blob cannot be decrypted
    /// -- e.g. it was produced on a different machine or by a different
    /// Windows account. Callers should treat that the same as "nobody signed
    /// in", not as an error.
    pub fn unprotect(ciphertext: &[u8]) -> Option<Vec<u8>> {
        unsafe {
            let mut input = DataBlob {
                cb_data: ciphertext.len() as u32,
                pb_data: ciphertext.as_ptr() as *mut u8,
            };
            let mut output = DataBlob {
                cb_data: 0,
                pb_data: std::ptr::null_mut(),
            };

            let ok = CryptUnprotectData(
                &mut input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            );

            if ok == 0 || output.pb_data.is_null() {
                return None;
            }

            let bytes =
                std::slice::from_raw_parts(output.pb_data, output.cb_data as usize).to_vec();
            LocalFree(output.pb_data as *mut c_void);
            Some(bytes)
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn protect(_plaintext: &[u8]) -> Option<Vec<u8>> {
        None
    }

    pub fn unprotect(_ciphertext: &[u8]) -> Option<Vec<u8>> {
        None
    }
}

pub use imp::{protect, unprotect};

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn protect_and_unprotect_roundtrip() {
        let secret = b"nimbus-test-secret".to_vec();
        let ciphertext = protect(&secret).expect("DPAPI should be available in CI");
        assert_ne!(ciphertext, secret);
        let restored = unprotect(&ciphertext).expect("DPAPI should decrypt its own output");
        assert_eq!(restored, secret);
    }

    #[test]
    fn unprotect_rejects_garbage() {
        assert!(unprotect(b"not a real DPAPI blob").is_none());
    }
}
