//! Dual 8259 initialization/register subset for early PC bring-up.
//! No IRQ source, interrupt acknowledge/injection, auto-EOI, rotation, poll,
//! special-mask or level-triggered operation. Never forwards host port I/O.
//! IRR/ISR remain zero because this subset has no connected interrupt sources.
#[derive(Default)]
struct Controller {
    mask:u8, vector:u8, cascade:u8, step:u8, read_isr:bool,
}
impl Controller {
    fn access(&mut self,data:bool,write:bool,value:u8)->Result<Option<u8>,String> {
        if !write {
            let (irr,isr)=(0,0); // No connected IRQ sources in this subset.
            return Ok(Some(if data {self.mask} else if self.read_isr {isr} else {irr}));
        }
        if !data {
            if value&0x10!=0 {
                // Require cascaded, edge-triggered initialization with ICW4.
                if value!=0x11 {return Err(format!("Unsupported PIC ICW1 0x{value:02x}"));}
                self.step=2;self.mask=0;self.read_isr=false;return Ok(None);
            }
            if self.step!=0 {return Err("PIC command during incomplete initialization".into());}
            match value {
                0x0a=>self.read_isr=false,
                0x0b=>self.read_isr=true,
                // Non-specific/specific EOI: no ISR bits can be set without an IRQ source.
                0x20|0x60..=0x67=>{},
                _=>return Err(format!("Unsupported PIC OCW 0x{value:02x}")),
            }
        } else {
            match self.step {
                2=>{self.vector=value&0xf8;self.step=3;},
                3=>{self.cascade=value;self.step=4;},
                4=>{
                    if value!=1 {return Err(format!("Unsupported PIC ICW4 0x{value:02x}; requires 8086 mode, no auto-EOI"));}
                    self.step=0;
                }
                _=>self.mask=value,
            }
        }
        Ok(None)
    }
}

pub struct PicPair {master:Controller,slave:Controller,pub accesses:u64,pub initializations:u64}
impl Default for PicPair {
    fn default()->Self {
        Self{master:Controller{mask:0xff,..Default::default()},slave:Controller{mask:0xff,..Default::default()},accesses:0,initializations:0}
    }
}
impl PicPair {
    pub fn access(&mut self,port:u16,write:bool,value:u8)->Result<Option<u8>,String> {
        let controller=match port {0x20|0x21=>&mut self.master,0xa0|0xa1=>&mut self.slave,_=>return Err(format!("Unsupported PIC port 0x{port:x}"))};
        self.accesses+=1;
        let result=controller.access(port&1!=0,write,value)?;
        if write&&port&1==0&&value==0x11 {self.initializations+=1;}
        Ok(result)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn masks_roundtrip_and_reset() {
        let mut p=PicPair::default();
        for port in [0x21,0xa1] {
            assert_eq!(p.access(port,false,0).unwrap(),Some(255));
            for value in [0,0x55,0xaa,255] {
                p.access(port,true,value).unwrap();assert_eq!(p.access(port,false,0).unwrap(),Some(value));
            }
        }
    }
    #[test] fn linux_cascaded_initialization_and_register_reads() {
        let mut p=PicPair::default();
        for (port,value) in [(0x20,0x11),(0x21,0x20),(0x21,4),(0x21,1),(0xa0,0x11),(0xa1,0x28),(0xa1,2),(0xa1,1),(0x21,0xfb),(0xa1,0xff)] {
            p.access(port,true,value).unwrap();
        }
        assert_eq!(p.initializations,2);assert_eq!(p.master.vector,0x20);assert_eq!(p.slave.vector,0x28);
        assert_eq!(p.master.cascade,4);assert_eq!(p.slave.cascade,2);
        assert_eq!(p.access(0x21,false,0).unwrap(),Some(0xfb));
        for port in [0x20,0xa0] {
            for command in [0x0a,0x0b,0x20,0x62] {
                p.access(port,true,command).unwrap();assert_eq!(p.access(port,false,0).unwrap(),Some(0));
            }
        }
    }
    #[test] fn unsupported_modes_fail_without_silently_completing_init() {
        let mut p=PicPair::default();
        for value in [0x10,0x13,0x19,0xc0,0x0c,0x68] {assert!(p.access(0x20,true,value).is_err());}
        p.access(0x20,true,0x11).unwrap();p.access(0x21,true,0x20).unwrap();p.access(0x21,true,4).unwrap();
        assert!(p.access(0x21,true,3).is_err());assert_eq!(p.master.step,4);
        p.access(0x21,true,1).unwrap();assert_eq!(p.master.step,0);
        assert!(p.access(0x22,false,0).is_err());
    }
}
