//! Transport-independent VirtIO net request executor over bounded split queues.
//! No MMIO registers or IRQ delivery yet. All guest CPUs must be stopped during calls.
//! The backend is a packet source/sink; a loopback backend echoes TX frames back as
//! RX so the full device model is exercisable without a host network stack.
use crate::{guest_memory::GuestMemory,virtqueue::{SplitQueue,MAX_TRANSFER}};

/// VirtIO-net header length: destination MAC (6) + source MAC (6).
pub const HDR_LEN:usize=12;
/// Negotiated feature bit: the device exposes a MAC address in its config space.
pub const F_MAC:u64=1<<0;
/// Fixed loopback MAC (locally administered, non-routable).
pub const LOOPBACK_MAC:[u8;6]=[0x52,0x54,0x00,0x00,0x00,0x01];

/// Build a 12-byte virtio-net header (destination MAC + source MAC) for loopback.
pub fn net_header()->[u8;HDR_LEN] {
    let mut header=[0u8;HDR_LEN];
    header[..6].copy_from_slice(&LOOPBACK_MAC);
    header[6..].copy_from_slice(&LOOPBACK_MAC);
    header
}

/// A packet backend: transmit frames (without the virtio-net header) and receive
/// frames into a caller-provided buffer. `receive` returns 0 when no frame is ready.
pub trait NetBackend {
    fn transmit(&mut self,frame:&[u8])->Result<(),String>;
    fn receive(&mut self,buf:&mut [u8])->Result<usize,String>;
}

/// Loopback backend: every transmitted frame is queued and handed back on receive.
/// This exercises the RX/TX device model end-to-end without a host network stack.
#[derive(Default)]
pub struct Loopback{tx:std::collections::VecDeque<Vec<u8>>}
impl Loopback {
    pub fn new()->Self {Self::default()}
    pub fn pending(&self)->usize {self.tx.len()}
}
impl NetBackend for Loopback {
    fn transmit(&mut self,frame:&[u8])->Result<(),String> {
        if frame.len()>MAX_TRANSFER {return Err("Loopback frame exceeds transfer limit".into());}
        self.tx.push_back(frame.to_vec());Ok(())
    }
    fn receive(&mut self,buf:&mut [u8])->Result<usize,String> {
        match self.tx.pop_front() {
            Some(frame)=>{let n=frame.len().min(buf.len());buf[..n].copy_from_slice(&frame[..n]);Ok(n)}
            None=>Ok(0),
        }
    }
}

#[derive(Debug)]
pub struct Completion{pub head:u16,pub status:u8,pub written:u32,pub notify:bool,pub error:Option<String>}

/// Process one TX request: a read-only packet descriptor (with the 12-byte header)
/// followed by a one-byte writable status descriptor. The frame (header stripped)
/// is handed to the backend for transmission.
fn process_tx(m:&mut GuestMemory<'_>,queue:&mut SplitQueue,backend:&mut dyn NetBackend)->Result<Option<Completion>,String> {
    let Some(chain)=queue.peek(m)? else {return Ok(None);};
    let descriptors=&chain.descriptors;
    if descriptors.len()<2 {return Err("TX request needs packet and status descriptors".into());}
    let packet=&descriptors[0];
    let status=descriptors.last().ok_or("Missing TX status descriptor")?;
    if packet.writable||!status.writable||status.length!=1 {return Err("TX packet must be read-only with a one-byte writable status".into());}
    if packet.length<HDR_LEN as u32 {return Err("TX packet shorter than the virtio-net header".into());}
    let length=packet.length as usize;
    if length>MAX_TRANSFER {return Err("TX packet exceeds transfer limit".into());}
    let mut buffer=vec![0;length];
    m.read(packet.address,&mut buffer)?;
    let frame=&buffer[HDR_LEN..];
    match backend.transmit(frame) {
        Ok(())=>{m.write(status.address,&[0])?;queue.complete(m,chain.head,packet.length)?;
            Ok(Some(Completion{head:chain.head,status:0,written:packet.length,notify:chain.notify,error:None}))}
        Err(e)=>Ok(Some(Completion{head:chain.head,status:1,written:0,notify:chain.notify,error:Some(e)})),
    }
}

/// Process one RX request: a single writable buffer. The backend's next frame is
/// written with the 12-byte header prepended; the used entry carries the total
/// length. Returns None (without consuming the buffer) when no frame is ready.
fn process_rx(m:&mut GuestMemory<'_>,queue:&mut SplitQueue,backend:&mut dyn NetBackend)->Result<Option<Completion>,String> {
    let Some(chain)=queue.peek(m)? else {return Ok(None);};
    let descriptors=&chain.descriptors;
    if descriptors.len()!=1 {return Err("RX request is a single buffer descriptor".into());}
    let buffer=&descriptors[0];
    if !buffer.writable {return Err("RX buffer descriptor must be writable".into());}
    if buffer.length<HDR_LEN as u32 {return Err("RX buffer too small for the virtio-net header".into());}
    let max_frame=(buffer.length as usize)-HDR_LEN;
    let mut frame=vec![0;max_frame];
    let n=backend.receive(&mut frame)?;
    if n==0 {return Ok(None);}
    let mut packet=vec![0u8;HDR_LEN+n];
    packet[..HDR_LEN].copy_from_slice(&net_header());
    packet[HDR_LEN..].copy_from_slice(&frame[..n]);
    m.write(buffer.address,&packet)?;
    let total=(HDR_LEN+n) as u32;
    queue.complete(m,chain.head,total)?;
    Ok(Some(Completion{head:chain.head,status:0,written:total,notify:chain.notify,error:None}))
}

/// Process one request on the selected queue. `rx` selects the receive queue.
pub fn process_one(m:&mut GuestMemory<'_>,queue:&mut SplitQueue,backend:&mut dyn NetBackend,rx:bool)->Result<Option<Completion>,String> {
    if rx {process_rx(m,queue,backend)} else {process_tx(m,queue,backend)}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guest_memory::RamRegion;
    fn memory(bytes:usize)->GuestMemory<'static> {
        // Leak a buffer so tests can hold GuestMemory across queue setup; bounded and test-only.
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
        m.write(avail,&1u16.to_le_bytes()).unwrap();
        m.write(avail+2,&1u16.to_le_bytes()).unwrap();
        m.write(avail+4,&head.to_le_bytes()).unwrap();
    }

    #[test] fn tx_strips_header_transmits_and_completes_with_status() {
        let mut m=memory(0x2000);
        // descriptor table at 0x1000, avail at 0x1200, used at 0x1400
        let desc_base=0x1000u64;
        m.write(desc_base,&desc(0x800,HDR_LEN as u32+4,false,1)).unwrap();
        m.write(desc_base+16,&desc(0x900,1,true,0)).unwrap();
        // packet at 0x800: 12-byte header + "HELLO"
        let mut packet=vec![0u8;HDR_LEN+5];
        packet[..HDR_LEN].copy_from_slice(&net_header());
        packet[HDR_LEN..].copy_from_slice(b"HELLO");
        m.write(0x800,&packet).unwrap();
        let mut queue=setup_queue(&m,8,desc_base,0x1200,0x1400);
        push_avail(&mut m,0x1200,0);
        let mut backend=Loopback::new();
        let completion=process_tx(&mut m,&mut queue,&mut backend).unwrap().unwrap();
        assert_eq!(completion.status,0);
        assert_eq!(completion.written,(HDR_LEN+5) as u32);
        assert_eq!(backend.pending(),1);
        // status byte written to 0x900
        let mut status_byte=[0u8;1];
        m.read(0x900,&mut status_byte).unwrap();
        assert_eq!(status_byte[0],0);
    }

    #[test] fn rx_prepends_header_and_completes_with_total_length() {
        let mut m=memory(0x2000);
        let mut backend=Loopback::new();
        backend.transmit(b"WORLD").unwrap();
        let desc_base=0x1000u64;
        // single writable RX buffer at 0x800, length 64
        m.write(desc_base,&desc(0x800,64,true,0)).unwrap();
        let mut queue=setup_queue(&m,8,desc_base,0x1200,0x1400);
        push_avail(&mut m,0x1200,0);
        let completion=process_rx(&mut m,&mut queue,&mut backend).unwrap().unwrap();
        assert_eq!(completion.written,(HDR_LEN+5) as u32);
        // packet written into 0x800: header + WORLD
        let mut out=[0u8;HDR_LEN+5];
        m.read(0x800,&mut out).unwrap();
        assert_eq!(&out[..HDR_LEN],&net_header());
        assert_eq!(&out[HDR_LEN..],b"WORLD");
    }

    #[test] fn rx_returns_none_without_consuming_buffer_when_no_frame() {
        let mut m=memory(0x2000);
        let mut backend=Loopback::new();
        let desc_base=0x1000u64;
        m.write(desc_base,&desc(0x800,64,true,0)).unwrap();
        let mut queue=setup_queue(&m,8,desc_base,0x1200,0x1400);
        push_avail(&mut m,0x1200,0);
        assert!(process_rx(&mut m,&mut queue,&mut backend).unwrap().is_none());
        // buffer still available: peek should still return a chain
        assert!(queue.peek(&m).unwrap().is_some());
    }

    #[test] fn tx_rejects_short_packet_and_wrong_direction() {
        let mut m=memory(0x2000);
        let desc_base=0x1000u64;
        // packet shorter than header
        m.write(desc_base,&desc(0x800,4,false,1)).unwrap();
        m.write(desc_base+16,&desc(0x900,1,true,0)).unwrap();
        let mut queue=setup_queue(&m,8,desc_base,0x1200,0x1400);
        push_avail(&mut m,0x1200,0);
        let mut backend=Loopback::new();
        assert!(process_tx(&mut m,&mut queue,&mut backend).is_err());
        // packet writable (wrong direction)
        m.write(desc_base,&desc(0x800,HDR_LEN as u32+4,true,1)).unwrap();
        let mut queue=setup_queue(&m,8,desc_base,0x1200,0x1400);
        push_avail(&mut m,0x1200,0);
        assert!(process_tx(&mut m,&mut queue,&mut backend).is_err());
    }

    #[test] fn loopback_round_trip_tx_then_rx() {
        let mut m=memory(0x4000);
        let mut backend=Loopback::new();
        let desc_base=0x1000u64;
        // TX queue: descriptor table at 0x1000, avail 0x1200, used 0x1400
        m.write(desc_base,&desc(0x800,HDR_LEN as u32+3,false,1)).unwrap();
        m.write(desc_base+16,&desc(0x900,1,true,0)).unwrap();
        let mut packet=vec![0u8;HDR_LEN+3];
        packet[..HDR_LEN].copy_from_slice(&net_header());
        packet[HDR_LEN..].copy_from_slice(b"ABC");
        m.write(0x800,&packet).unwrap();
        let mut tx=setup_queue(&m,8,desc_base,0x1200,0x1400);
        push_avail(&mut m,0x1200,0);
        let tx_done=process_tx(&mut m,&mut tx,&mut backend).unwrap().unwrap();
        assert_eq!(tx_done.status,0);
        // RX queue: separate descriptor table at 0x2000, avail 0x2200, used 0x2400
        let rx_desc=0x2000u64;
        m.write(rx_desc,&desc(0x1800,64,true,0)).unwrap();
        let mut rx=setup_queue(&m,8,rx_desc,0x2200,0x2400);
        push_avail(&mut m,0x2200,0);
        let rx_done=process_rx(&mut m,&mut rx,&mut backend).unwrap().unwrap();
        assert_eq!(rx_done.written,(HDR_LEN+3) as u32);
        let mut out=[0u8;HDR_LEN+3];
        m.read(0x1800,&mut out).unwrap();
        assert_eq!(&out[HDR_LEN..],b"ABC");
    }
}
