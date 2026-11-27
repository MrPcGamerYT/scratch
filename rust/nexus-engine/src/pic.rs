//! Dual 8259 initialization/register subset for early PC bring-up.
//! IRQ0/IRQ3/IRQ4 edge and dedicated IRQ5 edge/ELCR-level input. No slave IRQ sources,
//! rotation, poll or special-mask mode. Supports 8086 auto-EOI initialization. Host ports never exposed.
//! External CPU injection belongs to the WHP backend.
#[derive(Default)]
struct Controller {
    mask:u8, vector:u8, cascade:u8, step:u8, read_isr:bool,irr:u8,isr:u8,initialized:bool,auto_eoi:bool,
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
                self.step=2;self.mask=0;self.irr=0;self.isr=0;self.initialized=false;self.auto_eoi=false;self.read_isr=false;return Ok(None);
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
                    if value!=1&&value!=3 {return Err(format!("Unsupported PIC ICW4 0x{value:02x}; requires 8086 mode"));}
                    self.auto_eoi=value&2!=0;self.step=0;self.initialized=true;
                }
                _=>self.mask=value,
            }
        }
        Ok(None)
    }
}

pub struct PicPair {master:Controller,slave:Controller,pub accesses:u64,pub initializations:u64,irq5_high:bool,irq4_high:bool,irq3_high:bool,irq6_high:bool,elcr:u8}
impl Default for PicPair {
    fn default()->Self {
        Self{master:Controller{mask:0xff,..Default::default()},slave:Controller{mask:0xff,..Default::default()},accesses:0,initializations:0,irq5_high:false,irq4_high:false,irq3_high:false,irq6_high:false,elcr:0}
    }
}
impl PicPair {
    pub fn set_irq3(&mut self,high:bool) {if high&&!self.irq3_high {self.master.irr|=8;}self.irq3_high=high;}
    pub fn set_irq4(&mut self,high:bool) {if high&&!self.irq4_high {self.master.irr|=0x10;}self.irq4_high=high;}
    pub fn raise_irq0(&mut self) {self.master.irr|=1;}
    /// Device line input. ELCR bit 5 selects level mode; otherwise only rising edges latch.
    pub fn set_irq5(&mut self,high:bool) {
        if self.elcr&0x20!=0 {
            if high {self.master.irr|=0x20;} else {self.master.irr&=!0x20;}
        } else if high&&!self.irq5_high {self.master.irr|=0x20;}
        self.irq5_high=high;
    }
    /// IRQ6 line for the VirtIO-net device. Edge-triggered (no ELCR level mode).
    pub fn set_irq6(&mut self,high:bool) {
        if high&&!self.irq6_high {self.master.irr|=0x40;}
        self.irq6_high=high;
    }
    fn pending_irq(&self)->Option<u8> {
        let m=&self.master;
        if !m.initialized||m.step!=0 {return None;}
        let pending=m.irr&!m.mask&0x79;
        if pending==0 {return None;}
        let irq=pending.trailing_zeros() as u8;
        if m.isr!=0&&u32::from(irq)>=m.isr.trailing_zeros() {return None;}
        Some(irq)
    }
    pub fn pending_vector(&self)->Option<u8> {self.pending_irq().map(|irq|self.master.vector+irq)}
    /// Acknowledge the selected request only after WHP accepted external injection.
    pub fn acknowledge(&mut self)->Result<(),String> {
        let irq=self.pending_irq().ok_or("PIC acknowledged without a deliverable request")?;
        self.master.irr&=!(1<<irq);if !self.master.auto_eoi {self.master.isr|=1<<irq;}
        else if irq==5&&self.elcr&0x20!=0&&self.irq5_high {self.master.irr|=0x20;}
        Ok(())
    }
    pub fn acknowledge_irq0(&mut self)->Result<(),String> {
        if self.pending_irq()!=Some(0) {return Err("PIC IRQ0 acknowledged without a deliverable request".into());}
        self.acknowledge()
    }

    pub fn access(&mut self,port:u16,write:bool,value:u8)->Result<Option<u8>,String> {
        if port==0x4d0||port==0x4d1 {
            self.accesses+=1;
            if !write {return Ok(Some(if port==0x4d0 {self.elcr} else {0}));}
            if (port==0x4d0&&value&!0x20!=0)||(port==0x4d1&&value!=0) {return Err("Only dedicated IRQ5 ELCR mode is supported".into());}
            if port==0x4d0 {self.elcr=value;self.set_irq5(self.irq5_high);}
            return Ok(None);
        }
        let controller=match port {0x20|0x21=>&mut self.master,0xa0|0xa1=>&mut self.slave,_=>return Err(format!("Unsupported PIC port 0x{port:x}"))};
        self.accesses+=1;
        let result=controller.access(port&1!=0,write,value)?;
        if write&&port&1==0&&value==0x11 {self.initializations+=1;}
        // Level requests remain asserted until device ACK, independent of PIC EOI.
        if self.elcr&0x20!=0&&self.irq5_high&&self.master.isr&0x20==0 {self.master.irr|=0x20;}
        Ok(result)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn auto_eoi_does_not_block_subsequent_edges_and_reinitialization_resets_it() {
        let mut p=PicPair::default();
        for (port,value) in [(0x20,0x11),(0x21,0x20),(0x21,4),(0x21,3),(0x21,0)] {p.access(port,true,value).unwrap();}
        p.raise_irq0();p.acknowledge_irq0().unwrap();p.access(0x20,true,0x0b).unwrap();assert_eq!(p.access(0x20,false,0).unwrap(),Some(0));
        p.raise_irq0();assert_eq!(p.pending_vector(),Some(0x20));p.acknowledge_irq0().unwrap();
        for (port,value) in [(0x20,0x11),(0x21,0x20),(0x21,4),(0x21,1),(0x21,0)] {p.access(port,true,value).unwrap();}
        p.raise_irq0();p.acknowledge_irq0().unwrap();p.raise_irq0();assert_eq!(p.pending_vector(),None);
    }
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
    fn initialized()->PicPair {
        let mut p=PicPair::default();
        for (port,value) in [(0x20,0x11),(0x21,0x20),(0x21,4),(0x21,1),(0x21,0)] {p.access(port,true,value).unwrap();}
        p
    }
    #[test] fn private_bridge_irq3_edge_and_priority() {
        let mut p=initialized();p.access(0x21,true,8).unwrap();
        p.set_irq3(true);p.set_irq3(false);assert_eq!(p.pending_vector(),None);
        p.set_irq4(true);p.access(0x21,true,0).unwrap();assert_eq!(p.pending_vector(),Some(0x23));
        p.acknowledge().unwrap();assert_eq!(p.pending_vector(),None);
        p.raise_irq0();assert_eq!(p.pending_vector(),Some(0x20));p.acknowledge_irq0().unwrap();
        p.access(0x20,true,0x20).unwrap();assert_eq!(p.pending_vector(),None);
        p.access(0x20,true,0x20).unwrap();assert_eq!(p.pending_vector(),Some(0x24));
        p.acknowledge().unwrap();p.access(0x20,true,0x20).unwrap();
        p.set_irq3(true);assert_eq!(p.pending_vector(),Some(0x23));p.acknowledge().unwrap();
        p.access(0x20,true,0x20).unwrap();p.set_irq3(true);assert_eq!(p.pending_vector(),None);
    }
    #[test] fn uart_edge_latches_while_masked_and_prioritizes_above_disk() {
        let mut p=initialized();p.access(0x21,true,0x10).unwrap();
        p.set_irq4(true);p.set_irq4(false);assert_eq!(p.pending_vector(),None);
        p.set_irq5(true);p.access(0x21,true,0).unwrap();assert_eq!(p.pending_vector(),Some(0x24));
        p.acknowledge().unwrap();assert_eq!(p.pending_vector(),None);
        p.raise_irq0();assert_eq!(p.pending_vector(),Some(0x20));p.acknowledge_irq0().unwrap();
        p.access(0x20,true,0x20).unwrap();assert_eq!(p.pending_vector(),None);
        p.access(0x20,true,0x20).unwrap();assert_eq!(p.pending_vector(),Some(0x25));
        p.acknowledge().unwrap();p.access(0x20,true,0x20).unwrap();
        p.set_irq4(true);assert_eq!(p.pending_vector(),Some(0x24));p.acknowledge().unwrap();
        p.access(0x20,true,0x20).unwrap();p.set_irq4(true);assert_eq!(p.pending_vector(),None);
        p.set_irq4(false);p.set_irq4(true);assert_eq!(p.pending_vector(),Some(0x24));
    }
    #[test] fn dedicated_level_irq_mask_ack_eoi_reassert_and_deassert() {
        let mut p=initialized();p.access(0x4d0,true,0x20).unwrap();
        p.access(0x21,true,0x20).unwrap();p.set_irq5(true);assert_eq!(p.pending_vector(),None);
        p.access(0x21,true,0).unwrap();assert_eq!(p.pending_vector(),Some(0x25));
        assert!(p.acknowledge_irq0().is_err());p.acknowledge().unwrap();assert_eq!(p.pending_vector(),None);
        p.access(0x20,true,0x60).unwrap();assert_eq!(p.pending_vector(),None);
        p.access(0x20,true,0x65).unwrap();assert_eq!(p.pending_vector(),Some(0x25));
        p.acknowledge().unwrap();p.set_irq5(false);p.access(0x20,true,0x20).unwrap();
        assert_eq!(p.pending_vector(),None);assert!(p.acknowledge().is_err());
        p.set_irq5(true);p.set_irq5(false);assert_eq!(p.pending_vector(),None);
    }
    #[test] fn irq0_preempts_irq5_but_lower_priority_waits_for_eoi() {
        let mut p=initialized();p.access(0x4d0,true,0x20).unwrap();p.set_irq5(true);p.acknowledge().unwrap();
        p.raise_irq0();assert_eq!(p.pending_vector(),Some(0x20));p.acknowledge_irq0().unwrap();
        p.set_irq5(false);p.set_irq5(true);assert_eq!(p.pending_vector(),None);
        p.access(0x20,true,0x20).unwrap();assert_eq!(p.pending_vector(),None);
        p.access(0x20,true,0x20).unwrap();assert_eq!(p.pending_vector(),Some(0x25));
    }
    #[test] fn irq5_edge_mode_and_elcr_restrictions() {
        let mut p=initialized();p.set_irq5(true);p.set_irq5(false);assert_eq!(p.pending_vector(),Some(0x25));
        p.acknowledge().unwrap();p.access(0x20,true,0x20).unwrap();assert_eq!(p.pending_vector(),None);
        assert_eq!(p.access(0x4d0,false,0).unwrap(),Some(0));assert_eq!(p.access(0x4d1,false,0).unwrap(),Some(0));
        for (port,value) in [(0x4d0,1),(0x4d0,0x80),(0x4d1,1)] {assert!(p.access(port,true,value).is_err());}
        p.access(0x4d0,true,0x20).unwrap();assert_eq!(p.access(0x4d0,false,0).unwrap(),Some(0x20));
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
        assert!(p.access(0x21,true,5).is_err());assert_eq!(p.master.step,4);
        p.access(0x21,true,1).unwrap();assert_eq!(p.master.step,0);
        assert!(p.access(0x22,false,0).is_err());
    }
}
