//! VirtIO-MMIO net register/service model; WHP adapter/IRQ wiring mirrors virtio_mmio.rs.
//! Synchronous service with all guest CPUs/workers stopped; two direct split queues
//! (RX queue 0, TX queue 1). Loopback backend by default so the model is exercisable
//! without a host network stack.
use crate::{guest_memory::GuestMemory,virtqueue::{SplitQueue,MAX_QUEUE},virtio_net::{self,NetBackend,Completion,F_MAC,HDR_LEN,LOOPBACK_MAC}};
const VERSION_1:u64=1<<32;
const ACK:u32=1;
const DRIVER:u32=2;
const DRIVER_OK:u32=4;
const FEATURES_OK:u32=8;
const NEEDS_RESET:u32=64;
const FAILED:u32=128;
#[derive(Default)]
struct QueueConfig {size:u16,ready:bool,desc:u64,avail:u64,used:u64,active:Option<SplitQueue>}
pub struct NetMmio {
    features_sel:u32,driver_sel:u32,driver_features:u64,invalid_feature_page:bool,
    queue_sel:u32,rx:QueueConfig,tx:QueueConfig,status:u32,interrupt:u32,pending:bool,
}
impl NetMmio {
    pub fn new()->Self {Self{features_sel:0,driver_sel:0,driver_features:0,invalid_feature_page:false,
        queue_sel:0,rx:QueueConfig::default(),tx:QueueConfig::default(),status:0,interrupt:0,pending:false}}
    fn offered(&self)->u64 {VERSION_1|F_MAC}
    pub fn irq_asserted(&self)->bool {self.interrupt!=0}
    pub fn work_pending(&self)->bool {self.pending}
    fn queue(&self)->&QueueConfig {if self.queue_sel==0 {&self.rx} else {&self.tx}}
    fn queue_mut(&mut self)->&mut QueueConfig {if self.queue_sel==0 {&mut self.rx} else {&mut self.tx}}
    pub fn read(&self,offset:u64,size:u8)->Result<u32,String> {
        if (0x100..0x106).contains(&offset)&&[1,2,4].contains(&size) {
            if offset+u64::from(size)>0x106 {return Err("Config read crosses MAC space".into());}
            let mut out=[0;4];let at=(offset-0x100) as usize;
            out[..size as usize].copy_from_slice(&LOOPBACK_MAC[at..at+size as usize]);
            return Ok(u32::from_le_bytes(out));
        }
        if size!=4||offset%4!=0 {return Err("VirtIO registers require aligned 32-bit access".into());}
        Ok(match offset {
            0=>0x74726976,4=>2,8=>2,0xc=>0x5358454e, // local NEXS vendor label, not a PCI allocation.
            0x10=>if self.features_sel<2 {(self.offered()>>(self.features_sel*32)) as u32} else {0},
            0x34=>u32::from(MAX_QUEUE), // two queues (RX, TX) both bounded by MAX_QUEUE.
            0x44=>u32::from(self.queue().ready),0x60=>self.interrupt,0x70=>self.status,0xfc=>0,
            _=>return Err(format!("Unsupported VirtIO-MMIO net read 0x{offset:x}")),
        })
    }
    fn set_status(&mut self,value:u32)->Result<(),String> {
        if value==0 {
            self.features_sel=0;self.driver_sel=0;self.driver_features=0;self.invalid_feature_page=false;
            self.queue_sel=0;self.rx=QueueConfig::default();self.tx=QueueConfig::default();self.status=0;self.interrupt=0;self.pending=false;return Ok(());
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
                if value!=0||!self.queue().ready||self.status&DRIVER_OK==0 {return Err("Notify for unavailable/not-started queue".into());}
                if self.queue_sel>1 {return Err("Invalid queue selection".into());}
                self.pending=true;
            }
            0x38|0x44|0x80|0x84|0x90|0x94|0xa0|0xa4=>{
                if self.queue_sel>1||self.status&FEATURES_OK==0 {return Err("Unavailable queue or features not accepted".into());}
                if offset==0x44&&value==0 {let q=self.queue_mut();q.ready=false;q.active=None;self.pending=false;return Ok(());}
                if self.queue().ready {
                    if offset==0x44&&value==1 {return Ok(());}
                    return Err("Live queue cannot be reprogrammed".into());
                }
                let q=self.queue_mut();
                match offset {
                    0x38=>{if value==0||value>u32::from(MAX_QUEUE)||!value.is_power_of_two() {return Err("Invalid queue size".into());}q.size=value as u16;}
                    0x44=>{
                        if value>1 {return Err("QueueReady must be 0 or 1".into());}
                        if value==1 {
                            if q.size==0||q.desc%16!=0||q.avail%2!=0||q.used%4!=0 {return Err("Invalid queue layout".into());}
                            q.ready=true;
                        }
                    }
                    _=>{
                        let address=match offset {0x80|0x84=>&mut q.desc,0x90|0x94=>&mut q.avail,_=>&mut q.used};
                        if offset&4==0 {*address=(*address&0xffff_ffff_0000_0000)|u64::from(value);} else {*address=(*address&0xffff_ffff)|(u64::from(value)<<32);}
                    }
                }
            }
            _=>return Err(format!("Unsupported VirtIO-MMIO net write 0x{offset:x}")),
        }
        Ok(())
    }
    /// Bounded synchronous dispatch; never called from an instruction-emulator callback.
    pub fn service(&mut self,m:&mut GuestMemory<'_>,backend:&mut dyn NetBackend,budget:usize)->Result<Vec<Completion>,String> {
        if budget==0||budget>MAX_QUEUE as usize {return Err("Service budget must be 1..256".into());}
        if self.status&(DRIVER_OK|FAILED|NEEDS_RESET)!=DRIVER_OK||!self.queue().ready {return Err("VirtIO device/queue not running".into());}
        if !self.pending {return Ok(Vec::new());}
        let rx=self.queue_sel==0;
        let result=(|| {
            if self.queue().active.is_none() {
                let q=self.queue();
                self.queue_mut().active=Some(SplitQueue::new(m,q.size,q.desc,q.avail,q.used)?);
            }
            let queue=self.queue_mut().active.as_mut().ok_or("Missing active queue")?;
            let mut done=Vec::new();
            let mut notify=false;
            for _ in 0..budget {
                let Some(completion)=virtio_net::process_one(m,queue,backend,rx)? else {break;};
                if completion.notify {notify=true;}
                done.push(completion);
            }
            let still_pending=queue.peek(m)?.is_some();
            if notify {self.interrupt|=1;}
            self.pending=still_pending;
            Ok(done)
        })();
        if result.is_err() {self.status|=NEEDS_RESET;self.interrupt|=2;self.pending=false;}
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guest_memory::RamRegion;
    fn memory(bytes:usize)->GuestMemory<'static> {
        let ram=Box::leak(Box::new(vec![0u8;bytes]));
        GuestMemory::new(vec![RamRegion{gpa:0,bytes:ram,writable:true}]).unwrap()
    }
    fn desc(address:u64,length:u32,writable:bool,next_index:u16)->[u8;16] {
        let mut raw=[0;16];
        raw[..8].copy_from_slice(&address.to_le_bytes());
        raw[8..12].copy_from_slice(&length.to_le_bytes());
        let has_next=next_index!=0;
        let flags=(if writable {2u16} else {0})|(if has_next {1u16} else {0});
        raw[12..14].copy_from_slice(&flags.to_le_bytes());
        raw[14..16].copy_from_slice(&next_index.to_le_bytes());
        raw
    }
    fn negotiate(device:&mut NetMmio) {
        device.write(0x70,4,ACK).unwrap();
        device.write(0x70,4,ACK|DRIVER).unwrap();
        device.write(0x24,4,0).unwrap(); // driver_sel = 0 (low word)
        device.write(0x20,4,F_MAC as u32).unwrap(); // low word = F_MAC
        device.write(0x24,4,1).unwrap(); // driver_sel = 1 (high word)
        device.write(0x20,4,(VERSION_1>>32) as u32).unwrap(); // high word = VERSION_1
        device.write(0x70,4,ACK|DRIVER|FEATURES_OK).unwrap();
        device.write(0x70,4,ACK|DRIVER|FEATURES_OK|DRIVER_OK).unwrap();
    }
    fn setup_queue(device:&mut NetMmio,m:&GuestMemory<'_>,sel:u32,size:u16,desc:u64,avail:u64,used:u64) {
        device.write(0x30,4,sel).unwrap();
        device.write(0x38,4,size as u32).unwrap();
        device.write(0x80,4,(desc&0xffff_ffff) as u32).unwrap();
        device.write(0x84,4,(desc>>32) as u32).unwrap();
        device.write(0x90,4,(avail&0xffff_ffff) as u32).unwrap();
        device.write(0x94,4,(avail>>32) as u32).unwrap();
        device.write(0xa0,4,(used&0xffff_ffff) as u32).unwrap();
        device.write(0xa4,4,(used>>32) as u32).unwrap();
        device.write(0x44,4,1).unwrap();
        // make one descriptor available
        let _=m;
    }
    fn push_avail(m:&mut GuestMemory<'_>,avail:u64,head:u16) {
        m.write(avail,&0u16.to_le_bytes()).unwrap();
        m.write(avail+2,&1u16.to_le_bytes()).unwrap();
        m.write(avail+4,&head.to_le_bytes()).unwrap();
    }

    #[test] fn mac_config_is_readable_from_config_space() {
        let device=NetMmio::new();
        assert_eq!(device.read(0x100,1).unwrap(),u32::from(LOOPBACK_MAC[0]));
        assert_eq!(device.read(0x105,1).unwrap(),u32::from(LOOPBACK_MAC[5]));
    }

    #[test] fn status_negotiation_requires_ack_driver_features_then_ok() {
        let mut device=NetMmio::new();
        assert!(device.write(0x70,4,DRIVER).is_err()); // DRIVER without ACK
        negotiate(&mut device);
        assert_eq!(device.read(0x70,4).unwrap(),ACK|DRIVER|FEATURES_OK|DRIVER_OK);
    }

    #[test] fn tx_notify_service_and_irq() {
        let mut m=memory(0x4000);
        let mut device=NetMmio::new();
        negotiate(&mut device);
        // TX queue (sel=1): descriptor table 0x1000, avail 0x1200, used 0x1400
        m.write(0x1000,&desc(0x800,(HDR_LEN+4) as u32,false,1)).unwrap();
        m.write(0x1010,&desc(0x900,1,true,0)).unwrap();
        let mut packet=vec![0u8;HDR_LEN+4];
        packet[..HDR_LEN].copy_from_slice(&virtio_net::net_header());
        packet[HDR_LEN..].copy_from_slice(b"PING");
        m.write(0x800,&packet).unwrap();
        setup_queue(&mut device,&m,1,8,0x1000,0x1200,0x1400);
        push_avail(&mut m,0x1200,0);
        device.write(0x30,4,1).unwrap();
        device.write(0x50,4,0).unwrap();
        assert!(device.irq_asserted()==false);
        let mut backend=virtio_net::Loopback::new();
        let done=device.service(&mut m,&mut backend,8).unwrap();
        assert_eq!(done.len(),1);
        assert_eq!(done[0].status,0);
        assert!(device.irq_asserted());
        assert_eq!(backend.pending(),1);
    }

    #[test] fn rx_notify_service_delivers_loopback_frame() {
        let mut m=memory(0x4000);
        let mut device=NetMmio::new();
        negotiate(&mut device);
        let mut backend=virtio_net::Loopback::new();
        backend.transmit(b"PONG").unwrap();
        // RX queue (sel=0): descriptor table 0x1000, avail 0x1200, used 0x1400
        m.write(0x1000,&desc(0x800,64,true,0)).unwrap();
        setup_queue(&mut device,&m,0,8,0x1000,0x1200,0x1400);
        push_avail(&mut m,0x1200,0);
        device.write(0x30,4,0).unwrap();
        device.write(0x50,4,0).unwrap();
        let done=device.service(&mut m,&mut backend,8).unwrap();
        assert_eq!(done.len(),1);
        assert_eq!(done[0].written,(HDR_LEN+4) as u32);
        let mut out=[0u8;HDR_LEN+4];
        m.read(0x800,&mut out).unwrap();
        assert_eq!(&out[HDR_LEN..],b"PONG");
    }

    #[test] fn notify_before_driver_ok_is_rejected() {
        let mut device=NetMmio::new();
        assert!(device.write(0x50,4,0).is_err());
    }
}

/// In-process VirtIO-MMIO net smoke: negotiate features, drive a TX frame through
/// the loopback backend, then deliver it back through the RX queue. No WHP or guest
/// CPU required; exercises the full register model + executor end to end.
pub fn net_smoke()->Result<(),String> {
    use crate::guest_memory::RamRegion;
    let ram=Box::leak(Box::new(vec![0u8;0x4000]));
    let mut m=GuestMemory::new(vec![RamRegion{gpa:0,bytes:ram,writable:true}]).map_err(|e|e.to_string())?;
    let mut device=NetMmio::new();
    device.write(0x70,4,ACK).map_err(|e|e.to_string())?;
    device.write(0x70,4,ACK|DRIVER).map_err(|e|e.to_string())?;
    device.write(0x24,4,0).map_err(|e|e.to_string())?;
    device.write(0x20,4,F_MAC as u32).map_err(|e|e.to_string())?;
    device.write(0x24,4,1).map_err(|e|e.to_string())?;
    device.write(0x20,4,(VERSION_1>>32) as u32).map_err(|e|e.to_string())?;
    device.write(0x70,4,ACK|DRIVER|FEATURES_OK).map_err(|e|e.to_string())?;
    device.write(0x70,4,ACK|DRIVER|FEATURES_OK|DRIVER_OK).map_err(|e|e.to_string())?;
    let mut backend=virtio_net::Loopback::new();
    // TX queue (sel=1): descriptor table 0x1000, avail 0x1200, used 0x1400.
    let mut packet=vec![0u8;HDR_LEN+5];
    packet[..HDR_LEN].copy_from_slice(&virtio_net::net_header());
    packet[HDR_LEN..].copy_from_slice(b"NEXUS");
    let mut td=[0u8;16];
    td[..8].copy_from_slice(&0x800u64.to_le_bytes());
    td[8..12].copy_from_slice(&((HDR_LEN+5) as u32).to_le_bytes());
    td[12..14].copy_from_slice(&0u16.to_le_bytes()); // read-only
    let mut ts=[0u8;16];
    ts[..8].copy_from_slice(&0x900u64.to_le_bytes());
    ts[8..12].copy_from_slice(&1u32.to_le_bytes());
    ts[12..14].copy_from_slice(&2u16.to_le_bytes()); // writable status
    m.write(0x1000,&td).map_err(|e|e.to_string())?;
    m.write(0x1010,&ts).map_err(|e|e.to_string())?;
    m.write(0x800,&packet).map_err(|e|e.to_string())?;
    device.write(0x30,4,1).map_err(|e|e.to_string())?;
    device.write(0x38,4,8).map_err(|e|e.to_string())?;
    device.write(0x80,4,0x1000).map_err(|e|e.to_string())?;
    device.write(0x90,4,0x1200).map_err(|e|e.to_string())?;
    device.write(0xa0,4,0x1400).map_err(|e|e.to_string())?;
    device.write(0x44,4,1).map_err(|e|e.to_string())?;
    m.write(0x1200,&1u16.to_le_bytes()).map_err(|e|e.to_string())?;
    m.write(0x1202,&1u16.to_le_bytes()).map_err(|e|e.to_string())?;
    m.write(0x1204,&0u16.to_le_bytes()).map_err(|e|e.to_string())?;
    device.write(0x30,4,1).map_err(|e|e.to_string())?;
    device.write(0x50,4,0).map_err(|e|e.to_string())?;
    let tx=device.service(&mut m,&mut backend,8).map_err(|e|e.to_string())?;
    if tx.len()!=1||tx[0].status!=0 {return Err("Net TX completion failed".into());}
    if backend.pending()!=1 {return Err("Loopback backend did not capture TX frame".into());}
    // RX queue (sel=0): descriptor table 0x2000, avail 0x2200, used 0x2400.
    let mut rd=[0u8;16];
    rd[..8].copy_from_slice(&0x1800u64.to_le_bytes());
    rd[8..12].copy_from_slice(&64u32.to_le_bytes());
    rd[12..14].copy_from_slice(&2u16.to_le_bytes()); // writable
    m.write(0x2000,&rd).map_err(|e|e.to_string())?;
    device.write(0x30,4,0).map_err(|e|e.to_string())?;
    device.write(0x38,4,8).map_err(|e|e.to_string())?;
    device.write(0x80,4,0x2000).map_err(|e|e.to_string())?;
    device.write(0x90,4,0x2200).map_err(|e|e.to_string())?;
    device.write(0xa0,4,0x2400).map_err(|e|e.to_string())?;
    device.write(0x44,4,1).map_err(|e|e.to_string())?;
    m.write(0x2200,&1u16.to_le_bytes()).map_err(|e|e.to_string())?;
    m.write(0x2202,&1u16.to_le_bytes()).map_err(|e|e.to_string())?;
    m.write(0x2204,&0u16.to_le_bytes()).map_err(|e|e.to_string())?;
    device.write(0x30,4,0).map_err(|e|e.to_string())?;
    device.write(0x50,4,0).map_err(|e|e.to_string())?;
    let rx=device.service(&mut m,&mut backend,8).map_err(|e|e.to_string())?;
    if rx.len()!=1||rx[0].written!=(HDR_LEN+5) as u32 {return Err("Net RX completion failed".into());}
    let mut out=[0u8;HDR_LEN+5];
    m.read(0x1800,&mut out).map_err(|e|e.to_string())?;
    if &out[HDR_LEN..]!=b"NEXUS" {return Err("Loopback RX frame data mismatch".into());}
    Ok(())
}
