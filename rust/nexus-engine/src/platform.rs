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

#[cfg(test)]
mod tests {
    use super::*;
    fn io(port:u16,size:u8,write:bool,rax:u64)->IoExit {
        IoExit{port,size,write,rax,rip:0x100000,instruction_len:1}
    }
    #[test] fn chipset_reads_preserve_upper_rax_and_do_not_touch_uart() {
        let mut bus=EarlyPlatform::default();
        assert_eq!(bus.access(io(0xa1,1,false,0x123400),0).unwrap(),Some(0x1234ff));
        bus.access(io(0x70,1,true,0x0d),0).unwrap();
        assert_eq!(bus.access(io(0x71,1,false,0xab00),0).unwrap(),Some(0xab80));
        assert!(bus.uart.output.is_empty());assert_eq!(bus.pic.accesses,1);
    }
    #[test] fn chipset_rejects_wide_io_and_unknown_ports() {
        let mut bus=EarlyPlatform::default();
        for port in [0x20,0x21,0xa0,0xa1,0x70,0x71,0x42,0x43,0x61] {
            for size in [2,4] {assert!(bus.access(io(port,size,true,0),0).is_err());}
        }
        assert_eq!(bus.pic.accesses,0);assert_eq!(bus.rtc.accesses,0);assert_eq!(bus.pit.accesses,0);
        assert!(bus.access(io(0x123,1,false,0),0).is_err());
    }
}
