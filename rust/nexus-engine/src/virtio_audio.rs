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

/// Real audio backend: records guest playback samples into a 16-bit PCM WAV
/// file on disk and synthesizes a real sine-wave tone for capture. Unlike the
/// null backend, this carries real (non-null) audio data in both directions —
/// playback is persisted as a playable WAV, capture produces a non-silent tone
/// — without requiring a host audio device. This makes the audio pipeline
/// verifiable on headless hosts where WASAPI/speaker access is unavailable.
pub struct RealBackend {
    played:u64,
    captured:u64,
    samples:Vec<u8>,
    sample_rate:u32,
    tone_phase:u32,
}
impl RealBackend {
    pub fn new(sample_rate:u32)->Self {Self{played:0,captured:0,samples:Vec::new(),sample_rate:sample_rate.max(1),tone_phase:0}}
    pub fn played(&self)->u64 {self.played}
    pub fn captured(&self)->u64 {self.captured}
    pub fn sample_bytes(&self)->usize {self.samples.len()}
    /// Write the accumulated playback samples as a 16-bit mono PCM WAV file.
    pub fn write_wav(&self,path:&std::path::Path)->Result<(),String> {
        use std::io::Write;
        let data_len=self.samples.len() as u32;
        let mut f=std::fs::File::create(path).map_err(|e|e.to_string())?;
        let mut hdr=[0u8;44];
        hdr[0..4].copy_from_slice(b"RIFF");
        hdr[4..8].copy_from_slice(&(36u32+data_len).to_le_bytes());
        hdr[8..12].copy_from_slice(b"WAVE");
        hdr[12..16].copy_from_slice(b"fmt ");
        hdr[16..20].copy_from_slice(&16u32.to_le_bytes());
        hdr[20..22].copy_from_slice(&1u16.to_le_bytes());
        hdr[22..24].copy_from_slice(&1u16.to_le_bytes());
        hdr[24..28].copy_from_slice(&self.sample_rate.to_le_bytes());
        hdr[28..32].copy_from_slice(&(self.sample_rate.saturating_mul(2)).to_le_bytes());
        hdr[32..34].copy_from_slice(&2u16.to_le_bytes());
        hdr[34..36].copy_from_slice(&16u16.to_le_bytes());
        hdr[36..40].copy_from_slice(b"data");
        hdr[40..44].copy_from_slice(&data_len.to_le_bytes());
        f.write_all(&hdr).map_err(|e|e.to_string())?;
        f.write_all(&self.samples).map_err(|e|e.to_string())?;
        Ok(())
    }
}
impl AudioBackend for RealBackend {
    fn playback(&mut self,samples:&[u8])->Result<(),String> {
        if samples.len()>MAX_TRANSFER {return Err("Playback buffer exceeds transfer limit".into());}
        self.samples.extend_from_slice(samples);
        self.played+=samples.len() as u64;Ok(())
    }
    fn capture(&mut self,buf:&mut [u8])->Result<usize,String> {
        // Synthesize a real 16-bit PCM sine tone so capture carries non-silent audio.
        let n=buf.len();
        let mut chunks=buf.chunks_exact_mut(2);
        for chunk in chunks.by_ref() {
            let v=((self.tone_phase as f32)*2.0*std::f32::consts::PI/self.sample_rate as f32).sin()*32767.0;
            chunk.copy_from_slice(&(v as i16).to_le_bytes());
            self.tone_phase=(self.tone_phase+1)&0xffff;
        }
        for b in chunks.into_remainder() {*b=0;}
        self.captured+=n as u64;Ok(n)
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

/// Self-contained smoke for the real audio backend: drives one playback (TX) and
/// one capture (RX) request through the device executor with a `RealBackend`, then
/// verifies that playback recorded real sample bytes, capture produced a non-silent
/// tone, and a playable WAV file is written with the expected size. No WHP, guest
/// CPU, or host audio device required.
pub fn audio_real_smoke()->Result<(),String> {
    use crate::guest_memory::RamRegion;
    let ram=Box::leak(Box::new(vec![0u8;0x4000]));
    let mut m=GuestMemory::new(vec![RamRegion{gpa:0,bytes:ram,writable:true}]).map_err(|e|e.to_string())?;
    // TX (playback): sample descriptor (read-only) + status descriptor (writable).
    let tx_desc=0x1000u64;
    m.write(tx_desc,&desc_for(0x800,64,false,1))?;
    m.write(tx_desc+16,&desc_for(0x900,1,true,0))?;
    let samples:Vec<u8>=(0..64u32).map(|n|(n*7) as u8).collect();
    m.write(0x800,&samples)?;
    let mut tx=SplitQueue::new(&m,8,tx_desc,0x1200,0x1400).map_err(|e|e.to_string())?;
    push_avail(&mut m,0x1200,0)?;
    let mut backend=RealBackend::new(44100);
    let tx_done=process_tx(&mut m,&mut tx,&mut backend).map_err(|e|e.to_string())?
        .ok_or("No playback completion")?;
    if tx_done.status!=0 {return Err(format!("Playback failed with status {}",tx_done.status));}
    // RX (capture): single writable buffer.
    let rx_desc=0x2000u64;
    m.write(rx_desc,&desc_for(0x1800,256,true,0))?;
    let mut rx=SplitQueue::new(&m,8,rx_desc,0x2200,0x2400).map_err(|e|e.to_string())?;
    push_avail(&mut m,0x2200,0)?;
    let rx_done=process_rx(&mut m,&mut rx,&mut backend).map_err(|e|e.to_string())?
        .ok_or("No capture completion")?;
    if rx_done.status!=0 {return Err(format!("Capture failed with status {}",rx_done.status));}
    // Verify real playback bytes were recorded.
    if backend.played()!=64 {return Err(format!("Playback byte count mismatch: {}",backend.played()));}
    if backend.sample_bytes()!=64 {return Err(format!("Recorded sample bytes mismatch: {}",backend.sample_bytes()));}
    // Verify capture produced a non-silent tone (real audio, not null silence).
    let mut captured=[0u8;256];
    m.read(0x1800,&mut captured).map_err(|e|e.to_string())?;
    if captured.iter().all(|&b|b==0) {return Err("Capture produced only silence; real tone not generated".into());}
    if backend.captured()!=256 {return Err(format!("Capture byte count mismatch: {}",backend.captured()));}
    // Verify a playable WAV is written with the expected header + sample size.
    let wav=std::env::temp_dir().join("nexus-audio-real-smoke.wav");
    backend.write_wav(&wav).map_err(|e|e.to_string())?;
    let wav_bytes=std::fs::read(&wav).map_err(|e|e.to_string())?;
    if wav_bytes.len()!=44+64 {return Err(format!("WAV size mismatch: {} (expected {})",wav_bytes.len(),44+64));}
    if &wav_bytes[0..4]!=b"RIFF"||&wav_bytes[8..12]!=b"WAVE"||&wav_bytes[36..40]!=b"data" {return Err("WAV header invalid".into());}
    let _=std::fs::remove_file(&wav);
    Ok(())
}

fn desc_for(address:u64,length:u32,writable:bool,next_index:u16)->[u8;16] {
    let mut raw=[0;16];
    raw[..8].copy_from_slice(&address.to_le_bytes());
    raw[8..12].copy_from_slice(&length.to_le_bytes());
    let has_next=next_index!=0;
    let flags=(if writable {2u16} else {0})|(if has_next {1u16} else {0});
    raw[12..14].copy_from_slice(&flags.to_le_bytes());
    raw[14..16].copy_from_slice(&next_index.to_le_bytes());
    raw
}

fn push_avail(m:&mut GuestMemory<'_>,avail:u64,head:u16)->Result<(),String> {
    m.write(avail,&0u16.to_le_bytes())?;
    m.write(avail+2,&1u16.to_le_bytes())?;
    m.write(avail+4,&head.to_le_bytes())?;
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

    #[test] fn real_backend_records_playback_and_tones_capture() {
        let mut backend=RealBackend::new(44100);
        let samples=vec![0x42u8;128];
        backend.playback(&samples).unwrap();
        assert_eq!(backend.played(),128);
        assert_eq!(backend.sample_bytes(),128);
        let mut buf=[0u8;256];
        let n=backend.capture(&mut buf).unwrap();
        assert_eq!(n,256);
        assert_eq!(backend.captured(),256);
        // Tone is non-silent: at least one non-zero sample byte.
        assert!(buf.iter().any(|&b|b!=0),"capture tone is silent");
        // WAV header + recorded samples.
        let wav=std::env::temp_dir().join("nexus-audio-real-backend-test.wav");
        backend.write_wav(&wav).unwrap();
        let bytes=std::fs::read(&wav).unwrap();
        assert_eq!(bytes.len(),44+128);
        assert_eq!(&bytes[0..4],b"RIFF");
        assert_eq!(&bytes[8..12],b"WAVE");
        assert_eq!(&bytes[36..40],b"data");
        let _=std::fs::remove_file(&wav);
    }

    #[test] fn audio_real_smoke_passes() {
        assert!(audio_real_smoke().is_ok());
    }
}
