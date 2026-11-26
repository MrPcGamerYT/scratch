//! Edge-triggered local APIC interrupt arbitration. No timers or level-source EOI yet.
//! Device state is independent of WHP; acknowledge only after CPU injection succeeds.
pub struct LocalApic {id:u8,svr:u32,tpr:u8,irr:[u32;8],isr:[u32;8]}
fn highest(bits:&[u32;8])->Option<u8> {
    for i in (0..8).rev() {if bits[i]!=0 {return Some((i*32+31-bits[i].leading_zeros() as usize) as u8);}}
    None
}
impl LocalApic {
    pub fn new(id:u8)->Self {Self{id,svr:255,tpr:0,irr:[0;8],isr:[0;8]}}
    fn ppr(&self)->u8 {
        let service=highest(&self.isr).map_or(0,|v|v&0xf0);
        if self.tpr&0xf0>=service {self.tpr} else {service}
    }
    pub fn request_edge(&mut self,vector:u8)->Result<(),String> {
        if vector<16 {return Err("APIC fixed vector must be 16..255".into());}
        self.irr[usize::from(vector/32)]|=1u32<<(vector%32);Ok(())
    }
    pub fn pending(&self)->Option<u8> {
        if self.svr&0x100==0 {return None;}
        highest(&self.irr).filter(|v|v&0xf0>self.ppr()&0xf0)
    }
    pub fn acknowledge(&mut self,vector:u8)->Result<(),String> {
        if self.pending()!=Some(vector) {return Err("APIC acknowledge without deliverable vector".into());}
        self.irr[usize::from(vector/32)]&=!(1u32<<(vector%32));
        self.isr[usize::from(vector/32)]|=1u32<<(vector%32);Ok(())
    }
    pub fn eoi(&mut self)->Option<u8> {
        let v=highest(&self.isr)?;self.isr[usize::from(v/32)]&=!(1u32<<(v%32));Some(v)
    }
    pub fn read(&self,offset:u32)->Result<u32,String> {
        match offset {
            0x20=>Ok(u32::from(self.id)<<24),0x30=>Ok(0x00050014),
            0x80=>Ok(u32::from(self.tpr)),0xa0=>Ok(u32::from(self.ppr())),0xf0=>Ok(self.svr),
            0x100..=0x170 if offset%16==0=>Ok(self.isr[((offset-0x100)/16) as usize]),
            0x200..=0x270 if offset%16==0=>Ok(self.irr[((offset-0x200)/16) as usize]),
            _=>Err(format!("APIC register read 0x{offset:x} not implemented")),
        }
    }
    pub fn write(&mut self,offset:u32,value:u32)->Result<(),String> {
        match offset {
            0x80=>{self.tpr=value as u8;Ok(())},
            0xb0=>{self.eoi();Ok(())},
            0xf0 if value&!0x1ff==0=>{self.svr=value;Ok(())},
            _=>Err(format!("APIC register write 0x{offset:x} not implemented")),
        }
    }
}
/// Physical destination or ICR shorthand: 1=self, 2=all, 3=all excluding self.
/// Validate the whole request before changing any destination IRR.
pub fn send_fixed(cpus:&mut [LocalApic],source:usize,destination:u8,shorthand:u8,vector:u8)->Result<(),String> {
    if cpus.is_empty()||cpus.len()>8||source>=cpus.len()||vector<16||shorthand>3 {return Err("Invalid fixed IPI request".into());}
    let explicit=cpus.iter().position(|cpu|cpu.id==destination);
    if shorthand==0&&explicit.is_none() {return Err("Fixed IPI targets nonexistent APIC ID".into());}
    for (index,cpu) in cpus.iter_mut().enumerate() {
        let selected=match shorthand {0=>Some(index)==explicit,1=>index==source,2=>true,3=>index!=source,_=>false};
        if selected {cpu.request_edge(vector)?;}
    }Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn physical_destination_and_shorthand_do_not_touch_other_cpus() {
        let mut cpus=vec![LocalApic::new(0),LocalApic::new(1),LocalApic::new(2)];
        for cpu in &mut cpus {cpu.write(0xf0,0x1ff).unwrap();}
        send_fixed(&mut cpus,0,2,0,0x40).unwrap();
        assert_eq!(cpus.iter().map(LocalApic::pending).collect::<Vec<_>>(),vec![None,None,Some(0x40)]);
        assert!(send_fixed(&mut cpus,0,9,0,0x50).is_err());
        assert!(send_fixed(&mut cpus,0,0,2,15).is_err());assert_eq!(cpus[0].pending(),None);
        send_fixed(&mut cpus,1,0,1,0x60).unwrap();assert_eq!(cpus[1].pending(),Some(0x60));
        send_fixed(&mut cpus,1,0,3,0x70).unwrap();
        assert_eq!(cpus.iter().map(LocalApic::pending).collect::<Vec<_>>(),vec![Some(0x70),Some(0x60),Some(0x70)]);
        send_fixed(&mut cpus,0,0,2,0x80).unwrap();assert!(cpus.iter().all(|c|c.pending()==Some(0x80)));
    }
    #[test] fn disabled_masked_priority_and_deferred_acknowledgement() {
        let mut a=LocalApic::new(2);a.request_edge(0x40).unwrap();assert_eq!(a.pending(),None);
        a.write(0xf0,0x1ff).unwrap();assert_eq!(a.pending(),Some(0x40));
        assert_eq!(a.pending(),Some(0x40));assert_eq!(a.read(0x120).unwrap(),0);
        a.write(0x80,0x4f).unwrap();assert_eq!(a.pending(),None);assert!(a.acknowledge(0x40).is_err());
        a.write(0x80,0).unwrap();a.acknowledge(0x40).unwrap();assert_eq!(a.pending(),None);
        assert_eq!(a.read(0x120).unwrap(),1);assert_eq!(a.read(0xa0).unwrap(),0x40);
        assert_eq!(a.eoi(),Some(0x40));assert_eq!(a.eoi(),None);assert_eq!(a.read(0x20).unwrap(),2<<24);
    }
    #[test] fn nested_higher_priority_and_same_class_waits_for_eoi() {
        let mut a=LocalApic::new(0);a.write(0xf0,0x1ff).unwrap();
        a.request_edge(0x41).unwrap();a.acknowledge(0x41).unwrap();
        a.request_edge(0x4f).unwrap();a.request_edge(0x30).unwrap();assert_eq!(a.pending(),None);
        a.request_edge(0x60).unwrap();assert_eq!(a.pending(),Some(0x60));a.acknowledge(0x60).unwrap();
        assert_eq!(a.eoi(),Some(0x60));assert_eq!(a.pending(),None);
        assert_eq!(a.eoi(),Some(0x41));assert_eq!(a.pending(),Some(0x4f));
        a.acknowledge(0x4f).unwrap();a.eoi();assert_eq!(a.pending(),Some(0x30));
    }
    #[test] fn edge_coalescing_and_invalid_registers_leave_requests_intact() {
        let mut a=LocalApic::new(0);a.write(0xf0,0x1ff).unwrap();
        a.request_edge(255).unwrap();a.request_edge(255).unwrap();
        assert!(a.request_edge(15).is_err());assert!(a.write(0xf0,0x3ff).is_err());
        assert_eq!(a.pending(),Some(255));a.acknowledge(255).unwrap();a.eoi();assert_eq!(a.pending(),None);
        assert!(a.read(0x201).is_err());assert!(a.write(0x200,0).is_err());
    }
}
