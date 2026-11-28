//! Minimal VirtIO GPU device (2D commands, virtio-gpu specification 1.0).
//!
//! This is the guest-facing GPU acceleration transport for the Nexus engine: the
//! guest driver (DRM/Mesa HAL later) submits 2D commands on a command queue and
//! the device rasterizes them into a scanout surface. It is a real acceleration
//! transport -- commands execute on the device side -- and is distinct from the
//! fixed software scanout in `crate::framebuffer`, which exists only for
//! software-renderer boot and capture.
//!
//! Supported surface: a single 2D scanout surface (not a 3D context), one command
//! queue and one resource-update queue. The command set covers what a minimal
//! DRM/Mesa 2D path needs to present a frame: resource update, transfer to/from
//! host, composite, fill, and scanout.
use crate::{guest_memory::GuestMemory,virtqueue::{Chain,SplitQueue,MAX_TRANSFER}};

/// VirtIO-MMIO device identity (same register map the engine uses for its other
/// VirtIO-MMIO devices).
pub const MAGIC:u32=0x74726976; // "virt"
pub const VERSION:u32=2;
pub const DEVICE_ID:u32=0x10; // virtio-gpu
pub const VENDOR_ID:u32=0x5358454e; // "NEXS" vendor label (not a PCI ID).

/// Feature bits advertised by this device.
pub const F_VERSION_1:u64=1<<32;
pub const F_FORMAT_LIST:u64=1<<0;
pub const F_RESOURCE_ID_LIST:u64=1<<1;
pub const F_CTRL_QUEUE:u64=1<<3;
pub const F_RESOURCE_CUBE:u64=1<<4;
/// Combined feature bits advertised by this device.
pub const OFFERED:u64=F_VERSION_1|F_FORMAT_LIST|F_RESOURCE_ID_LIST|F_CTRL_QUEUE|F_RESOURCE_CUBE;

/// Guest-visible pixel formats (VirtIO-GPU 1.0, `VIRTIO_GPU_FORMAT_*`).
pub const FORMAT_XRGB8888:u32=0x00000003;
pub const FORMAT_ARGB8888:u32=0x00000004;

/// 2D command opcodes (VirtIO-GPU 1.0, `VIRTIO_GPU_CMD_*`).
pub const CMD_TRANSFER_TO_HOST2D:u32=0x00;
pub const CMD_RESOURCE_UPDATE2D:u32=0x01;
pub const CMD_TRANSFER_FROM_HOST2D:u32=0x02;
pub const CMD_COMPOSITE2D:u32=0x03;
pub const CMD_FILL2D:u32=0x04;
pub const CMD_SCANOUT2D:u32=0x05;

/// One scanout surface. Width/height/format are the contract the guest driver
/// negotiated; the pitch is in bytes, and `data` is row-major (pitch rows).
#[derive(Clone)]
pub struct Surface {
    pub width:u32,
    pub height:u32,
    pub format:u32,
    pub pitch:u32,
    pub data:Vec<u8>,
}

impl Surface {
    /// Allocate a zeroed scanout surface, validating geometry, pitch and format.
    pub fn allocate(width:u32,height:u32,format:u32,pitch:u32)->Result<Self,String> {
        let bpp=4u32;
        if width==0||height==0||pitch<width*bpp {return Err("Scanout surface has invalid geometry".into());}
        if format!=FORMAT_XRGB8888&&format!=FORMAT_ARGB8888 {return Err(format!("Unsupported pixel format 0x{format:08x}"));}
        Ok(Self{width,height,format,pitch,data:vec![0;(pitch as usize).checked_mul(height as usize).ok_or("Scanout surface size overflow")?]})
    }
}

/// The backend a VirtIO GPU device rasterizes onto. Kept behind a trait so a
/// future display-controller backend can replace the built-in scanout surface.
pub trait GpuBackend {
    fn scanout(&self)->&Surface;
    fn scanout_mut(&mut self)->&mut Surface;
    /// Bytes copied from the scanout surface to a caller-supplied buffer.
    fn readback(&self,x:u32,y:u32,width:u32,height:u32,out:&mut Vec<u8>)->Result<(),String>;
    /// Bytes copied from a caller-supplied buffer to the scanout surface.
    fn blit_to_scanout(&mut self,x:u32,y:u32,width:u32,height:u32,data:&[u8])->Result<(),String>;
}

/// Default backend: the device's own scanout surface, addressable as pixels.
pub struct ScanoutBackend {pub surface:Surface}
impl GpuBackend for ScanoutBackend {
    fn scanout(&self)->&Surface {&self.surface}
    fn scanout_mut(&mut self)->&mut Surface {&mut self.surface}
    fn readback(&self,x:u32,y:u32,width:u32,height:u32,out:&mut Vec<u8>)->Result<(),String> {
        let s=&self.surface;
        if x+width>s.width||y+height>s.height||width==0||height==0 {return Err("Readback region out of scanout bounds".into());}
        out.clear();
        let bytes=(width as usize)*4;
        for row in 0..height {
            let src=y+row;
            let start=(src as usize)*s.pitch as usize+(x as usize)*4;
            out.extend_from_slice(&s.data[start..start+bytes]);
        }
        Ok(())
    }
    fn blit_to_scanout(&mut self,x:u32,y:u32,width:u32,height:u32,data:&[u8])->Result<(),String> {
        let s=&mut self.surface;
        let bytes=(width as usize).checked_mul(4).ok_or("Blit width overflow")?;
        if x+width>s.width||y+height>s.height||width==0||height==0 {return Err("Blit region out of scanout bounds".into());}
        if data.len()!=bytes*height as usize {return Err(format!("Blit buffer is {} bytes, expected {}",data.len(),bytes*height as usize));}
        for row in 0..height as usize {
            let dst=(y as usize+row)*s.pitch as usize+(x as usize)*4;
            s.data[dst..dst+bytes].copy_from_slice(&data[row*bytes..(row+1)*bytes]);
        }
        Ok(())
    }
}

/// Result of processing one command-queue request.
#[derive(Clone,Debug,PartialEq,Eq)]
pub struct Completion {
    pub head:u16,
    pub status:u8,
    pub written:u32,
    pub notify:bool,
    pub error:Option<String>,
}

#[derive(Clone,Debug,PartialEq,Eq)]
enum Failure {Unsupported,Io(String)}

fn fail(e:impl Into<String>)->Failure {Failure::Io(e.into())}

/// The 24-byte command header shared by the 2D commands:
/// opcode, arg (color for FILL_2D, resource id otherwise), x, y, width, height.
type CmdHeader=(u32,u32,u32,u32,u32,u32);

/// Read one VirtIO-GPU command header (all fields little-endian) from guest RAM.
fn read_cmd(m:&GuestMemory<'_>,addr:u64)->Result<CmdHeader,String> {
    let mut raw=[0u8;24];
    m.read(addr,&mut raw)?;
    Ok((
        u32::from_le_bytes(raw[0..4].try_into().unwrap()),
        u32::from_le_bytes(raw[4..8].try_into().unwrap()),
        u32::from_le_bytes(raw[8..12].try_into().unwrap()),
        u32::from_le_bytes(raw[12..16].try_into().unwrap()),
        u32::from_le_bytes(raw[16..20].try_into().unwrap()),
        u32::from_le_bytes(raw[20..24].try_into().unwrap()),
    ))
}

/// Execute one already-parsed command chain against the backend.
fn execute(m:&mut GuestMemory<'_>,chain:&Chain,backend:&mut dyn GpuBackend)->Result<u32,Failure> {
    let descriptors=&chain.descriptors;
    if descriptors.len()<2 {return Err("GPU command needs a header and a status".into());}
    let header=descriptors[0];
    if header.writable||header.length!=24 {return Err("GPU command header must be 24 device-readable bytes".into());}
    let (cmd,arg,x,y,width,height)=match read_cmd(m,header.address) {Ok(v)=>v,Err(e)=>return Err(fail(e))};
    let data:&[_]=&descriptors[1..descriptors.len()-1];
    match cmd {
        CMD_FILL2D=>execute_fill(backend,arg,x,y,width,height),
        CMD_RESOURCE_UPDATE2D=>execute_resource_update(m,data,backend,x,y,width,height),
        CMD_TRANSFER_FROM_HOST2D=>execute_transfer_from_host(m,data,backend,x,y,width,height),
        CMD_TRANSFER_TO_HOST2D=>execute_transfer_to_host(m,data,backend,x,y,width,height),
        CMD_COMPOSITE2D=>execute_composite(m,data,backend,x,y,width,height),
        CMD_SCANOUT2D=>execute_scanout(backend,x,y,width,height),
        _=>Err(Failure::Unsupported),
    }
}

fn validate_region(x:u32,y:u32,width:u32,height:u32)->Result<(),Failure> {
    if width==0||height==0||width.checked_mul(height).is_none() {return Err(fail("Zero or overflowing GPU region"));}
    Ok(())
}

/// VIRTIO_GPU_CMD_FILL_2D: fill a rectangle with a 32-bit color (from `arg`).
fn execute_fill(backend:&mut dyn GpuBackend,color:u32,x:u32,y:u32,width:u32,height:u32)->Result<u32,Failure> {
    validate_region(x,y,width,height)?;
    let s=backend.scanout_mut();
    if x.checked_add(width).is_none_or(|v| v>s.width)||y.checked_add(height).is_none_or(|v| v>s.height) {
        return Err(fail("FILL_2D region out of scanout bounds"));
    }
    let bytes=(width as usize).checked_mul(4).ok_or("FILL_2D width overflow")?;
    let pixels=[color as u8,(color>>8) as u8,(color>>16) as u8,(color>>24) as u8];
    for row in 0..height as usize {
        let dst=(y as usize+row)*s.pitch as usize+(x as usize)*4;
        for c in 0..bytes {s.data[dst+c]=pixels[c%4];}
    }
    Ok(1)
}

/// VIRTIO_GPU_CMD_RESOURCE_UPDATE_2D: blit a host buffer into the scanout surface.
fn execute_resource_update(m:&mut GuestMemory<'_>,data:&[_],backend:&mut dyn GpuBackend,x:u32,y:u32,width:u32,height:u32)->Result<u32,Failure> {
    validate_region(x,y,width,height)?;
    if data.len()!=1 {return Err(fail("RESOURCE_UPDATE_2D needs exactly one data descriptor"));}
    if data[0].writable {return Err(fail("RESOURCE_UPDATE_2D data must be device-readable"));}
    let len=data[0].length as usize;
    let bytes=region_bytes(width,height)?;
    if len!=bytes {return Err(fail(format!("RESOURCE_UPDATE_2D data is {len} bytes, expected {bytes}")));}
    let mut buf=vec![0;len];
    m.read(data[0].address,&mut buf)?;
    backend.blit_to_scanout(x,y,width,height,&buf)?;
    Ok(1)
}

/// VIRTIO_GPU_CMD_TRANSFER_FROM_HOST_2D: blit a host buffer into the scanout.
fn execute_transfer_from_host(m:&mut GuestMemory<'_>,data:&[_],backend:&mut dyn GpuBackend,x:u32,y:u32,width:u32,height:u32)->Result<u32,Failure> {
    validate_region(x,y,width,height)?;
    if data.len()!=1 {return Err(fail("TRANSFER_FROM_HOST_2D needs exactly one data descriptor"));}
    if data[0].writable {return Err(fail("TRANSFER_FROM_HOST_2D data must be device-readable"));}
    let len=data[0].length as usize;
    if len>MAX_TRANSFER {return Err(fail("TRANSFER_FROM_HOST_2D exceeds transfer limit"));}
    let bytes=region_bytes(width,height)?;
    if len!=bytes {return Err(fail(format!("TRANSFER_FROM_HOST_2D data is {len} bytes, expected {bytes}")));}
    let mut buf=vec![0;len];
    m.read(data[0].address,&mut buf)?;
    backend.blit_to_scanout(x,y,width,height,&buf)?;
    Ok(1)
}

/// VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D: read back a scanout region into a host buffer.
fn execute_transfer_to_host(m:&mut GuestMemory<'_>,data:&[_],backend:&mut dyn GpuBackend,x:u32,y:u32,width:u32,height:u32)->Result<u32,Failure> {
    validate_region(x,y,width,height)?;
    if data.len()!=1 {return Err(fail("TRANSFER_TO_HOST_2D needs exactly one data descriptor"));}
    if !data[0].writable {return Err(fail("TRANSFER_TO_HOST_2D data must be device-writable"));}
    let len=data[0].length as usize;
    let bytes=region_bytes(width,height)?;
    if len!=bytes {return Err(fail(format!("TRANSFER_TO_HOST_2D data is {len} bytes, expected {bytes}")));}
    let mut buf=Vec::with_capacity(len);
    backend.readback(x,y,width,height,&mut buf)?;
    m.write(data[0].address,&buf)?;
    Ok(1)
}

/// VIRTIO_GPU_CMD_COMPOSITE_2D: draw one resource onto the scanout. With a
/// single scanout surface, source and destination are the same surface.
fn execute_composite(m:&mut GuestMemory<'_>,data:&[_],backend:&mut dyn GpuBackend,x:u32,y:u32,width:u32,height:u32)->Result<u32,Failure> {
    validate_region(x,y,width,height)?;
    if data.len()!=1 {return Err(fail("COMPOSITE_2D needs exactly one data descriptor"));}
    if data[0].writable {return Err(fail("COMPOSITE_2D data must be device-readable"));}
    let len=data[0].length as usize;
    let bytes=region_bytes(width,height)?;
    if len!=bytes {return Err(fail(format!("COMPOSITE_2D data is {len} bytes, expected {bytes}")));}
    let mut buf=vec![0;len];
    m.read(data[0].address,&mut buf)?;
    backend.blit_to_scanout(x,y,width,height,&buf)?;
    Ok(1)
}

/// VIRTIO_GPU_CMD_SCANOUT_2D: mark a scanout region ready for presentation.
fn execute_scanout(backend:&mut dyn GpuBackend,x:u32,y:u32,width:u32,height:u32)->Result<u32,Failure> {
    validate_region(x,y,width,height)?;
    let s=backend.scanout();
    if x.checked_add(width).is_none_or(|v| v>s.width)||y.checked_add(height).is_none_or(|v| v>s.height) {
        return Err(fail("SCANOUT_2D region out of scanout bounds"));
    }
    Ok(1)
}

/// Expected pixel-buffer size for a region at the scanout pitch (4 bpp).
fn region_bytes(width:u32,height:u32)->Result<usize,Failure> {
    (width as usize).checked_mul(height as usize).ok_or_else(||fail("GPU region overflow"))?
        .checked_mul(4).map_err(|_|fail("GPU region overflow"))
}

/// Process at most one command-queue request. Returns `Ok(None)` when the queue
/// is empty. Completion status is 0 (success), 2 (unsupported), 1 (device error).
pub fn process_one(m:&mut GuestMemory<'_>,queue:&mut SplitQueue,backend:&mut dyn GpuBackend)->Result<Option<Completion>,String> {
    let Some(chain)=queue.peek(m)? else {return Ok(None);};
    let status=chain.descriptors.last().ok_or("Missing status descriptor")?;
    if !status.writable||status.length!=1 {return Err("Missing valid one-byte writable status tail".into());}
    let (status_byte,written,error)=match execute(m,&chain,backend) {
        Ok(written)=>(0,written,None),
        Err(Failure::Unsupported)=>(2,1,Some("Unsupported GPU command".into())),
        Err(Failure::Io(e))=>(1,1,Some(e)),
    };
    m.write(status.address,&[status_byte])?;
    queue.complete(m,chain.head,written)?;
    Ok(Some(Completion{head:chain.head,status:status_byte,written,notify:chain.notify,error}))
}
