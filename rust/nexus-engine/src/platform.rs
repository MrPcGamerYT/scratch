//! Early bring-up platform; not a complete PC chipset. Host ports never exposed.
use crate::{io::IoExit,pit::Pit2,pic::PicPair,rtc::Rtc,uart::EarlyUart};
#[derive(Default)]
pub struct EarlyPlatform {pub uart:EarlyUart,pub pit:Pit2,pub pic:PicPair,pub rtc:Rtc}
impl EarlyPlatform {
    pub fn access(&mut self,io:IoExit,now_ns:u64)->Result<Option<u64>,String> {
        if [0x42,0x43,0x61].contains(&io.port) {
            if io.size!=1 {return Err("PIT/control accesses must be bytes".into());}
            return self.pit.access(io.port,io.write,io.rax as u8,now_ns).map(|value|value.map(|b|io.input_register(u32::from(b))));
        }
        if [0x20,0x21,0xa0,0xa1].contains(&io.port) {
            if io.size!=1 {return Err("PIC accesses must be bytes".into());}
            return self.pic.access(io.port,io.write,io.rax as u8).map(|value|value.map(|b|io.input_register(u32::from(b))));
        }
        if [0x70,0x71].contains(&io.port) {
            if io.size!=1 {return Err("RTC accesses must be bytes".into());}
            return self.rtc.access(io.port,io.write,io.rax as u8,now_ns).map(|value|value.map(|b|io.input_register(u32::from(b))));
        }
        self.uart.access(io)
    }
}
