//! Primary-master ATA PIO LBA28 read-only subset backed by a verified ZLAG file.
//! Polled, nIEN=1 only. No IRQ14, DMA, ATAPI, LBA48, writes or persistent overlay.
//! Unsupported ATA commands return ABRT; unsupported host I/O shapes fail closed.
use crate::{image::Image,io::IoExit};
use std::path::Path;
const READY:u8=0x50;
const DRQ:u8=8;
const ERR:u8=1;
pub struct AtaPio {
    image:Image,regs:[u8;8],control:u8,buffer:Vec<u8>,offset:usize,
    read_lba:Option<u32>,pub commands:u64,pub sectors_read:u64,
}
impl AtaPio {
    pub fn open(path:&Path)->Result<Self,String> {
        let mut image=Image::open(path).map_err(|e|e.to_string())?;
        image.verify().map_err(|e|e.to_string())?;
        if image.disk_bytes/512>1<<28 {return Err("ATA LBA28 subset supports disks up to 128 GiB".into());}
        let mut result=Self{image,regs:[0;8],control:2,buffer:Vec::new(),offset:0,read_lba:None,commands:0,sectors_read:0};
        result.reset();Ok(result)
    }
    fn reset(&mut self) {
        self.regs=[0,1,1,1,0,0,0xa0,READY];self.clear_transfer();
    }
    fn clear_transfer(&mut self) {self.buffer.clear();self.offset=0;self.read_lba=None;}
    fn abort(&mut self,error:u8) {self.clear_transfer();self.regs[1]=error;self.regs[7]=READY|ERR;}
    fn selected(&self)->bool {self.regs[6]&0x10==0}
    fn lba(&self)->u32 {u32::from(self.regs[3])|(u32::from(self.regs[4])<<8)|(u32::from(self.regs[5])<<16)|(u32::from(self.regs[6]&15)<<24)}
    fn set_lba(&mut self,lba:u32) {
        self.regs[3]=lba as u8;self.regs[4]=(lba>>8) as u8;self.regs[5]=(lba>>16) as u8;
        self.regs[6]=(self.regs[6]&0xf0)|((lba>>24) as u8&15);
    }
    fn identify(&self)->Vec<u8> {
        let mut data=vec![0;512];
        let sectors=(self.image.disk_bytes/512) as u32;
        // Fixed ATA disk, LBA28 only. No DMA or LBA48 feature bits advertised.
        for (word,value) in [(0,0x0040),(47,0),(49,1<<9),(60,sectors as u16),(61,(sectors>>16) as u16)] {
            data[word*2..word*2+2].copy_from_slice(&value.to_le_bytes());
        }
        let model=b"NEXUS READ ONLY ZLAG PIO DISK             ";
        for (index,pair) in model.chunks_exact(2).enumerate() {data[54+index*2]=pair[1];data[55+index*2]=pair[0];}
        data
    }
    fn command(&mut self,value:u8)->Result<(),String> {
        self.commands+=1;self.clear_transfer();self.regs[1]=0;self.regs[7]=READY;
        if !self.selected() {return Ok(());}
        match value {
            0xec=>self.buffer=self.identify(),
            0x20=>{
                if self.regs[6]&0x40==0 {self.abort(4);return Ok(());}
                let count=if self.regs[2]==0 {256} else {u32::from(self.regs[2])};let lba=self.lba();
                if u64::from(lba)+u64::from(count)>self.image.disk_bytes/512 {self.abort(0x10);return Ok(());}
                self.buffer.resize(count as usize*512,0);
                if let Err(error)=self.image.read_sectors(u64::from(lba),&mut self.buffer) {
                    self.abort(0x40);return Err(format!("ATA backing read failed: {error}"));
                }
                self.read_lba=Some(lba);
            }
            _=>{self.abort(4);return Ok(());}
        }
        self.regs[7]=READY|DRQ;Ok(())
    }
    pub fn access(&mut self,io:IoExit)->Result<Option<u64>,String> {
        if io.port==0x3f6&&io.size==1 {
            if !io.write {return Ok(Some(io.input_register(u32::from(if self.selected() {self.regs[7]} else {0}))));}
            let value=io.rax as u8;
            if value&!6!=0||value&2==0 {return Err("ATA PIO subset requires nIEN=1; IRQ14 is not implemented".into());}
            let was_reset=self.control&4!=0;self.control=value;
            if value&4!=0 {self.clear_transfer();self.regs[7]=0x80;}
            else if was_reset {self.reset();}
            return Ok(None);
        }
        if !(0x1f0..=0x1f7).contains(&io.port) {return Err("Unsupported ATA port".into());}
        if self.control&4!=0 {return Err("ATA task-file access while software reset is asserted".into());}
        let index=(io.port-0x1f0) as usize;
        if index==0 {
            if io.size!=2 {return Err("ATA data port requires scalar 16-bit I/O".into());}
            if io.write {self.abort(4);return Ok(None);}
            if !self.selected()||self.regs[7]&DRQ==0||self.offset+2>self.buffer.len() {
                self.abort(4);return Ok(Some(io.input_register(0xffff)));
            }
            let word=u16::from_le_bytes([self.buffer[self.offset],self.buffer[self.offset+1]]);self.offset+=2;
            if self.offset%512==0 {
                if let Some(lba)=self.read_lba {self.sectors_read+=1;self.regs[2]=self.regs[2].wrapping_sub(1);self.set_lba(lba+1);self.read_lba=Some(lba+1);}
            }
            if self.offset==self.buffer.len() {self.clear_transfer();self.regs[7]=READY;}
            return Ok(Some(io.input_register(u32::from(word))));
        }
        if io.size!=1 {return Err("ATA task-file registers require byte I/O".into());}
        if io.write {
            let value=io.rax as u8;
            if index==7 {self.command(value)?;}
            else {
                if self.regs[7]&DRQ!=0 {self.abort(4);}
                self.regs[index]=value;
            }
            Ok(None)
        } else {
            Ok(Some(io.input_register(u32::from(if self.selected() {self.regs[index]} else {0}))))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64,Ordering};
    static NEXT:AtomicU64=AtomicU64::new(0);
    struct Fixture {dir:std::path::PathBuf,path:std::path::PathBuf,payload:Vec<u8>}
    impl Fixture {
        fn new(sectors:usize)->Self {
            let dir=std::env::temp_dir().join(format!("nexus-ata-{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));
            std::fs::create_dir(&dir).unwrap();let raw=dir.join("raw");let path=dir.join("disk.zlag");
            let payload=(0..sectors*512).map(|n|((n*17+n/512)%251) as u8).collect::<Vec<_>>();
            std::fs::write(&raw,&payload).unwrap();crate::image::import_raw(&raw,&path).unwrap();Self{dir,path,payload}
        }
    }
    impl Drop for Fixture {fn drop(&mut self) {let _=std::fs::remove_dir_all(&self.dir);}}
    fn io(port:u16,size:u8,write:bool,rax:u64)->IoExit {IoExit{port,size,write,rax,rip:0,instruction_len:1}}
    fn out(a:&mut AtaPio,port:u16,value:u8) {a.access(io(port,1,true,u64::from(value))).unwrap();}
    fn input(a:&mut AtaPio,port:u16)->u8 {a.access(io(port,1,false,0)).unwrap().unwrap() as u8}
    fn data(a:&mut AtaPio,bytes:usize)->Vec<u8> {
        (0..bytes/2).flat_map(|_|(a.access(io(0x1f0,2,false,0xabcd0000)).unwrap().unwrap() as u16).to_le_bytes()).collect()
    }
    #[test] fn identify_capacity_and_pio_sector_data_not_container_header() {
        let f=Fixture::new(4);let mut a=AtaPio::open(&f.path).unwrap();
        out(&mut a,0x1f7,0xec);assert_eq!(input(&mut a,0x1f7),READY|DRQ);
        let id=data(&mut a,512);assert_eq!(u32::from_le_bytes(id[120..124].try_into().unwrap()),4);
        assert_eq!(id[98],0);assert_eq!(id[99],2);assert_eq!(input(&mut a,0x1f7),READY);
        for (p,v) in [(0x1f6,0xe0),(0x1f2,2),(0x1f3,1),(0x1f4,0),(0x1f5,0),(0x1f7,0x20)] {out(&mut a,p,v);}
        assert_eq!(data(&mut a,1024),f.payload[512..1536]);assert_eq!(a.sectors_read,2);
        assert_eq!(input(&mut a,0x1f2),0);assert_eq!(input(&mut a,0x1f3),3);assert_eq!(input(&mut a,0x1f7),READY);
    }
    #[test] fn zero_count_means_256_sectors_and_upper_rax_is_preserved() {
        let f=Fixture::new(256);let mut a=AtaPio::open(&f.path).unwrap();
        for (p,v) in [(0x1f6,0xe0),(0x1f2,0),(0x1f3,0),(0x1f7,0x20)] {out(&mut a,p,v);}
        let value=a.access(io(0x1f0,2,false,0x123456780000)).unwrap().unwrap();assert_eq!(value&!0xffff,0x123456780000);
        let mut result=(value as u16).to_le_bytes().to_vec();result.extend(data(&mut a,256*512-2));
        assert_eq!(result,f.payload);assert_eq!(a.sectors_read,256);assert_eq!(input(&mut a,0x1f2),0);
    }
    #[test] fn write_dma_and_bounds_refused_and_source_unchanged() {
        let f=Fixture::new(4);let before=std::fs::read(&f.path).unwrap();let mut a=AtaPio::open(&f.path).unwrap();
        for cmd in [0x30,0xca,0xc8,0x24,0xe7,0xa1] {out(&mut a,0x1f7,cmd);assert_eq!(input(&mut a,0x1f7)&ERR,ERR);assert_eq!(input(&mut a,0x1f1),4);}
        for (p,v) in [(0x1f6,0xe0),(0x1f2,2),(0x1f3,3),(0x1f7,0x20)] {out(&mut a,p,v);}
        assert_eq!(input(&mut a,0x1f1),0x10);assert_eq!(input(&mut a,0x1f7)&DRQ,0);
        assert!(a.access(io(0x1f0,1,false,0)).is_err());assert!(a.access(io(0x1f7,2,true,0)).is_err());
        assert!(a.access(io(0x3f6,1,true,0)).is_err());assert_eq!(std::fs::read(&f.path).unwrap(),before);
    }
    #[test] fn reset_slave_and_corrupt_images() {
        let f=Fixture::new(4);let mut a=AtaPio::open(&f.path).unwrap();out(&mut a,0x1f7,0xec);
        out(&mut a,0x3f6,6);assert_eq!(input(&mut a,0x3f6),0x80);out(&mut a,0x3f6,2);
        assert_eq!(input(&mut a,0x1f7),READY);assert_eq!(input(&mut a,0x1f2),1);
        out(&mut a,0x1f6,0xb0);assert_eq!(input(&mut a,0x1f7),0);out(&mut a,0x1f6,0xe0);
        out(&mut a,0x1f7,0xec);assert_eq!(data(&mut a,512).len(),512);
        let mut bytes=std::fs::read(&f.path).unwrap();bytes[100]^=1;std::fs::write(&f.path,bytes).unwrap();
        assert!(AtaPio::open(&f.path).is_err());
    }
}
