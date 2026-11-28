//! VirtIO-MMIO GPU register/service model; WHP adapter/IRQ wiring mirrors
//! virtio_mmio.rs. Synchronous service with all guest CPUs/workers stopped; one
//! command queue (0) and one resource-update queue (1).
//!
//! Minimal scanout contract: the guest driver writes the scanout geometry into
//! the config space (0x100 width, 0x104 height, 0x108 format, 0x10c pitch) and
//! then submits 2D commands on the command queue. Commands rasterize into the
//! device's scanout surface; SCANOUT_2D marks a frame ready.
use crate::{guest_memory::GuestMemory,virtio_gpu::{self,Completion,GpuBackend,OFFERED,FORMAT_XRGB8888},virtqueue::{SplitQueue,MAX_QUEUE}};

const ACK:u32=1;
const DRIVER:u32=2;
const DRIVER_OK:u32=4;
const FEATURES_OK:u32=8;
const NEEDS_RESET:u32=64;
const FAILED:u32=128;

/// Config-space offsets for the scanout geometry contract.
const CFG_WIDTH:u64=0x100;
const CFG_HEIGHT:u64=0x104;
const CFG_FORMAT:u64=0x108;
const CFG_PITCH:u64=0x10c;

#[derive(Default)]
struct QueueConfig {size:u16,ready:bool,desc:u64,avail:u64,used:u64,active:Option<SplitQueue>}

pub struct GpuMmio {
    features_sel:u32,driver_sel:u32,driver_features:u64,invalid_feature_page:bool,
    queue_sel:u32,command:QueueConfig,res_update:QueueConfig,status:u32,interrupt:u32,pending:bool,
    /// Scanout geometry written through the config space (0 when unset).
    scanout_width:u32,scanout_height:u32,scanout_format:u32,scanout_pitch:u32,
}

impl GpuMmio {
    pub fn new()->Self {Self{features_sel:0,driver_sel:0,driver_features:0,invalid_feature_page:false,
        queue_sel:0,command:QueueConfig::default(),res_update:QueueConfig::default(),status:0,interrupt:0,pending:false,
        scanout_width:0,scanout_height:0,scanout_format:FORMAT_XRGB8888,scanout_pitch:0}}

    pub fn irq_asserted(&self)->bool {self.interrupt!=0}
    pub fn work_pending(&self)->bool {self.pending}
    pub fn scanout_configured(&self)->bool {self.scanout_width!=0&&self.scanout_height!=0}

    fn offered(&self)->u64 {OFFERED}
    fn queue(&self)->&QueueConfig {if self.queue_sel==0 {&self.command} else {&self.res_update}}
    fn queue_mut(&mut self)->&mut QueueConfig {if self.queue_sel==0 {&mut self.command} else {&mut self.res_update}}

    pub fn read(&self,offset:u64,size:u8)->Result<u32,String> {
        if size!=4||offset%4!=0 {return Err("VirtIO registers require aligned 32-bit access".into());}
        Ok(match offset {
            0=>virtio_gpu::MAGIC,4=>virtio_gpu::VERSION,8=>virtio_gpu::DEVICE_ID,0xc=>virtio_gpu::VENDOR_ID,
            0x10=>if self.features_sel<2 {(self.offered()>>(self.features_sel*32)) as u32} else {0},
            0x34=>u32::from(MAX_QUEUE), // two queues (command, resource-update) both bounded by MAX_QUEUE.
            0x44=>u32::from(self.queue().ready),0x48=>0,0x58=>0, // no readable-queue counts beyond selection.
            0x60=>self.interrupt,0x70=>self.status,0xfc=>0,
            CFG_WIDTH=>self.scanout_width,CFG_HEIGHT=>self.scanout_height,CFG_FORMAT=>self.scanout_format,CFG_PITCH=>self.scanout_pitch,
            _=>return Err(format!("Unsupported VirtIO-MMIO gpu read 0x{offset:x}")),
        })
    }

    fn set_status(&mut self,value:u32)->Result<(),String> {
        if value==0 {
            self.features_sel=0;self.driver_sel=0;self.driver_features=0;self.invalid_feature_page=false;
            self.queue_sel=0;self.command=QueueConfig::default();self.res_update=QueueConfig::default();
            self.status=0;self.interrupt=0;self.pending=false;
            self.scanout_width=0;self.scanout_height=0;self.scanout_pitch=0;
            return Ok(());
        }
        if self.status&(FAILED|NEEDS_RESET)!=0 {return Err("VirtIO device requires full reset".into());}
        if value&!(ACK|DRIVER|DRIVER_OK|FEATURES_OK|FAILED)!=0||value&self.status!=self.status {return Err("Invalid/non-monotonic device status".into());}
        if value&DRIVER!=0&&value&ACK==0 {return Err("DRIVER requires ACKNOWLEDGE".into());}
        if value&FEATURES_OK!=0 {
            if value&(ACK|DRIVER)!=(ACK|DRIVER) {return Err("FEATURES_OK requires DRIVER".into());}
            if self.driver_features&1==0||self.driver_features&!self.offered()!=0||self.invalid_feature_page {
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
                    0x44=>{if value>1 {return Err("QueueReady must be 0 or 1".into());}q.ready=value!=0;}
                    _=>{
                        let address=match offset {0x80|0x84=>&mut q.desc,0x90|0x94=>&mut q.avail,_=>&mut q.used};
                        if offset&4==0 {*address=(*address&0xffff_ffff_0000_0000)|u64::from(value);} else {*address=(*address&0xffff_ffff)|(u64::from(value)<<32);}
                    }
                }
            }
            CFG_WIDTH|CFG_HEIGHT|CFG_FORMAT|CFG_PITCH=>{
                if self.status&DRIVER_OK==0 {return Err("Scanout geometry requires DRIVER_OK".into());}
                match offset {
                    CFG_WIDTH=>self.scanout_width=value,
                    CFG_HEIGHT=>self.scanout_height=value,
                    CFG_FORMAT=>self.scanout_format=value,
                    _=>self.scanout_pitch=value,
                }
            }
            _=>return Err(format!("Unsupported VirtIO-MMIO gpu write 0x{offset:x}")),
        }
        Ok(())
    }

    /// Bounded synchronous dispatch, never called directly from an
    /// instruction-emulator callback. Serves the command queue.
    pub fn service(&mut self,m:&mut GuestMemory<'_>,backend:&mut dyn GpuBackend,budget:usize)->Result<Vec<Completion>,String> {
        if budget==0||budget>MAX_QUEUE as usize {return Err("Service budget must be 1..256".into());}
        if self.status&(DRIVER_OK|FAILED|NEEDS_RESET)!=DRIVER_OK||!self.command.ready {return Err("VirtIO gpu device/queue not running".into());}
        if !self.pending {return Ok(Vec::new());}
        let result=(|| {
            if self.command.active.is_none() {self.command.active=Some(SplitQueue::new(m,self.command.size,self.command.desc,self.command.avail,self.command.used)?);}
            let queue=self.command.active.as_mut().ok_or("Missing active command queue")?;
            let mut done=Vec::new();
            for _ in 0..budget {
                let Some(completion)=virtio_gpu::process_one(m,queue,backend)? else {break;};
                if completion.notify {self.interrupt|=1;}
                done.push(completion);
            }
            self.pending=queue.peek(m)?.is_some();
            Ok(done)
        })();
        if result.is_err() {self.status|=NEEDS_RESET;self.interrupt|=2;self.pending=false;}
        result
    }

    /// Materialize the negotiated scanout surface on the backend from the
    /// config-space geometry. Called by the WHP layer before service dispatch.
    pub fn sync_scanout(&mut self,backend:&mut dyn GpuBackend)->Result<(),String> {
        if !self.scanout_configured() {return Ok(());}
        let needs_resize={
            let s=backend.scanout();
            s.width!=self.scanout_width||s.height!=self.scanout_height||s.format!=self.scanout_format||s.pitch!=self.scanout_pitch
        };
        if needs_resize {
            let s=backend.scanout_mut();
            *s=virtio_gpu::Surface::allocate(self.scanout_width,self.scanout_height,self.scanout_format,self.scanout_pitch)?;
        }
        Ok(())
    }
}

impl Default for GpuMmio {fn default()->Self {Self::new()}}
