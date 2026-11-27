//! Private COM2 bridge bring-up, distinct from COM1 boot logs; no host sockets/ADB.
//! Fixed QUERY_APP probe or signed-fixture APK smoke sequence; no arbitrary guest shell/path API.
use std::collections::VecDeque;
use std::io::{Read,Seek,SeekFrom};
struct Apk {file:std::fs::File,total:u64,sent:u64,digest:String,phase:u8}
impl Apk {
    fn open(path:&std::path::Path)->Result<Self,String> {
        let mut file=std::fs::File::open(path).map_err(|e|e.to_string())?;
        if !file.metadata().map_err(|e|e.to_string())?.is_file() {return Err("APK must be a regular file".into());}
        let (digest,total)=crate::sha256::digest(&mut file,512*1024*1024)?;
        if total==0 {return Err("Empty APK".into());}file.seek(SeekFrom::Start(0)).map_err(|e|e.to_string())?;
        Ok(Self{file,total,sent:0,digest,phase:0})
    }
}
use crate::{io::IoExit,uart::EarlyUart};
pub const GREETING:&[u8]=b"NEXUS 1 HELLO READY NAV VOLUME SHUTDOWN HOME_CHECK APP APK";
#[derive(Default)]
pub struct Bridge {apk:Option<Apk>,line:Vec<u8>,pending:VecDeque<u8>,greeted:bool,app:Option<String>,result:bool,pub received:usize}
impl Bridge {
    pub fn with_apk(path:&std::path::Path)->Result<Self,String> {Ok(Self{apk:Some(Apk::open(path)?),..Default::default()})}
    pub fn installing(&self)->bool {self.apk.is_some()}
    pub fn complete(&self)->bool {if self.apk.is_some() {self.result} else {self.greeted&&self.app.is_some()&&self.result}}
    pub fn app(&self)->Option<&str> {self.app.as_deref()}
    pub fn next_byte(&mut self)->Option<u8> {self.pending.pop_front()}
    pub fn receive(&mut self,bytes:&[u8])->Result<(),String> {
        for &byte in bytes {
            if self.received>=1024*1024 {return Err("Native bridge receive budget exceeded".into());}self.received+=1;
            if byte==b'\n' {let line=std::mem::take(&mut self.line);self.frame(&line)?;}
            else {
                if !byte.is_ascii_graphic()&&byte!=b' ' {return Err(format!("Invalid native bridge text byte 0x{byte:02x}"));}
                if self.line.len()>=127 {return Err("Native bridge frame too long".into());}self.line.push(byte);
            }
        }Ok(())
    }
    fn frame(&mut self,line:&[u8])->Result<(),String> {
        if !self.greeted {
            if line!=GREETING {return Err("Native bridge version/capability greeting mismatch".into());}
            self.greeted=true;println!("NATIVE_BRIDGE_EVENT HELLO");if self.apk.is_none() {self.pending.extend(b"NEXUS 1 QUERY_APP\n");}return Ok(());
        }
        if let Some(apk)=&mut self.apk {
            println!("NATIVE_BRIDGE_EVENT {}",String::from_utf8_lossy(line));
            if line==b"NEXUS 1 READY 0" {return Ok(());}
            if line==b"NEXUS 1 READY 1" {
                if apk.phase==0 {self.pending.extend(format!("NEXUS 1 APK BEGIN {} {}\n",apk.total,apk.digest).bytes());apk.phase=1;}return Ok(());
            }
            if line==b"NEXUS 1 APK NEXT"&&apk.phase==1&&self.pending.is_empty() {
                if apk.sent==apk.total {self.pending.extend(b"NEXUS 1 APK COMMIT\n");apk.phase=2;}
                else {
                    let size=(apk.total-apk.sent).min(65536) as usize;let mut chunk=vec![0;size];apk.file.read_exact(&mut chunk).map_err(|e|e.to_string())?;
                    self.pending.extend(format!("NEXUS 1 APK DATA {size}\n").bytes());self.pending.extend(chunk);apk.sent+=size as u64;
                }return Ok(());
            }
            if line==b"NEXUS 1 RESULT APK_INSTALL OK"&&apk.phase==2&&self.pending.is_empty() {self.pending.extend(b"NEXUS 1 VERIFY_TEST_APK\n");apk.phase=3;return Ok(());}
            if line==b"NEXUS 1 RESULT VERIFY_TEST_APK OK"&&apk.phase==3&&self.pending.is_empty() {apk.phase=4;self.result=true;return Ok(());}
            return Err("APK bridge installation/verification failed or response out of order".into());
        }
        if line==b"NEXUS 1 READY 0"||line==b"NEXUS 1 READY 1" {return Ok(());}
        if let Some(name)=line.strip_prefix(b"NEXUS 1 APP ") {
            if self.app.is_some()||name.len()>100||name.is_empty() {return Err("Invalid/duplicate app identity".into());}
            let valid=name==b"-"||name.split(|b|*b==b'.').all(|part|!part.is_empty()&&(part[0].is_ascii_alphabetic()||part[0]==b'_')&&part.iter().all(|b|b.is_ascii_alphanumeric()||*b==b'_'));
            if !valid {return Err("Invalid bridge package identity".into());}
            self.app=Some(String::from_utf8(name.to_vec()).map_err(|_|"Invalid app encoding")?);return Ok(());
        }
        if line==b"NEXUS 1 RESULT QUERY_APP OK"&&self.app.is_some()&&!self.result {self.result=true;return Ok(());}
        Err("Unexpected/failed native bridge response".into())
    }
}
#[derive(Default)]
pub struct Device {pub uart:EarlyUart,pub client:Bridge,probe_bytes:usize,pub raw_output:Vec<u8>}
impl Device {
    pub fn with_apk(path:&std::path::Path)->Result<Self,String> {Ok(Self{uart:EarlyUart::default(),client:Bridge::with_apk(path)?,probe_bytes:0,raw_output:Vec::new()})}
    pub fn tick(&mut self)->Result<(),String> {
        if self.uart.can_receive() {if let Some(byte)=self.client.next_byte() {self.uart.receive(byte)?;}}Ok(())
    }
    pub fn access(&mut self,mut io:IoExit)->Result<Option<u64>,String> {
        if !(0x2f8..=0x2ff).contains(&io.port) {return Err("Private COM2 access out of range".into());}
        io.port+=0x100;
        let answer=self.uart.access(io)?;
        if !self.uart.output.is_empty() {
            let bytes=std::mem::take(&mut self.uart.output);
            self.raw_output.extend_from_slice(&bytes);
            for byte in bytes {
                // Linux 8250 autoconfig_irq deliberately transmits 0xff before
                // a userspace port is opened. Bound this hardware probe preface;
                // never discard bytes inside a frame or after the greeting.
                if byte==0xff&&!self.client.greeted&&self.client.line.is_empty()&&self.probe_bytes<8 {
                    self.probe_bytes+=1;println!("NATIVE_BRIDGE_UART_PROBE_FF count={}",self.probe_bytes);continue;
                }
                self.client.receive(&[byte])?;
            }
        }Ok(answer)
    }
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn fragmented_greeting_query_and_result_not_home_or_install_success() {
        let mut b=Bridge::default();for byte in GREETING {b.receive(&[*byte]).unwrap();}assert!(b.pending.is_empty());b.receive(b"\n").unwrap();
        assert_eq!(b.pending.drain(..).collect::<Vec<_>>(),b"NEXUS 1 QUERY_APP\n");
        b.receive(b"NEXUS 1 READY 0\nNEXUS 1 APP org.nexus.launcher\n").unwrap();assert!(!b.complete());
        b.receive(b"NEXUS 1 RESULT QUERY_APP OK\n").unwrap();assert!(b.complete());assert_eq!(b.app(),Some("org.nexus.launcher"));
    }
    #[test] fn rejects_versions_injection_overflow_and_out_of_order_result() {
        for bytes in [b"NEXUS 2 HELLO\n".as_slice(),b"bad\0\n",&[b'x';129]] {assert!(Bridge::default().receive(bytes).is_err());}
        for line in [b"NEXUS 1 APP com.a;id\n".as_slice(),b"NEXUS 1 APP .bad\n",b"NEXUS 1 RESULT QUERY_APP OK\n",b"NEXUS 1 RESULT QUERY_APP FAILED\n"] {
            let mut b=Bridge::default();b.receive(GREETING).unwrap();b.receive(b"\n").unwrap();assert!(b.receive(line).is_err());
        }
    }
    #[test] fn hardware_irq_probe_preface_is_bounded_and_not_allowed_after_greeting() {
        let mut d=Device::default();let io=|b|IoExit{port:0x2f8,size:1,write:true,rax:b,rip:0,instruction_len:1};
        for _ in 0..8 {d.access(io(255)).unwrap();}assert!(d.access(io(255)).is_err());
        let mut d=Device::default();d.access(io(255)).unwrap();for b in GREETING {d.access(io(u64::from(*b))).unwrap();}d.access(io(10)).unwrap();assert!(d.access(io(255)).is_err());
    }
    #[test] fn apk_stream_is_binary_safe_acknowledged_and_not_complete_until_verification() {
        let path=std::env::temp_dir().join(format!("nexus-apk-client-{}",std::process::id()));
        std::fs::write(&path,[0,10,255,17]).unwrap();let mut b=Bridge::with_apk(&path).unwrap();
        b.receive(GREETING).unwrap();b.receive(b"\nNEXUS 1 READY 0\n").unwrap();assert!(b.pending.is_empty());
        b.receive(b"NEXUS 1 READY 1\n").unwrap();let begin=b.pending.drain(..).collect::<Vec<_>>();assert!(begin.starts_with(b"NEXUS 1 APK BEGIN 4 "));
        b.receive(b"NEXUS 1 APK NEXT\n").unwrap();assert_eq!(b.pending.drain(..).collect::<Vec<_>>(),[b"NEXUS 1 APK DATA 4\n".as_slice(),&[0,10,255,17]].concat());
        b.receive(b"NEXUS 1 APK NEXT\n").unwrap();assert_eq!(b.pending.drain(..).collect::<Vec<_>>(),b"NEXUS 1 APK COMMIT\n");assert!(!b.complete());
        b.receive(b"NEXUS 1 RESULT APK_INSTALL OK\n").unwrap();assert!(!b.complete());assert_eq!(b.pending.drain(..).collect::<Vec<_>>(),b"NEXUS 1 VERIFY_TEST_APK\n");
        b.receive(b"NEXUS 1 RESULT VERIFY_TEST_APK OK\n").unwrap();assert!(b.complete());drop(b);std::fs::remove_file(path).unwrap();
    }
    #[test] fn backpressure_does_not_overwrite_receiver() {
        let mut d=Device::default();d.client.pending.extend(b"abc");d.tick().unwrap();d.tick().unwrap();assert_eq!(d.client.pending.len(),2);
    }
}
