//! Transport-independent VirtIO block request executor over bounded split queues.
//! No MMIO registers or IRQ delivery yet. All guest CPUs must be stopped during calls.
use crate::{guest_memory::GuestMemory,virtqueue::{SplitQueue,Chain,MAX_TRANSFER},overlay::Overlay,image::Image};
pub trait BlockBackend {
    fn capacity_sectors(&self)->u64;
    fn read(&mut self,sector:u64,data:&mut [u8])->Result<(),String>;
    fn write(&mut self,sector:u64,data:&[u8])->Result<(),String>;
    fn flush(&mut self)->Result<(),String>;
}
impl BlockBackend for Overlay {
    fn capacity_sectors(&self)->u64 {self.disk_bytes()/512}
    fn read(&mut self,s:u64,d:&mut [u8])->Result<(),String> {self.read_sectors(s,d)}
    fn write(&mut self,s:u64,d:&[u8])->Result<(),String> {self.write_sectors(s,d)}
    fn flush(&mut self)->Result<(),String> {Overlay::flush(self)}
}
pub struct ReadOnlyDisk {image:Image}
impl ReadOnlyDisk {
    pub fn open(path:&std::path::Path)->Result<Self,String> {
        let mut image=Image::open(path).map_err(|e|e.to_string())?;image.verify().map_err(|e|e.to_string())?;Ok(Self{image})
    }
}
impl BlockBackend for ReadOnlyDisk {
    fn capacity_sectors(&self)->u64 {self.image.disk_bytes/512}
    fn read(&mut self,s:u64,d:&mut [u8])->Result<(),String> {self.image.read_sectors(s,d).map_err(|e|e.to_string())}
    fn write(&mut self,_s:u64,_d:&[u8])->Result<(),String> {Err("Read-only base device".into())}
    fn flush(&mut self)->Result<(),String> {Ok(())}
}
#[derive(Debug)]
pub struct Completion {pub head:u16,pub status:u8,pub written:u32,pub notify:bool,pub error:Option<String>}
enum Failure {Unsupported,Io(String)}
impl From<String> for Failure {fn from(e:String)->Self {Self::Io(e)}}
impl From<&str> for Failure {fn from(e:&str)->Self {Self::Io(e.into())}}
fn execute(m:&mut GuestMemory<'_>,chain:&Chain,backend:&mut dyn BlockBackend)->Result<u32,Failure> {
    let descriptors=&chain.descriptors;
    if descriptors.len()<2 {return Err("Block request needs header and status".into());}
    let header=descriptors[0];
    if header.writable||header.length!=16 {return Err("Block header must be 16 device-readable bytes".into());}
    let mut raw=[0;16];m.read(header.address,&mut raw)?;
    let operation=u32::from_le_bytes(raw[..4].try_into().unwrap());let sector=u64::from_le_bytes(raw[8..].try_into().unwrap());
    let data=&descriptors[1..descriptors.len()-1];
    if operation==4 { // FLUSH: header + status only; sector is unused by the specification.
        if !data.is_empty() {return Err("FLUSH must not contain data descriptors".into());}
        backend.flush()?;return Ok(1);
    }
    if operation!=0&&operation!=1 {return Err(Failure::Unsupported);}
    let mut length=0usize;
    for d in data {
        if d.writable!=(operation==0) {return Err("Block data descriptor has wrong direction".into());}
        length=length.checked_add(d.length as usize).ok_or("Transfer length overflow")?;
        if length>MAX_TRANSFER {return Err("Block request exceeds transfer limit".into());}
    }
    if length==0||length%512!=0||sector.checked_add((length/512) as u64).is_none_or(|end|end>backend.capacity_sectors()) {
        return Err("Unaligned, empty or out-of-range block request".into());
    }
    let mut buffer=vec![0;length];
    if operation==0 {
        backend.read(sector,&mut buffer)?;let mut offset=0;
        for d in data {let n=d.length as usize;m.write(d.address,&buffer[offset..offset+n])?;offset+=n;}
        Ok(length as u32+1)
    } else {
        let mut offset=0;for d in data {let n=d.length as usize;m.read(d.address,&mut buffer[offset..offset+n])?;offset+=n;}
        backend.write(sector,&buffer)?;Ok(1)
    }
}
pub fn process_one(m:&mut GuestMemory<'_>,queue:&mut SplitQueue,backend:&mut dyn BlockBackend)->Result<Option<Completion>,String> {
    let Some(chain)=queue.peek(m)? else {return Ok(None);};
    // Never report IOERR through an unvalidated/guessed status address.
    let status=chain.descriptors.last().ok_or("Missing status descriptor")?;
    if chain.descriptors.len()<2||!status.writable||status.length!=1 {return Err("Missing valid one-byte writable status tail".into());}
    let (status_byte,written,error)=match execute(m,&chain,backend) {
        Ok(written)=>(0,written,None),Err(Failure::Unsupported)=>(2,1,Some("Unsupported block operation".into())),Err(Failure::Io(e))=>(1,1,Some(e)),
    };
    m.write(status.address,&[status_byte])?;
    queue.complete(m,chain.head,written)?;
    Ok(Some(Completion{head:chain.head,status:status_byte,written,notify:chain.notify,error}))
}
