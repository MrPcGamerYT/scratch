//! Private COM2 bridge bring-up, distinct from COM1 boot logs; no host sockets/ADB.
//! Only QUERY_APP is sent by this probe. No arbitrary shell/file/installation API.
use std::collections::VecDeque;
use crate::{io::IoExit,uart::EarlyUart};
pub const GREETING:&[u8]=b"NEXUS 1 HELLO READY NAV VOLUME SHUTDOWN HOME_CHECK APP APK";
#[derive(Default)]
pub struct Bridge {line:Vec<u8>,pending:VecDeque<u8>,greeted:bool,app:Option<String>,result:bool,pub received:usize}
impl Bridge {
    pub fn complete(&self)->bool {self.greeted&&self.app.is_some()&&self.result}
    pub fn app(&self)->Option<&str> {self.app.as_deref()}
    pub fn next_byte(&mut self)->Option<u8> {self.pending.pop_front()}
    pub fn receive(&mut self,bytes:&[u8])->Result<(),String> {
        for &byte in bytes {
            if self.received>=1024*1024 {return Err("Native bridge receive budget exceeded".into());}self.received+=1;
            if byte==b'\n' {let line=std::mem::take(&mut self.line);self.frame(&line)?;}
            else {
                if !byte.is_ascii_graphic()&&byte!=b' ' {return Err("Invalid native bridge text byte".into());}
                if self.line.len()>=127 {return Err("Native bridge frame too long".into());}self.line.push(byte);
            }
        }Ok(())
    }
    fn frame(&mut self,line:&[u8])->Result<(),String> {
        if !self.greeted {
            if line!=GREETING {return Err("Native bridge version/capability greeting mismatch".into());}
            self.greeted=true;self.pending.extend(b"NEXUS 1 QUERY_APP\n");return Ok(());
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
pub struct Device {pub uart:EarlyUart,pub client:Bridge}
impl Device {
    pub fn tick(&mut self)->Result<(),String> {
        if self.uart.can_receive() {if let Some(byte)=self.client.next_byte() {self.uart.receive(byte)?;}}Ok(())
    }
    pub fn access(&mut self,mut io:IoExit)->Result<Option<u64>,String> {
        if !(0x2f8..=0x2ff).contains(&io.port) {return Err("Private COM2 access out of range".into());}
        io.port+=0x100;
        let answer=self.uart.access(io)?;
        if !self.uart.output.is_empty() {
            let bytes=std::mem::take(&mut self.uart.output);self.client.receive(&bytes)?;
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
    #[test] fn backpressure_does_not_overwrite_receiver() {
        let mut d=Device::default();d.client.pending.extend(b"abc");d.tick().unwrap();d.tick().unwrap();assert_eq!(d.client.pending.len(),2);
    }
}
