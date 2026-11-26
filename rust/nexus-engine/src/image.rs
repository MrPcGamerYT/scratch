//! ZLAG v1: 64-byte little-endian header followed by raw, sector-aligned disk.
//! CRC32 detects accidental corruption; it is NOT a signature or security hash.
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

const MAGIC: &[u8; 8] = b"ZLAGIMG\0";
const HEADER: u64 = 64;
const MAX_DISK: u64 = 2 * 1024 * 1024 * 1024 * 1024;
fn invalid(text: &str) -> io::Error { io::Error::new(io::ErrorKind::InvalidData, text) }

const fn crc_table() -> [u32;256] {
    let mut table=[0;256]; let mut index=0;
    while index<256 {
        let mut value=index as u32; let mut bit=0;
        while bit<8 { value=(value>>1) ^ (0xedb88320 & (0u32.wrapping_sub(value&1))); bit+=1; }
        table[index]=value; index+=1;
    }
    table
}
const CRC_TABLE:[u32;256]=crc_table();
pub fn crc32(bytes: &[u8], mut state: u32) -> u32 {
    for &byte in bytes { state=(state>>8)^CRC_TABLE[((state ^ u32::from(byte))&255) as usize]; }
    state
}
fn checksum(bytes: &[u8]) -> u32 { !crc32(bytes, !0) }
fn header(length: u64, crc: u32) -> [u8; 64] {
    let mut h = [0u8; 64];
    h[..8].copy_from_slice(MAGIC);
    h[8..12].copy_from_slice(&1u32.to_le_bytes());
    h[12..16].copy_from_slice(&64u32.to_le_bytes());
    h[16..20].copy_from_slice(&512u32.to_le_bytes());
    h[24..32].copy_from_slice(&length.to_le_bytes());
    h[32..36].copy_from_slice(&crc.to_le_bytes());
    let crc = checksum(&h[..60]);
    h[60..64].copy_from_slice(&crc.to_le_bytes());
    h
}

/// Import an OFFLINE raw disk, never a renamed QCOW2. Destination is exclusive.
/// Failed imports retain an invalid-header partial output for diagnosis.
pub fn import_raw(source: &Path, target: &Path) -> io::Result<()> {
    let mut input = File::open(source)?;
    if !input.metadata()?.is_file() { return Err(invalid("Source must be a regular raw disk file")); }
    let length = input.metadata()?.len();
    if length == 0 || length % 512 != 0 || length > MAX_DISK { return Err(invalid("Raw length must be 512-byte aligned, nonzero and at most 2 TiB")); }
    let mut prefix = [0; 8]; input.read_exact(&mut prefix)?; input.seek(SeekFrom::Start(0))?;
    if prefix.starts_with(b"QFI\xfb") || &prefix == MAGIC || &prefix == b"vhdxfile" {
        return Err(invalid("Container image detected. Export a standalone raw disk first; renaming is not conversion"));
    }
    let mut output = OpenOptions::new().write(true).create_new(true).open(target)?;
    output.write_all(&[0u8; 64])?; // No valid magic until the entire import succeeds.
    let mut buffer = vec![0; 1024 * 1024];
    let mut remaining = length; let mut crc = !0;
    while remaining > 0 {
        let count = remaining.min(buffer.len() as u64) as usize;
        input.read_exact(&mut buffer[..count])?;
        crc = crc32(&buffer[..count], crc);
        output.write_all(&buffer[..count])?;
        remaining -= count as u64;
    }
    let mut extra = [0];
    if input.read(&mut extra)? != 0 || input.metadata()?.len() != length { return Err(invalid("Source changed during import")); }
    output.sync_all()?;
    output.seek(SeekFrom::Start(0))?;
    output.write_all(&header(length, !crc))?;
    output.sync_all()
}

pub struct Image { file: File, pub disk_bytes: u64, pub payload_crc32: u32 }
impl Image {
    /// Validates header and exact length. Call verify() before trusting payload.
    pub fn open(path: &Path) -> io::Result<Self> {
        let mut file = File::open(path)?;
        if !file.metadata()?.is_file() { return Err(invalid("Image must be a regular file")); }
        let mut h = [0; 64]; file.read_exact(&mut h)?;
        let u32_at = |i| u32::from_le_bytes(h[i..i+4].try_into().unwrap());
        if &h[..8] != MAGIC || u32_at(8) != 1 || u32_at(12) != 64 || u32_at(16) != 512 {
            return Err(invalid("Unsupported ZLAG magic/version/header/sector size"));
        }
        if h[20..24].iter().chain(h[36..60].iter()).any(|&b| b != 0) || checksum(&h[..60]) != u32_at(60) {
            return Err(invalid("Unknown flags/reserved fields or corrupt header"));
        }
        let disk_bytes = u64::from_le_bytes(h[24..32].try_into().unwrap());
        if disk_bytes == 0 || disk_bytes % 512 != 0 || disk_bytes > MAX_DISK || file.metadata()?.len() != HEADER + disk_bytes {
            return Err(invalid("Invalid/truncated/trailing image payload"));
        }
        Ok(Self { file, disk_bytes, payload_crc32: u32_at(32) })
    }
    pub fn verify(&mut self) -> io::Result<()> {
        self.file.seek(SeekFrom::Start(HEADER))?;
        let mut crc = !0; let mut remaining = self.disk_bytes;
        let mut buffer = vec![0; 1024 * 1024];
        while remaining > 0 {
            let count = remaining.min(buffer.len() as u64) as usize;
            self.file.read_exact(&mut buffer[..count])?;
            crc = crc32(&buffer[..count], crc); remaining -= count as u64;
        }
        if !crc != self.payload_crc32 { return Err(invalid("Disk payload checksum mismatch")); }
        Ok(())
    }
    /// Read-only sector transport for a future block device. Header never leaks.
    pub fn read_sectors(&mut self, lba: u64, data: &mut [u8]) -> io::Result<()> {
        if data.len() % 512 != 0 { return Err(invalid("Sector buffer must be aligned")); }
        let offset = lba.checked_mul(512).ok_or_else(|| invalid("LBA overflow"))?;
        let end = offset.checked_add(data.len() as u64).ok_or_else(|| invalid("Read overflow"))?;
        if end > self.disk_bytes { return Err(invalid("Read beyond virtual disk")); }
        self.file.seek(SeekFrom::Start(HEADER + offset))?;
        self.file.read_exact(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static ID: AtomicU64 = AtomicU64::new(0);
    struct Temp(std::path::PathBuf);
    impl Temp { fn new() -> Self { let p = std::env::temp_dir().join(format!("nexus-native-{}-{}", std::process::id(), ID.fetch_add(1, Ordering::Relaxed))); std::fs::create_dir(&p).unwrap(); Self(p) } }
    impl Drop for Temp { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }
    #[test] fn crc_vector() { assert_eq!(checksum(b"123456789"), 0xcbf43926); }
    #[test] fn round_trip_bounds_and_no_overwrite() {
        let t = Temp::new(); let raw = t.0.join("raw.img"); let zlag = t.0.join("gustimg.zlag");
        let data: Vec<u8> = (0..2048).map(|n| (n % 251) as u8).collect(); std::fs::write(&raw, &data).unwrap();
        import_raw(&raw, &zlag).unwrap(); let mut image = Image::open(&zlag).unwrap(); image.verify().unwrap();
        let mut sector = [0;512]; image.read_sectors(2,&mut sector).unwrap(); assert_eq!(sector, data[1024..1536]);
        assert!(image.read_sectors(u64::MAX,&mut sector).is_err()); assert!(image.read_sectors(4,&mut sector).is_err());
        assert!(image.read_sectors(0,&mut [0;511]).is_err());
        assert!(import_raw(&raw,&zlag).is_err()); assert_eq!(std::fs::read(&raw).unwrap(), data);
    }
    #[test] fn corruption_rejected() {
        let t = Temp::new(); let raw=t.0.join("raw"); let z=t.0.join("z"); std::fs::write(&raw,[7;512]).unwrap(); import_raw(&raw,&z).unwrap();
        let original=std::fs::read(&z).unwrap();
        for index in [0,8,12,16,20,24,36,60] {
            let mut bytes=original.clone(); bytes[index]^=1; std::fs::write(&z,&bytes).unwrap(); assert!(Image::open(&z).is_err());
        }
        let mut bytes=original.clone(); bytes[100]^=1; std::fs::write(&z,bytes).unwrap(); assert!(Image::open(&z).unwrap().verify().is_err());
        std::fs::write(&z,&original[..original.len()-1]).unwrap(); assert!(Image::open(&z).is_err());
        let mut bytes=original; bytes.push(0); std::fs::write(&z,bytes).unwrap(); assert!(Image::open(&z).is_err());
    }
    #[test] fn renamed_containers_refused() {
        let t=Temp::new(); let raw=t.0.join("raw");
        for magic in [b"QFI\xfb\0\0\0\0",b"ZLAGIMG\0",b"vhdxfile"] {
            let mut data=[0;512]; data[..8].copy_from_slice(magic); std::fs::write(&raw,data).unwrap();
            assert!(import_raw(&raw,&t.0.join("dest")).is_err()); assert!(!t.0.join("dest").exists());
        }
    }
}
