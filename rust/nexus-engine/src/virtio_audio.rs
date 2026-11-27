//! Transport-independent VirtIO sound (PCM) request executor over bounded split
//! queues. No MMIO registers or IRQ delivery yet. All guest CPUs must be stopped
//! during calls. The backend is a PCM sample sink/source; a null backend discards
//! playback and returns silence for capture so the full device model is
//! exercisable without a host audio stack.
use crate::{guest_memory::GuestMemory,virtqueue::{SplitQueue,MAX_TRANSFER}};

/// Negotiated feature bit: the device is a PCM stream device.
pub const F_PCM:u64=1<<0;
/// Negotiated feature bit: the device supports control (mixer) operations.
pub const F_CTRL:u64=1<<1;

/// A PCM backend: playback consumes guest samples (guest→host), capture produces
/// samples into a caller-provided buffer (host→guest). `capture` returns 0 when
/// no sample block is ready.
pub trait AudioBackend {
    fn playback(&mut self,samples:&[u8])->Result<(),String>;
    fn capture(&mut self,buf:&mut [u8])->Result<usize,String>;
}

/// Null backend: discards playback samples and returns silence for capture.
/// Exercises the TX/RX device model end-to-end without a host audio stack.
#[derive(Default)]
pub struct NullBackend{played:u64,captured:u64}
impl NullBackend {
    pub fn new()->Self {Self::default()}
    pub fn played(&self)->u64 {self.played}
    pub fn captured(&self)->u64 {self.captured}
}
impl AudioBackend for NullBackend {
    fn playback(&mut self,samples:&[u8])->Result<(),String> {
        if samples.len()>MAX_TRANSFER {return Err("Playback buffer exceeds transfer limit".into());}
        self.played+=samples.len() as u64;Ok(())
    }
    fn capture(&mut self,buf:&mut [u8])->Result<usize,String> {
        // Return a fixed block of silence so capture completes deterministically.
        let n=buf.len().min(512);buf[..n].fill(0);self.captured+=n as u64;Ok(n)
    }
}

#[derive(Debug)]
pub struct Completion{pub head:u16,pub status:u8,pub written:u32,pub notify:bool,pub error:Option<String>}

/// Process one TX (playback) request: a read-only sample descriptor followed by
/// a one-byte writable status descriptor. The samples are handed to the backend.
fn process_tx(m:&mut GuestMemory<'_>,queue:&mut SplitQueue,backend:&mut dyn AudioBackend)->Result<Option<Completion>,String> {
    let Some(chain)=queue.peek(m)? else {return Ok(None);};
    let descriptors=&chain.descriptors;
    if descriptors.len()<2 {return Err("Playback request needs sample and status descriptors".into());}
    let samples=&descriptors[0];
    let status=descriptors.last().ok_or("Missing playback status descriptor")?;
    if samples.writable||!status.writable||status.length!=1 {return Err("Playback samples must be read-only with a one-byte writable status".into());}
    if samples.length==0||samples.length as usize>MAX_TRANSFER {return Err("Playback buffer size invalid".into());}
    let length=samples.length as usize;
    let mut buffer=vec![0;length];
    m.read(samples.address,&mut buffer)?;
    match backend.playback(&buffer) {
        Ok(())=>{m.write(status.address,&[0])?;queue.complete(m,chain.head,samples.length)?;
            Ok(Some(Completion{head:chain.head,status:0,written:samples.length,notify:chain.notify,error:None}))}
        Err(e)=>Ok(Some(Completion{head:chain.head,status:1,written:0,notify:chain.notify,error:Some(e)})),
    }
}

/// Process one RX (capture) request: a single writable buffer. The backend's next
/// sample block is written into the buffer; the used entry carries the length.
/// Returns None (without consuming the buffer) when no block is ready.
fn process_rx(m:&mut GuestMemory<'_>,queue:&mut SplitQueue,backend:&mut dyn AudioBackend)->Result<Option<Completion>,String> {
    let Some(chain)=queue.peek(m)? else {return Ok(None);};
    let descriptors=&chain.descriptors;
    if descriptors.len()!=1 {return Err("Capture request is a single buffer descriptor".into());}
    let buffer=&descriptors[0];
    if !buffer.writable {return Err("Capture buffer descriptor must be writable".into());}
    if buffer.length==0||buffer.length as usize>MAX_TRANSFER {return Err("Capture buffer size invalid".into());}
    let mut frame=vec![0;buffer.length as usize];
    let n=backend.capture(&mut frame)?;
    if n==0 {return Ok(None);}
    m.write(buffer.address,&frame[..n])?;
    queue.complete(m,chain.head,n as u32)?;
    Ok(Some(Completion{head:chain.head,status:0,written:n as u32,notify:chain.notify,error:None}))
}

/// Process one request on the selected queue. `rx` selects the capture queue.
pub fn process_one(m:&mut GuestMemory<'_>,queue:&mut SplitQueue,backend:&mut dyn AudioBackend,rx:bool)->Result<Option<Completion>,String> {
    if rx {process_rx(m,queue,backend)} else {process_tx(m,queue,backend)}
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
    fn setup_queue(m:&GuestMemory<'_>,size:u16,desc:u64,avail:u64,used:u64)->SplitQueue {
        SplitQueue::new(m,size,desc,avail,used).unwrap()
    }
    fn push_avail(m:&mut GuestMemory<'_>,avail:u64,head:u16) {
        m.write(avail,&0u16.to_le_bytes()).unwrap();
        m.write(avail+2,&1u16.to_le_bytes()).unwrap();
        m.write(avail+4,&head.to_le_bytes()).unwrap();
    }

    #[test] fn tx_plays_samples_and_completes_with_status() {
        let mut m=memory(0x2000);
        let desc_base=0x1000u64;
        m.write(desc_base,&desc(0x800,64,false,1)).unwrap();
        m.write(desc_base+16,&desc(0x900,1,true,0)).unwrap();
        let mut samples=vec![0u8;64];
        for (n,b) in samples.iter_mut().enumerate() {*b=(n as u8).wrapping_mul(7);}
        m.write(0x800,&samples).unwrap();
        let mut queue=setup_queue(&m,8,desc_base,0x1200,0x1400);
        push_avail(&mut m,0x1200,0);
        let mut backend=NullBackend::new();
        let completion=process_tx(&mut m,&mut queue,&mut backend).unwrap().unwrap();
        assert_eq!(completion.status,0);
        assert_eq!(completion.written,64);
        assert_eq!(backend.played(),64);
        let mut status_byte=[0u8;1];
        m.read(0x900,&mut status_byte).unwrap();
        assert_eq!(status_byte[0],0);
    }

    #[test] fn rx_captures_silence_and_completes() {
        let mut m=memory(0x2000);
        let desc_base=0x1000u64;
        m.write(desc_base,&desc(0x800,256,true,0)).unwrap();
        let mut queue=setup_queue(&m,8,desc_base,0x1200,0x1400);
        push_avail(&mut m,0x1200,0);
        let mut backend=NullBackend::new();
        let completion=process_rx(&mut m,&mut queue,&mut backend).unwrap().unwrap();
        assert_eq!(completion.status,0);
        assert_eq!(completion.written,256);
        let mut out=[0u8;256];
        m.read(0x800,&mut out).unwrap();
        assert!(out.iter().all(|&b|b==0));
        assert_eq!(backend.captured(),256);
    }

    #[test] fn tx_rejects_wrong_direction_and_empty() {
        let mut m=memory(0x2000);
        let desc_base=0x1000u64;
        // samples writable (wrong direction)
        m.write(desc_base,&desc(0x800,64,true,1)).unwrap();
        m.write(desc_base+16,&desc(0x900,1,true,0)).unwrap();
        let mut queue=setup_queue(&m,8,desc_base,0x1200,0x1400);
        push_avail(&mut m,0x1200,0);
        let mut backend=NullBackend::new();
        assert!(process_tx(&mut m,&mut queue,&mut backend).is_err());
        // empty samples
        m.write(desc_base,&desc(0x800,0,false,1)).unwrap();
        let mut queue=setup_queue(&m,8,desc_base,0x1200,0x1400);
        push_avail(&mut m,0x1200,0);
        assert!(process_tx(&mut m,&mut queue,&mut backend).is_err());
    }

    #[test] fn rx_rejects_readonly_buffer() {
        let mut m=memory(0x2000);
        let desc_base=0x1000u64;
        m.write(desc_base,&desc(0x800,256,false,0)).unwrap();
        let mut queue=setup_queue(&m,8,desc_base,0x1200,0x1400);
        push_avail(&mut m,0x1200,0);
        let mut backend=NullBackend::new();
        assert!(process_rx(&mut m,&mut queue,&mut backend).is_err());
    }

    #[test] fn round_trip_tx_then_rx() {
        let mut m=memory(0x4000);
        let mut backend=NullBackend::new();
        // TX queue
        let desc_base=0x1000u64;
        m.write(desc_base,&desc(0x800,32,false,1)).unwrap();
        m.write(desc_base+16,&desc(0x900,1,true,0)).unwrap();
        let samples=[0x11u8;32];
        m.write(0x800,&samples).unwrap();
        let mut tx=setup_queue(&m,8,desc_base,0x1200,0x1400);
        push_avail(&mut m,0x1200,0);
        let tx_done=process_tx(&mut m,&mut tx,&mut backend).unwrap().unwrap();
        assert_eq!(tx_done.status,0);
        // RX queue
        let rx_desc=0x2000u64;
        m.write(rx_desc,&desc(0x1800,128,true,0)).unwrap();
        let mut rx=setup_queue(&m,8,rx_desc,0x2200,0x2400);
        push_avail(&mut m,0x2200,0);
        let rx_done=process_rx(&mut m,&mut rx,&mut backend).unwrap().unwrap();
        assert_eq!(rx_done.written,128);
    }
}
