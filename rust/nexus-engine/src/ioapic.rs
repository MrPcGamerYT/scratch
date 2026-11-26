//! 24-input IOAPIC: fixed delivery, physical/flat logical destinations.
//! Remote IRR prevents level redelivery until the destination EOIs.
use crate::lapic::{LocalApic,send_fixed,send_fixed_logical};
pub struct IoApic {select:u8,id:u8,entries:[u64;24],lines:[bool;24],edges:[bool;24],owners:[u8;24],vectors:[u8;24]}
impl Default for IoApic {fn default()->Self {Self::new()}}
impl IoApic {
    pub fn new()->Self {Self{select:0,id:0,entries:[1<<16;24],lines:[false;24],edges:[false;24],owners:[0;24],vectors:[0;24]}}
    pub fn read(&self,offset:u32)->Result<u32,String> {
        match offset {0=>Ok(u32::from(self.select)),0x10=>match self.select {
            0=>Ok(u32::from(self.id)<<24),1=>Ok(0x00170011),2=>Ok(u32::from(self.id)<<24),
            0x10..=0x3f=>{let n=usize::from((self.select-0x10)/2);let shift=(self.select%2)*32;
                let value=self.entries[n]|if self.owners[n]!=0 {1<<14} else {0};Ok((value>>shift) as u32)},
            _=>Err("Unsupported IOAPIC register".into())},_=>Err("Invalid IOAPIC MMIO offset".into())}
    }
    pub fn write(&mut self,offset:u32,value:u32)->Result<(),String> {
        match offset {
            0 if value<=255=>{self.select=value as u8;Ok(())},
            0x10=>match self.select {
                0=>{self.id=(value>>24) as u8&15;Ok(())},
                0x10..=0x3f=>{let n=usize::from((self.select-0x10)/2);
                    if self.select%2==1 {self.entries[n]=(self.entries[n]&0xffffffff)|(u64::from(value&0xff000000)<<32);}
                    else {
                        // Do not silently accept lowest-priority/NMI/ExtINT delivery.
                        if value&0x700!=0||value&!0x1efff!=0 {return Err("Unsupported IOAPIC delivery mode/reserved bits".into());}
                        if value&255<16&&value&(1<<16)==0 {return Err("Invalid IOAPIC fixed vector".into());}
                        self.entries[n]=(self.entries[n]&!0xffffffff)|u64::from(value&0x1a8ff);
                    }Ok(())},
                _=>Err("Read-only or unsupported IOAPIC register".into())},
            _=>Err("Invalid IOAPIC MMIO write".into())}
    }
    /// Input is an electrical level; polarity bit determines active state.
    pub fn set_line(&mut self,pin:usize,high:bool)->Result<(),String> {
        if pin>=24 {return Err("Invalid IOAPIC input".into());}
        let low=self.entries[pin]&(1<<13)!=0;
        let was=self.lines[pin]^low;let active=high^low;
        if active&&!was {self.edges[pin]=true;}
        self.lines[pin]=high;Ok(())
    }
    pub fn service(&mut self,cpus:&mut [LocalApic])->Result<(),String> {
        if cpus.len()>8 {return Err("IOAPIC supports at most eight destinations".into());}
        for pin in 0..24 {
            let e=self.entries[pin];let level=e&(1<<15)!=0;
            let active=self.lines[pin]^(e&(1<<13)!=0);
            if e&(1<<16)!=0||self.owners[pin]!=0||if level {!active} else {!self.edges[pin]} {continue;}
            let destination=(e>>56) as u8;let logical=e&(1<<11)!=0;
            let mut owners=0;
            for (index,cpu) in cpus.iter().enumerate() {
                let selected=if logical {(cpu.read(0xd0)?>>24) as u8&destination!=0} else {(cpu.read(0x20)?>>24) as u8==destination};
                if selected {owners|=1<<index;}
            }
            if owners==0 {continue;}
            if logical {send_fixed_logical(cpus,0,destination,0,e as u8)?;} else {send_fixed(cpus,0,destination,0,e as u8)?;}
            self.edges[pin]=false;if level {self.owners[pin]=owners;self.vectors[pin]=e as u8;}
        }Ok(())
    }
    /// Broadcast EOI from a LAPIC clears only routes owned by that CPU/vector.
    pub fn eoi(&mut self,cpu:usize,vector:u8) {
        if cpu>=8 {return;}
        for pin in 0..24 {if self.vectors[pin]==vector {self.owners[pin]&=!(1<<cpu);}}
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn reg(io:&mut IoApic,r:u8,v:u32) {io.write(0,u32::from(r)).unwrap();io.write(0x10,v).unwrap();}
    fn cpus()->Vec<LocalApic> {(0..2).map(|i|{let mut c=LocalApic::new(i);c.write(0xf0,0x1ff).unwrap();c}).collect()}
    #[test] fn level_remote_irr_eoi_reassert_and_mask() {
        let mut io=IoApic::new();let mut c=cpus();reg(&mut io,0x11,1<<24);reg(&mut io,0x10,0x8041);
        io.set_line(0,true).unwrap();io.service(&mut c).unwrap();assert_eq!(c[0].pending(),None);assert_eq!(c[1].pending(),Some(0x41));
        assert_ne!(io.read(0x10).unwrap()&(1<<14),0);
        c[1].acknowledge(0x41).unwrap();io.service(&mut c).unwrap();c[1].eoi();
        io.eoi(0,0x41);io.service(&mut c).unwrap();assert_eq!(c[1].pending(),None);
        io.eoi(1,0x41);io.service(&mut c).unwrap();assert_eq!(c[1].pending(),Some(0x41));
        c[1].acknowledge(0x41).unwrap();io.set_line(0,false).unwrap();c[1].eoi();io.eoi(1,0x41);io.service(&mut c).unwrap();assert_eq!(c[1].pending(),None);
        reg(&mut io,0x10,0x18041);io.set_line(0,true).unwrap();io.service(&mut c).unwrap();assert_eq!(c[1].pending(),None);
        reg(&mut io,0x10,0x8041);io.service(&mut c).unwrap();assert_eq!(c[1].pending(),Some(0x41));
    }
    #[test] fn edge_logical_polarity_and_validation() {
        let mut io=IoApic::new();let mut c=cpus();c[1].write(0xd0,2<<24).unwrap();
        reg(&mut io,0x11,2<<24);reg(&mut io,0x10,0x2842); // active-low logical edge
        io.set_line(0,true).unwrap();io.set_line(0,false).unwrap();io.service(&mut c).unwrap();assert_eq!(c[1].pending(),Some(0x42));
        c[1].acknowledge(0x42).unwrap();c[1].eoi();io.service(&mut c).unwrap();assert_eq!(c[1].pending(),None);
        assert!(io.write(0x10,0x142).is_err());assert!(io.set_line(24,true).is_err());assert!(io.read(4).is_err());
        io.write(0,1).unwrap();assert_eq!(io.read(0x10).unwrap(),0x170011);
    }
}
