//! Dual 8259 initialization/register subset for early PC bring-up.
//! IRQ0 edge/acknowledge supported; no slave IRQ sources, auto-EOI, rotation, poll,
//! special-mask or level-triggered operation. Never forwards host port I/O.
//! External CPU injection belongs to the WHP backend. Other IRQ lines are not connected.
#[derive(Default)]
struct Controller {
    mask:u8, vector:u8, cascade:u8, step:u8, read_isr:bool,irr:u8,isr:u8,initialized:bool,
}
impl Controller {
    fn access(&mut self,data:bool,write:bool,value:u8)->Result<Option<u8>,String> {
        if !write {
            let (irr,isr)=(self.irr,self.isr);
            return Ok(Some(if data {self.mask} else if self.read_isr {isr} else {irr}));
        }
        if !data {
            if value&0x10!=0 {
                // Require cascaded, edge-triggered initialization with ICW4.
                if value!=0x11 {return Err(format!("Unsupported PIC ICW1 0x{value:02x}"));}
                self.step=2;self.mask=0;self.irr=0;self.isr=0;self.initialized=false;self.read_isr=false;return Ok(None);
            }
            if self.step!=0 {return Err("PIC command during incomplete initialization".into());}
            match value {
                0x0a=>self.read_isr=false,
                0x0b=>self.read_isr=true,
                // Fixed-priority non-specific EOI clears the highest-priority in-service bit.
                0x20=>{if self.isr!=0 {self.isr&=!(1u8<<self.isr.trailing_zeros());}},
                0x60..=0x67=>self.isr&=!(1u8<<(value&7)),
                _=>return Err(format!("Unsupported PIC OCW 0x{value:02x}")),
            }
        } else {
            match self.step {
                2=>{self.vector=value&0xf8;self.step=3;},
                3=>{self.cascade=value;self.step=4;},
                4=>{
                    if value!=1 {return Err(format!("Unsupported PIC ICW4 0x{value:02x}; requires 8086 mode, no auto-EOI"));}
                    self.step=0;self.initialized=true;
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
    pub fn raise_irq0(&mut self) {self.master.irr|=1;}
    pub fn pending_vector(&self)->Option<u8> {
        let m=&self.master;
        if m.initialized&&m.step==0&&m.irr&!m.mask&1!=0&&m.isr==0 {Some(m.vector)} else {None}
    }
    pub fn acknowledge_irq0(&mut self)->Result<(),String> {
        if self.pending_vector().is_none() {return Err("PIC IRQ0 acknowledged without a deliverable request".into());}
        self.master.irr&=!1;self.master.isr|=1;Ok(())
    }

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
    #[test] fn masked_pending_in_service_eoi_and_coalescing() {
        let mut p=PicPair::default();
        for (port,value) in [(0x20,0x11),(0x21,0x20),(0x21,4),(0x21,1),(0x21,255)] {p.access(port,true,value).unwrap();}
        p.raise_irq0();p.raise_irq0();assert_eq!(p.pending_vector(),None);
        p.access(0x20,true,0x0a).unwrap();assert_eq!(p.access(0x20,false,0).unwrap(),Some(1));
        p.access(0x21,true,0xfe).unwrap();assert_eq!(p.pending_vector(),Some(0x20));
        p.acknowledge_irq0().unwrap();assert_eq!(p.pending_vector(),None);
        p.access(0x20,true,0x0b).unwrap();assert_eq!(p.access(0x20,false,0).unwrap(),Some(1));
        p.raise_irq0();assert_eq!(p.pending_vector(),None);
        p.access(0x20,true,0x61).unwrap();assert_eq!(p.pending_vector(),None); // Wrong specific EOI.
        p.access(0x20,true,0x20).unwrap();assert_eq!(p.pending_vector(),Some(0x20));
        p.acknowledge_irq0().unwrap();assert!(p.acknowledge_irq0().is_err());
        p.access(0x20,true,0x60).unwrap();assert_eq!(p.pending_vector(),None);
    }
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
