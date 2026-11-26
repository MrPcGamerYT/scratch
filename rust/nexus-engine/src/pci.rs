//! Empty PCI configuration mechanism #1. No host PCI access or assigned devices.
//! Early x86 decompression code probes CF8/CFC even with pci=off on the command line.
use crate::io::IoExit;
#[derive(Default)]
pub struct EmptyPci {address:u32}
impl EmptyPci {
    pub fn access(&mut self,io:IoExit)->Result<Option<u64>,String> {
        if io.port==0xcf8 {
            if io.size!=4 {return Err("PCI config address requires 32-bit access".into());}
            if io.write {self.address=io.rax as u32;return Ok(None);}
            return Ok(Some(io.input_register(self.address)));
        }
        if !(0xcfc..=0xcff).contains(&io.port)||![1,2,4].contains(&io.size)
            ||io.port%u16::from(io.size)!=0||u32::from(io.port)+u32::from(io.size)>0xd00 {
            return Err("Unsupported PCI config-data access".into());
        }
        // Every function is absent, whether configuration enable is set or not.
        // Writes cannot create a device or reach physical hardware.
        Ok(if io.write {None} else {Some(io.input_register(u32::MAX))})
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn io(port:u16,size:u8,write:bool,rax:u64)->IoExit {IoExit{port,size,write,rax,rip:0,instruction_len:1}}
    #[test] fn address_roundtrip_and_empty_configuration() {
        let mut pci=EmptyPci::default();
        for address in [0,0x80000000,0x80ffffff,u32::MAX] {
            assert_eq!(pci.access(io(0xcf8,4,true,u64::from(address))).unwrap(),None);
            assert_eq!(pci.access(io(0xcf8,4,false,u64::MAX)).unwrap(),Some(u64::from(address)));
            for (port,size) in [(0xcfc,4),(0xcfc,2),(0xcfe,2),(0xcfc,1),(0xcff,1)] {
                let read=io(port,size,false,0x1234567890abcd00);
                assert_eq!(pci.access(read).unwrap(),Some(read.input_register(u32::MAX)));
                assert_eq!(pci.access(io(port,size,true,0)).unwrap(),None);
                assert_eq!(pci.access(read).unwrap(),Some(read.input_register(u32::MAX)));
            }
        }
    }
    #[test] fn rejects_reset_ports_crossing_and_misalignment_without_mutation() {
        let mut pci=EmptyPci::default();pci.access(io(0xcf8,4,true,0x80000000)).unwrap();
        for (port,size) in [(0xcf8,1),(0xcf8,2),(0xcf9,1),(0xcfb,1),(0xcfd,2),(0xcff,2),(0xcfe,4),(0xd00,4),(0xcfc,8),(0xcfc,0)] {
            for write in [false,true] {assert!(pci.access(io(port,size,write,0)).is_err());}
        }
        assert_eq!(pci.access(io(0xcf8,4,false,0)).unwrap(),Some(0x80000000));
    }
}
