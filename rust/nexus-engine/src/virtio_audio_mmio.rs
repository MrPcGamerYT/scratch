//! VirtIO-MMIO sound (PCM) register/service model; WHP adapter/IRQ wiring mirrors
//! virtio_net_mmio.rs. Synchronous service with all guest CPUs/workers stopped;
//! two direct split queues (capture queue 0, playback queue 1). Null backend by
//! default so the model is exercisable without a host audio stack.
use crate::{guest_memory::GuestMemory,virtqueue::{SplitQueue,MAX_QUEUE},virtio_audio::{self,AudioBackend,Completion,F_PCM,F_CTRL}};
const VERSION_1:u64=1<<32;
const ACK:u32=1;
const DRIVER:u32=2;
const DRIVER_OK:u32=4;
const FEATURES_OK:u32=8;
const NEEDS_RESET:u32=64;
const FAILED:u32=128;
#[derive(Default)]
struct QueueConfig {size:u16,ready:bool,desc:u64,avail:u64,used:u64,active:Option<SplitQueue>}
pub struct AudioMmio {
    features_sel:u32,driver_sel:u32,driver_features:u64,invalid_feature_page:bool,
    queue_sel:u32,rx:QueueConfig,tx:QueueConfig,status:u32,interrupt:u32,pending:bool,
}
impl AudioMmio {
    pub fn new()->Self {Self{features_sel:0,driver_sel:0,driver_features:0,invalid_feature_page:false,
        queue_sel:0,rx:QueueConfig::default(),tx:QueueConfig::default(),status:0,interrupt:0,pending:false}}
    fn offered(&self)->u64 {VERSION_1|F_PCM|F_CTRL}
    pub fn irq_asserted(&self)->bool {self.interrupt!=0}
    pub fn work_pending(&self)->bool {self.pending}
    fn queue(&self)->&QueueConfig {if self.queue_sel==0 {&self.rx} else {&self.tx}}
    fn queue_mut(&mut self)->&mut QueueConfig {if self.queue_sel==0 {&mut self.rx} else {&mut self.tx}}
    pub fn read(&self,offset:u64,size:u8)->Result<u32,String> {
        if size!=4||offset%4!=0 {return Err("VirtIO registers require aligned 32-bit access".into());}
        Ok(match offset {
            0=>0x74726976,4=>2,8=>2,0xc=>0x5358454e, // local NEXS vendor label, not a PCI allocation.
            0x10=>if self.features_sel<2 {(self.offered()>>(self.features_sel*32)) as u32} else {0},
            0x34=>u32::from(MAX_QUEUE), // two queues (capture, playback) both bounded by MAX_QUEUE.
            0x44=>u32::from(self.queue().ready),0x60=>self.interrupt,0x70=>self.status,0xfc=>0,
            _=>return Err(format!("Unsupported VirtIO-MMIO audio read 0x{offset:x}")),
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
            _=>return Err(format!("Unsupported VirtIO-MMIO audio write 0x{offset:x}")),
        }
        Ok(())
    }
    /// Bounded synchronous dispatch; never called from an instruction-emulator callback.
    pub fn service(&mut self,m:&mut GuestMemory<'_>,backend:&mut dyn AudioBackend,budget:usize)->Result<Vec<Completion>,String> {
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
                let Some(completion)=virtio_audio::process_one(m,queue,backend,rx)? else {break;};
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

impl Default for AudioMmio {fn default()->Self {Self::new()}}

/// In-process VirtIO-MMIO audio smoke: negotiate features, drive a playback
/// request through the null backend, then deliver a capture block back. No WHP
/// or guest CPU required; exercises the full register model + executor end to end.
pub fn audio_smoke()->Result<(),String> {
    use crate::guest_memory::RamRegion;
    let ram=Box::leak(Box::new(vec![0u8;0x4000]));
    let mut m=GuestMemory::new(vec![RamRegion{gpa:0,bytes:ram,writable:true}]).map_err(|e|e.to_string())?;
    let mut device=AudioMmio::new();
    device.write(0x70,4,ACK).map_err(|e|e.to_string())?;
    device.write(0x70,4,ACK|DRIVER).map_err(|e|e.to_string())?;
    device.write(0x24,4,0).map_err(|e|e.to_string())?;
    device.write(0x20,4,F_PCM as u32).map_err(|e|e.to_string())?;
    device.write(0x24,4,1).map_err(|e|e.to_string())?;
    device.write(0x20,4,(VERSION_1>>32) as u32).map_err(|e|e.to_string())?;
    device.write(0x70,4,ACK|DRIVER|FEATURES_OK).map_err(|e|e.to_string())?;
    device.write(0x70,4,ACK|DRIVER|FEATURES_OK|DRIVER_OK).map_err(|e|e.to_string())?;
    let mut backend=virtio_audio::NullBackend::new();
    // Playback queue (sel=1): descriptor table 0x1000, avail 0x1200, used 0x1400.
    let mut samples=vec![0u8;32];
    for (n,b) in samples.iter_mut().enumerate() {*b=(n as u8).wrapping_mul(13);}
    let mut td=[0u8;16];
    td[..8].copy_from_slice(&0x800u64.to_le_bytes());
    td[8..12].copy_from_slice(&32u32.to_le_bytes());
    td[12..14].copy_from_slice(&1u16.to_le_bytes()); // read-only, NEXT
    td[14..16].copy_from_slice(&1u16.to_le_bytes()); // next descriptor index
    let mut ts=[0u8;16];
    ts[..8].copy_from_slice(&0x900u64.to_le_bytes());
    ts[8..12].copy_from_slice(&1u32.to_le_bytes());
    ts[12..14].copy_from_slice(&2u16.to_le_bytes()); // writable status
    m.write(0x1000,&td).map_err(|e|e.to_string())?;
    m.write(0x1010,&ts).map_err(|e|e.to_string())?;
    m.write(0x800,&samples).map_err(|e|e.to_string())?;
    device.write(0x30,4,1).map_err(|e|e.to_string())?;
    device.write(0x38,4,8).map_err(|e|e.to_string())?;
    device.write(0x80,4,0x1000).map_err(|e|e.to_string())?;
    device.write(0x90,4,0x1200).map_err(|e|e.to_string())?;
    device.write(0xa0,4,0x1400).map_err(|e|e.to_string())?;
    device.write(0x44,4,1).map_err(|e|e.to_string())?;
    m.write(0x1200,&0u16.to_le_bytes()).map_err(|e|e.to_string())?;
    m.write(0x1202,&1u16.to_le_bytes()).map_err(|e|e.to_string())?;
    m.write(0x1204,&0u16.to_le_bytes()).map_err(|e|e.to_string())?;
    device.write(0x30,4,1).map_err(|e|e.to_string())?;
    device.write(0x50,4,0).map_err(|e|e.to_string())?;
    let tx=device.service(&mut m,&mut backend,8).map_err(|e|e.to_string())?;
    if tx.len()!=1||tx[0].status!=0 {return Err("Audio TX completion failed".into());}
    if backend.played()!=32 {return Err("Null backend did not capture playback samples".into());}
    // Capture queue (sel=0): descriptor table 0x2000, avail 0x2200, used 0x2400.
    let mut rd=[0u8;16];
    rd[..8].copy_from_slice(&0x1800u64.to_le_bytes());
    rd[8..12].copy_from_slice(&256u32.to_le_bytes());
    rd[12..14].copy_from_slice(&2u16.to_le_bytes()); // writable
    m.write(0x2000,&rd).map_err(|e|e.to_string())?;
    device.write(0x30,4,0).map_err(|e|e.to_string())?;
    device.write(0x38,4,8).map_err(|e|e.to_string())?;
    device.write(0x80,4,0x2000).map_err(|e|e.to_string())?;
    device.write(0x90,4,0x2200).map_err(|e|e.to_string())?;
    device.write(0xa0,4,0x2400).map_err(|e|e.to_string())?;
    device.write(0x44,4,1).map_err(|e|e.to_string())?;
    m.write(0x2200,&0u16.to_le_bytes()).map_err(|e|e.to_string())?;
    m.write(0x2202,&1u16.to_le_bytes()).map_err(|e|e.to_string())?;
    m.write(0x2204,&0u16.to_le_bytes()).map_err(|e|e.to_string())?;
    device.write(0x30,4,0).map_err(|e|e.to_string())?;
    device.write(0x50,4,0).map_err(|e|e.to_string())?;
    let rx=device.service(&mut m,&mut backend,8).map_err(|e|e.to_string())?;
    if rx.len()!=1||rx[0].written!=256 {return Err("Audio RX completion failed".into());}
    let mut out=[0u8;256];
    m.read(0x1800,&mut out).map_err(|e|e.to_string())?;
    if !out.iter().all(|&b|b==0) {return Err("Audio RX capture data not silence".into());}
    Ok(())
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
    fn write(d:&mut AudioMmio,offset:u64,value:u32) {d.write(offset,4,value).unwrap();}
    fn setup(d:&mut AudioMmio,m:&mut GuestMemory<'_>,sel:u32,size:u16,desc:u64,avail:u64,used:u64) {
        write(d,0x30,sel);write(d,0x38,size as u32);
        write(d,0x80,(desc&0xffffffff) as u32);write(d,0x84,(desc>>32) as u32);
        write(d,0x90,(avail&0xffffffff) as u32);write(d,0x94,(avail>>32) as u32);
        write(d,0xa0,(used&0xffffffff) as u32);write(d,0xa4,(used>>32) as u32);
        write(d,0x44,1);
    }
    fn push(m:&mut GuestMemory<'_>,avail:u64) {
        m.write(avail,&0u16.to_le_bytes()).unwrap();
        m.write(avail+2,&1u16.to_le_bytes()).unwrap();
        m.write(avail+4,&0u16.to_le_bytes()).unwrap();
    }

    #[test] fn status_negotiation_requires_ack_driver_features_then_ok() {
        let mut d=AudioMmio::new();
        assert!(d.write(0x70,4,3).is_err()); // DRIVER without ACK
        write(&mut d,0x70,1);write(&mut d,0x70,3);
        write(&mut d,0x24,0);write(&mut d,0x20,F_PCM as u32);
        write(&mut d,0x24,1);write(&mut d,0x20,(VERSION_1>>32) as u32);
        write(&mut d,0x70,11);write(&mut d,0x70,15);
        assert_eq!(d.read(0x70,4).unwrap(),15);
    }

    #[test] fn notify_before_driver_ok_is_rejected() {
        let mut d=AudioMmio::new();
        assert!(d.write(0x50,4,0).is_err());
    }

    #[test] fn tx_notify_service_and_irq() {
        let mut m=memory(0x2000);
        let mut d=AudioMmio::new();
        write(&mut d,0x70,1);write(&mut d,0x70,3);
        write(&mut d,0x24,0);write(&mut d,0x20,F_PCM as u32);
        write(&mut d,0x24,1);write(&mut d,0x20,(VERSION_1>>32) as u32);
        write(&mut d,0x70,11);write(&mut d,0x70,15);
        // TX descriptor chain
        m.write(0x1000,&desc(0x800,32,false,1)).unwrap();
        m.write(0x1010,&desc(0x900,1,true,0)).unwrap();
        let samples=[0x42u8;32];
        m.write(0x800,&samples).unwrap();
        setup(&mut d,&mut m,1,8,0x1000,0x1200,0x1400);
        push(&mut m,0x1200);
        write(&mut d,0x30,1);write(&mut d,0x50,0);
        assert!(d.work_pending());
        let mut backend=virtio_audio::NullBackend::new();
        let done=d.service(&mut m,&mut backend,8).unwrap();
        assert_eq!(done.len(),1);
        assert_eq!(done[0].status,0);
        assert!(d.irq_asserted());
        assert!(backend.played()==32);
    }

    #[test] fn rx_notify_service_delivers_capture() {
        let mut m=memory(0x2000);
        let mut d=AudioMmio::new();
        write(&mut d,0x70,1);write(&mut d,0x70,3);
        write(&mut d,0x24,0);write(&mut d,0x20,F_PCM as u32);
        write(&mut d,0x24,1);write(&mut d,0x20,(VERSION_1>>32) as u32);
        write(&mut d,0x70,11);write(&mut d,0x70,15);
        m.write(0x2000,&desc(0x1800,128,true,0)).unwrap();
        setup(&mut d,&mut m,0,8,0x2000,0x2200,0x2400);
        push(&mut m,0x2200);
        write(&mut d,0x30,0);write(&mut d,0x50,0);
        let mut backend=virtio_audio::NullBackend::new();
        let done=d.service(&mut m,&mut backend,8).unwrap();
        assert_eq!(done.len(),1);
        assert_eq!(done[0].written,128);
        let mut out=[0u8;128];
        m.read(0x1800,&mut out).unwrap();
        assert!(out.iter().all(|&b|b==0));
    }

    #[test] fn mac_config_is_readable_from_config_space() {
        // Audio has no MAC config; the vendor label and device identity are readable.
        let d=AudioMmio::new();
        assert_eq!(d.read(0,4).unwrap(),0x74726976);
        assert_eq!(d.read(4,4).unwrap(),2);
        assert_eq!(d.read(0xc,4).unwrap(),0x5358454e);
    }
}
