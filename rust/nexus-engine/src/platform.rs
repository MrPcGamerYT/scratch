//! Early bring-up platform; not a complete PC chipset. Host ports never exposed.
use crate::{ata::AtaPio,io::IoExit,pit::Pit2,pit0::Pit0,pic::PicPair,rtc::Rtc,uart::EarlyUart};
#[derive(Default)]
pub struct EarlyPlatform {pub pci:crate::pci::EmptyPci,pub uart:EarlyUart,pub pit:Pit2,pub pit0:Pit0,pub pic:PicPair,pub rtc:Rtc,pub ata:Option<AtaPio>}
impl EarlyPlatform {
    pub fn tick(&mut self,now_ns:u64) {
        self.pic.set_irq4(self.uart.irq_asserted());
        if self.pit0.poll(now_ns) {self.pic.raise_irq0();}
    }
    pub fn access(&mut self,io:IoExit,now_ns:u64)->Result<Option<u64>,String> {
        self.tick(now_ns);
        if io.port==0xcf8||(0xcfc..=0xcff).contains(&io.port) {return self.pci.access(io);}
        if (0x1f0..=0x1f7).contains(&io.port)||io.port==0x3f6 {
            if let Some(ata)=&mut self.ata {return ata.access(io);}
            if (io.port==0x1f0&&io.size==2)||(io.port!=0x1f0&&io.size==1) {
                return Ok(if io.write {None} else {Some(io.input_register(if io.size==2 {0xffff} else {0xff}))});
            }
            return Err("Unsupported ATA access width".into());
        }
        if io.port==0x40||(io.port==0x43&&io.write&&(io.rax as u8)>>6==0) {
            if io.size!=1 {return Err("PIT channel0 accesses must be bytes".into());}
            return self.pit0.access(io.port,io.write,io.rax as u8,now_ns).map(|value|value.map(|b|io.input_register(u32::from(b))));
        }
        if [0x42,0x43,0x61].contains(&io.port) {
            if io.size!=1 {return Err("PIT/control accesses must be bytes".into());}
            return self.pit.access(io.port,io.write,io.rax as u8,now_ns).map(|value|value.map(|b|io.input_register(u32::from(b))));
        }
        if [0x20,0x21,0xa0,0xa1,0x4d0,0x4d1].contains(&io.port) {
            if io.size!=1 {return Err("PIC accesses must be bytes".into());}
            return self.pic.access(io.port,io.write,io.rax as u8).map(|value|value.map(|b|io.input_register(u32::from(b))));
        }
        if [0x70,0x71].contains(&io.port) {
            if io.size!=1 {return Err("RTC accesses must be bytes".into());}
            return self.rtc.access(io.port,io.write,io.rax as u8,now_ns).map(|value|value.map(|b|io.input_register(u32::from(b))));
        }
        // No ISA DMA device is present. Linux i8237A probes PAGE_0 for 0xff.
        // These are absent-device reads, not a DMA transfer implementation.
        if [0x81,0x82,0x83,0x87,0x89,0x8a,0x8b,0x8f].contains(&io.port)&&!io.write&&io.size==1 {
            return Ok(Some(io.input_register(0xff)));
        }
        // No i8042 controller/PS2 devices are populated. Open-bus status/data
        // makes the Linux probe decline them; future input needs its own device ABI.
        if [0x60,0x64].contains(&io.port)&&io.size==1 {
            return Ok(if io.write {None} else {Some(io.input_register(0xff))});
        }
        // Only COM1 is populated; absent COM2/3/4 ports have open-bus reads.
        if ((0x2f8..=0x2ff).contains(&io.port)||(0x3e8..=0x3ef).contains(&io.port)||(0x2e8..=0x2ef).contains(&io.port))&&io.size==1 {
            return Ok(if io.write {None} else {Some(io.input_register(0xff))});
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
    #[test] fn absent_isa_dma_probes_never_expose_host_hardware() {
        let mut bus=EarlyPlatform::default();
        for port in [0x81,0x82,0x83,0x87,0x89,0x8a,0x8b,0x8f] {
            assert_eq!(bus.access(io(port,1,false,0x1200),0).unwrap(),Some(0x12ff));
            assert!(bus.access(io(port,1,true,0),0).is_err());
            assert!(bus.access(io(port,2,false,0),0).is_err());
        }
    }
    #[test] fn chipset_rejects_wide_io_and_unknown_ports() {
        let mut bus=EarlyPlatform::default();
        for port in [0x20,0x21,0xa0,0xa1,0x70,0x71,0x40,0x42,0x43,0x61] {
            for size in [2,4] {assert!(bus.access(io(port,size,true,0),0).is_err());}
        }
        assert_eq!(bus.pic.accesses,0);assert_eq!(bus.rtc.accesses,0);assert_eq!(bus.pit.accesses,0);
        assert!(bus.access(io(0x123,1,false,0),0).is_err());
    }
}
