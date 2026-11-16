//! 当前 Windows 用户的 DPAPI 加密；不使用自制密钥。
use std::{io, path::Path};
#[cfg(windows)]
fn protect(data: &[u8], encrypt: bool) -> io::Result<Vec<u8>> {
    use windows::Win32::{
        Foundation::{LocalFree, HLOCAL},
        Security::Cryptography::*,
    };
    if data.len() > 64 * 1024 * 1024 {
        return Err(io::Error::other("记录超过 64 MiB 上限"));
    }
    let input = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        let result = if encrypt {
            CryptProtectData(
                &input,
                None,
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                None,
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        result.map_err(|e| io::Error::other(e.to_string()))?;
        if output.pbData.is_null() {
            return Err(io::Error::other("加密存储失败"));
        }
        let bytes = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        let _ = LocalFree(HLOCAL(output.pbData.cast()));
        Ok(bytes)
    }
}
#[cfg(not(windows))]
fn protect(data: &[u8], _: bool) -> io::Result<Vec<u8>> {
    Ok(data.to_vec())
}
pub fn write(path: &Path, data: &[u8]) -> io::Result<()> {
    let mut bytes = b"ISLAND-DPAPI-1\0".to_vec();
    bytes.extend(protect(data, true)?);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    crate::model::atomic_write(path, &bytes)
}
pub fn read(path: &Path) -> io::Result<Vec<u8>> {
    let bytes = std::fs::read(path)?;
    let payload = bytes
        .strip_prefix(b"ISLAND-DPAPI-1\0")
        .ok_or_else(|| io::Error::other("无法识别加密记录"))?;
    protect(payload, false)
}
#[cfg(test)]
mod tests {
    #[test]
    fn encrypted_store_round_trip_does_not_expose_text() {
        let path =
            std::env::temp_dir().join(format!("island-encrypted-{}.dat", std::process::id()));
        let data = b"private-clipboard-example";
        super::write(&path, data).unwrap();
        assert_eq!(super::read(&path).unwrap(), data);
        #[cfg(windows)]
        assert!(!std::fs::read(&path)
            .unwrap()
            .windows(data.len())
            .any(|w| w == data));
        std::fs::remove_file(path).unwrap();
    }
}
