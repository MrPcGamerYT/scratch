//! Append-only sector overlay v1. Base stays read-only. CRC detects corruption, not tampering.
//! Strict reopen rejects incomplete tails; explicit recover_incomplete_tail truncates only
//! a short last record after validating every complete record. No automatic repair of CRC errors.
use crate::image::{self,Image};
use std::{collections::BTreeMap,fs::{File,OpenOptions},io::{Read,Write,Seek,SeekFrom},path::Path};
const HEADER:usize=64;
const RECORD:usize=536;
fn crc(bytes:&[u8])->u32 {!image::crc32(bytes,!0)}
fn error(text:&str)->String {format!("Overlay: {text}")}
fn open_exclusive(path:&Path,create:bool)->Result<File,String> {
    let mut options=OpenOptions::new();options.read(true).write(true).create_new(create);
    #[cfg(windows)] {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1); // FILE_SHARE_READ only; no second writer or delete.
    }
    let file=options.open(path).map_err(|e|e.to_string())?;
    #[cfg(unix)] {
        use std::os::fd::AsRawFd;
        extern "C" {fn flock(fd:i32,operation:i32)->i32;}
        if unsafe {flock(file.as_raw_fd(),2|4)}!=0 {return Err(error("already locked by another writer"));}
    }
    Ok(file)
}
pub struct Overlay {
    base:Image,file:File,index:BTreeMap<u64,u64>,sequence:u64,poisoned:bool,
}
impl Overlay {
    fn header(base:&Image)->[u8;HEADER] {
        let mut h=[0;HEADER];h[..8].copy_from_slice(b"NXCOW001");h[8..12].copy_from_slice(&1u32.to_le_bytes());
        h[12..16].copy_from_slice(&512u32.to_le_bytes());h[16..24].copy_from_slice(&base.disk_bytes.to_le_bytes());
        h[24..28].copy_from_slice(&base.payload_crc32.to_le_bytes());let sum=crc(&h[..60]);h[60..].copy_from_slice(&sum.to_le_bytes());h
    }
    pub fn create(base_path:&Path,path:&Path)->Result<Self,String> {
        let mut base=Image::open(base_path).map_err(|e|e.to_string())?;base.verify().map_err(|e|e.to_string())?;
        let mut file=open_exclusive(path,true)?;file.write_all(&Self::header(&base)).map_err(|e|e.to_string())?;
        file.sync_all().map_err(|e|e.to_string())?;
        Ok(Self{base,file,index:BTreeMap::new(),sequence:0,poisoned:false})
    }
    pub fn open(base_path:&Path,path:&Path)->Result<Self,String> {Self::load(base_path,path,false)}
    pub fn recover_incomplete_tail(base_path:&Path,path:&Path)->Result<Self,String> {Self::load(base_path,path,true)}
    fn load(base_path:&Path,path:&Path,recover:bool)->Result<Self,String> {
        let mut base=Image::open(base_path).map_err(|e|e.to_string())?;base.verify().map_err(|e|e.to_string())?;
        let mut file=open_exclusive(path,false)?;let mut header=[0;HEADER];file.read_exact(&mut header).map_err(|e|e.to_string())?;
        if header!=Self::header(&base) {return Err(error("header corrupt, unsupported format or wrong base image"));}
        let length=file.metadata().map_err(|e|e.to_string())?.len();let payload=length.checked_sub(HEADER as u64).ok_or_else(||error("header truncated during open"))?;
        let complete=payload/RECORD as u64;let tail=payload%RECORD as u64;
        let mut index=BTreeMap::new();let mut record=[0;RECORD];
        for n in 0..complete {
            file.read_exact(&mut record).map_err(|e|e.to_string())?;
            let sequence=u64::from_le_bytes(record[..8].try_into().unwrap());let lba=u64::from_le_bytes(record[8..16].try_into().unwrap());
            let expected=u32::from_le_bytes(record[528..532].try_into().unwrap());
            if sequence!=n+1||lba>=base.disk_bytes/512||&record[532..]!=b"CMIT"||crc(&record[..528])!=expected {
                return Err(error("corrupt record, sequence or LBA; refusing repair"));
            }
            index.insert(lba,HEADER as u64+n*RECORD as u64);
        }
        if tail!=0 {
            if !recover {return Err(error("incomplete last record; explicit recovery required"));}
            file.set_len(HEADER as u64+complete*RECORD as u64).map_err(|e|e.to_string())?;
            file.sync_all().map_err(|e|e.to_string())?;
        }
        Ok(Self{base,file,index,sequence:complete,poisoned:false})
    }
    pub fn disk_bytes(&self)->u64 {self.base.disk_bytes}
    fn bounds(&self,lba:u64,length:usize)->Result<(),String> {
        if length==0||length%512!=0||lba.checked_add((length/512) as u64).is_none_or(|end|end>self.disk_bytes()/512) {
            return Err(error("unaligned, empty or out-of-range request"));
        }
        if self.poisoned {return Err(error("previous host write failed; close and inspect overlay"));}Ok(())
    }
    pub fn read_sectors(&mut self,lba:u64,data:&mut [u8])->Result<(),String> {
        self.bounds(lba,data.len())?;
        for (n,sector) in data.chunks_exact_mut(512).enumerate() {
            let sector_lba=lba+n as u64;
            if let Some(&offset)=self.index.get(&sector_lba) {
                // Revalidate on every read; don't silently serve subsequently damaged records.
                self.file.seek(SeekFrom::Start(offset)).map_err(|e|e.to_string())?;let mut record=[0;RECORD];
                self.file.read_exact(&mut record).map_err(|e|e.to_string())?;
                if crc(&record[..528])!=u32::from_le_bytes(record[528..532].try_into().unwrap())||&record[532..]!=b"CMIT"||u64::from_le_bytes(record[8..16].try_into().unwrap())!=sector_lba {
                    return Err(error("record changed or corrupt during read"));
                }
                sector.copy_from_slice(&record[16..528]);
            } else {self.base.read_sectors(sector_lba,sector).map_err(|e|e.to_string())?;}
        }Ok(())
    }
    pub fn write_sectors(&mut self,lba:u64,data:&[u8])->Result<(),String> {
        self.bounds(lba,data.len())?;
        // Per-sector commit, not a multi-sector transaction. Caller must use filesystem journaling.
        for (n,sector) in data.chunks_exact(512).enumerate() {
            let sector_lba=lba+n as u64;let next=self.sequence.checked_add(1).ok_or_else(||error("sequence exhausted"))?;
            let mut record=[0;RECORD];record[..8].copy_from_slice(&next.to_le_bytes());record[8..16].copy_from_slice(&sector_lba.to_le_bytes());
            record[16..528].copy_from_slice(sector);let sum=crc(&record[..528]);record[528..532].copy_from_slice(&sum.to_le_bytes());record[532..].copy_from_slice(b"CMIT");
            let result=(|| {
                let offset=self.file.seek(SeekFrom::End(0)).map_err(|e|e.to_string())?;
                self.file.write_all(&record).map_err(|e|e.to_string())?;self.file.sync_data().map_err(|e|e.to_string())?;Ok::<u64,String>(offset)
            })();
            match result {Ok(offset)=>{self.index.insert(sector_lba,offset);self.sequence=next;},Err(e)=>{self.poisoned=true;return Err(e);}}
        }Ok(())
    }
    pub fn flush(&mut self)->Result<(),String> {
        if self.poisoned {return Err(error("cannot flush poisoned overlay"));}
        if let Err(e)=self.file.sync_all() {self.poisoned=true;return Err(e.to_string());}Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64,Ordering};
    static NEXT:AtomicU64=AtomicU64::new(0);
    struct Temp {root:std::path::PathBuf,base:std::path::PathBuf,cow:std::path::PathBuf}
    impl Temp {
        fn new()->Self {
            let root=std::env::temp_dir().join(format!("nexus-cow-{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));
            std::fs::create_dir(&root).unwrap();let raw=root.join("raw");let base=root.join("base.zlag");let cow=root.join("data.cow");
            std::fs::write(&raw,[7;2048]).unwrap();image::import_raw(&raw,&base).unwrap();Self{root,base,cow}
        }
    }
    impl Drop for Temp {fn drop(&mut self) {let _=std::fs::remove_dir_all(&self.root);}}
    #[test] fn persistence_latest_write_fallback_and_source_unchanged() {
        let t=Temp::new();let before=std::fs::read(&t.base).unwrap();
        {let mut o=Overlay::create(&t.base,&t.cow).unwrap();o.write_sectors(1,&[9;1024]).unwrap();o.write_sectors(1,&[11;512]).unwrap();o.flush().unwrap();}
        let mut o=Overlay::open(&t.base,&t.cow).unwrap();let mut data=[0;2048];o.read_sectors(0,&mut data).unwrap();
        assert_eq!(&data[..512],&[7;512]);assert_eq!(&data[512..1024],&[11;512]);assert_eq!(&data[1024..1536],&[9;512]);assert_eq!(&data[1536..],&[7;512]);
        assert_eq!(std::fs::read(&t.base).unwrap(),before);
    }
    #[test] fn single_writer_no_overwrite_and_bounds() {
        let t=Temp::new();let mut o=Overlay::create(&t.base,&t.cow).unwrap();
        assert!(Overlay::create(&t.base,&t.cow).is_err());assert!(Overlay::open(&t.base,&t.cow).is_err());
        for (lba,data) in [(4,&[0;512][..]),(u64::MAX,&[0;512][..]),(0,&[0;511][..]),(0,&[][..])] {assert!(o.write_sectors(lba,data).is_err());}
        assert_eq!(std::fs::metadata(&t.cow).unwrap().len(),HEADER as u64);
    }
    #[test] fn incomplete_tail_requires_explicit_recovery_and_crc_damage_never_repaired() {
        let t=Temp::new();{let mut o=Overlay::create(&t.base,&t.cow).unwrap();o.write_sectors(0,&[3;512]).unwrap();}
        let good=std::fs::read(&t.cow).unwrap();let mut torn=good.clone();torn.extend_from_slice(&[8;73]);std::fs::write(&t.cow,&torn).unwrap();
        assert!(Overlay::open(&t.base,&t.cow).is_err());assert_eq!(std::fs::read(&t.cow).unwrap(),torn);
        drop(Overlay::recover_incomplete_tail(&t.base,&t.cow).unwrap());assert_eq!(std::fs::read(&t.cow).unwrap(),good);
        let mut bad=good.clone();bad[HEADER+20]^=1;std::fs::write(&t.cow,&bad).unwrap();
        assert!(Overlay::recover_incomplete_tail(&t.base,&t.cow).is_err());assert_eq!(std::fs::read(&t.cow).unwrap(),bad);
    }
    #[test] fn wrong_base_and_header_refused_without_mutating_overlay() {
        let t=Temp::new();drop(Overlay::create(&t.base,&t.cow).unwrap());let original=std::fs::read(&t.cow).unwrap();
        let other=t.root.join("other.raw");let other_z=t.root.join("other.zlag");std::fs::write(&other,[8;2048]).unwrap();image::import_raw(&other,&other_z).unwrap();
        assert!(Overlay::open(&other_z,&t.cow).is_err());assert_eq!(std::fs::read(&t.cow).unwrap(),original);
        for index in [0,8,16,24,32,60] {let mut bad=original.clone();bad[index]^=1;std::fs::write(&t.cow,&bad).unwrap();assert!(Overlay::open(&t.base,&t.cow).is_err());}
    }
}
