//! Modern VirtIO-MMIO block register/service model; WHP adapter/IRQ wiring lives in whp_mmio.rs.
//! Synchronous service with all guest CPUs/workers stopped; one direct split queue.
use crate::{guest_memory::GuestMemory,virtqueue::{SplitQueue,MAX_QUEUE,MAX_TRANSFER},virtio_block::{BlockBackend,Completion,process_one_negotiated}};
const VERSION_1:u64=1<<32;
const FLUSH:u64=1<<9;
const RO:u64=1<<5;
const ACK:u32=1;
const DRIVER:u32=2;
const DRIVER_OK:u32=4;
const FEATURES_OK:u32=8;
const NEEDS_RESET:u32=64;
const FAILED:u32=128;
#[derive(Default)]
struct QueueConfig {size:u16,ready:bool,desc:u64,avail:u64,used:u64,active:Option<SplitQueue>}
pub struct BlockMmio {
    capacity:u64,readonly:bool,features_sel:u32,driver_sel:u32,driver_features:u64,
    invalid_feature_page:bool,queue_sel:u32,queue:QueueConfig,status:u32,interrupt:u32,pending:bool,
}
impl BlockMmio {
    pub fn new(capacity_sectors:u64,readonly:bool)->Result<Self,String> {
        if capacity_sectors==0||capacity_sectors>u64::MAX/512 {return Err("Invalid VirtIO disk capacity".into());}
        Ok(Self{capacity:capacity_sectors,readonly,features_sel:0,driver_sel:0,driver_features:0,invalid_feature_page:false,
            queue_sel:0,queue:QueueConfig::default(),status:0,interrupt:0,pending:false})
    }
    fn offered(&self)->u64 {VERSION_1|FLUSH|6|if self.readonly {RO} else {0}}
    pub fn irq_asserted(&self)->bool {self.interrupt!=0}
    pub fn work_pending(&self)->bool {self.pending}
    pub fn read(&self,offset:u64,size:u8)->Result<u32,String> {
        if (0x100..0x200).contains(&offset)&&[1,2,4].contains(&size) {
            if offset+u64::from(size)>0x200 {return Err("Config read crosses device config space".into());}
            let mut bytes=[0;256];bytes[..8].copy_from_slice(&self.capacity.to_le_bytes());
            bytes[8..12].copy_from_slice(&(MAX_TRANSFER as u32).to_le_bytes());
            bytes[12..16].copy_from_slice(&1u32.to_le_bytes()); // one data segment bounds aggregate request size.
            let mut out=[0;4];let at=(offset-0x100) as usize;
            out[..size as usize].copy_from_slice(&bytes[at..at+size as usize]);return Ok(u32::from_le_bytes(out));
        }
        if size!=4||offset%4!=0 {return Err("VirtIO registers require aligned 32-bit access".into());}
        let selected=self.queue_sel==0;
        Ok(match offset {
            0=>0x74726976,4=>2,8=>2,0xc=>0x5358454e, // local NEXS vendor label, not a PCI allocation.
            0x10=>if self.features_sel<2 {(self.offered()>>(self.features_sel*32)) as u32} else {0},
            0x34=>if selected {u32::from(MAX_QUEUE)} else {0},
            0x44=>u32::from(selected&&self.queue.ready),0x60=>self.interrupt,0x70=>self.status,0xfc=>0,
            _=>return Err(format!("Unsupported VirtIO-MMIO read 0x{offset:x}")),
        })
    }
    fn set_status(&mut self,value:u32)->Result<(),String> {
        if value==0 {
            self.features_sel=0;self.driver_sel=0;self.driver_features=0;self.invalid_feature_page=false;
            self.queue_sel=0;self.queue=QueueConfig::default();self.status=0;self.interrupt=0;self.pending=false;return Ok(());
        }
        if self.status&(FAILED|NEEDS_RESET)!=0 {return Err("VirtIO device requires full reset".into());}
        if value&!(ACK|DRIVER|DRIVER_OK|FEATURES_OK|FAILED)!=0||value&self.status!=self.status {return Err("Invalid/non-monotonic device status".into());}
        if value&DRIVER!=0&&value&ACK==0 {return Err("DRIVER requires ACKNOWLEDGE".into());}
        if value&FEATURES_OK!=0 {
            if value&(ACK|DRIVER)!=(ACK|DRIVER) {return Err("FEATURES_OK requires DRIVER".into());}
            if self.driver_features&VERSION_1==0||self.driver_features&!self.offered()!=0||self.invalid_feature_page {
                self.status=value&!(FEATURES_OK|DRIVER_OK);return Ok(());
            }
        }
        if value&DRIVER_OK!=0&&(self.status&FEATURES_OK==0||value&FEATURES_OK==0) {return Err("DRIVER_OK before accepted FEATURES_OK".into());}
        self.status=value;Ok(())
    }
    pub fn write(&mut self,offset:u64,size:u8,value:u32)->Result<(),String> {
        if size!=4||offset%4!=0 {return Err("VirtIO registers require aligned 32-bit access".into());}
        if offset==0x70 {return self.set_status(value);}
        if offset==0x64 {self.interrupt&=!(value&3);return Ok(());}
        if self.status&(FAILED|NEEDS_RESET)!=0 {return Err("VirtIO device requires reset".into());}
        match offset {
            0x14=>self.features_sel=value,
            0x24=>self.driver_sel=value,
            0x20=>{
                if self.status&DRIVER==0||self.status&FEATURES_OK!=0 {return Err("Feature write outside negotiation".into());}
                if self.driver_sel<2 {let shift=self.driver_sel*32;self.driver_features=(self.driver_features&!(0xffff_ffffu64<<shift))|(u64::from(value)<<shift);}
                else if value!=0 {self.invalid_feature_page=true;}
            }
            0x30=>self.queue_sel=value,
            0x50=>{
                if value!=0||!self.queue.ready||self.status&DRIVER_OK==0 {return Err("Notify for unavailable/not-started queue".into());}
                self.pending=true;
            }
            0x38|0x44|0x80|0x84|0x90|0x94|0xa0|0xa4=>{
                if self.queue_sel!=0||self.status&FEATURES_OK==0 {return Err("Unavailable queue or features not accepted".into());}
                if offset==0x44&&value==0 {self.queue.ready=false;self.queue.active=None;self.pending=false;return Ok(());}
                if self.queue.ready {
                    if offset==0x44&&value==1 {return Ok(());}
                    return Err("Live queue cannot be reprogrammed".into());
                }
                match offset {
                    0x38=>{if value==0||value>u32::from(MAX_QUEUE)||!value.is_power_of_two() {return Err("Invalid queue size".into());}self.queue.size=value as u16;}
                    0x44=>{
                        if value>1 {return Err("QueueReady must be 0 or 1".into());}
                        if value==1 {
                            if self.queue.size==0||self.queue.desc%16!=0||self.queue.avail%2!=0||self.queue.used%4!=0 {return Err("Invalid queue layout".into());}
                            self.queue.ready=true; // RAM ranges are checked before any service/IO.
                        }
                    }
                    _=>{
                        let address=match offset {0x80|0x84=>&mut self.queue.desc,0x90|0x94=>&mut self.queue.avail,_=>&mut self.queue.used};
                        if offset&4==0 {*address=(*address&0xffff_ffff_0000_0000)|u64::from(value);} else {*address=(*address&0xffff_ffff)|(u64::from(value)<<32);}
                    }
                }
            }
            _=>return Err(format!("Unsupported VirtIO-MMIO write 0x{offset:x}")),
        }Ok(())
    }
    /// Bounded synchronous dispatch, never called directly from an instruction-emulator callback.
    /// Backend must match advertised capacity/read-only policy; wrappers below enforce negotiated RO/FLUSH.
    pub fn service(&mut self,m:&mut GuestMemory<'_>,backend:&mut dyn BlockBackend,budget:usize)->Result<Vec<Completion>,String> {
        if budget==0||budget>MAX_QUEUE as usize {return Err("Service budget must be 1..256".into());}
        if self.status&(DRIVER_OK|FAILED|NEEDS_RESET)!=DRIVER_OK||!self.queue.ready {return Err("VirtIO device/queue not running".into());}
        if !self.pending {return Ok(Vec::new());}
        let result=(|| {
            if backend.capacity_sectors()!=self.capacity {return Err("Backing capacity changed".into());}
            if self.queue.active.is_none() {self.queue.active=Some(SplitQueue::new(m,self.queue.size,self.queue.desc,self.queue.avail,self.queue.used)?);}
            let queue=self.queue.active.as_mut().ok_or("Missing active queue")?;
            let mut policy=Policy{backend,readonly:self.readonly};let mut done=Vec::new();
            for _ in 0..budget {
                let Some(completion)=process_one_negotiated(m,queue,&mut policy,self.driver_features&FLUSH!=0)? else {break;};
                if completion.notify {self.interrupt|=1;}done.push(completion);
            }
            self.pending=queue.peek(m)?.is_some();Ok(done)
        })();
        if result.is_err() {self.status|=NEEDS_RESET;self.interrupt|=2;self.pending=false;}
        result
    }
}
struct Policy<'a> {backend:&'a mut dyn BlockBackend,readonly:bool}
impl BlockBackend for Policy<'_> {
    fn capacity_sectors(&self)->u64 {self.backend.capacity_sectors()}
    fn read(&mut self,s:u64,d:&mut [u8])->Result<(),String> {self.backend.read(s,d)}
    fn write(&mut self,s:u64,d:&[u8])->Result<(),String> {if self.readonly {Err("VirtIO device is read-only".into())} else {self.backend.write(s,d)}}
    fn flush(&mut self)->Result<(),String> {self.backend.flush()}
}
