//! Edge-triggered local APIC interrupt arbitration. Includes a 100 MHz virtual bus timer; no level-source EOI yet.
//! Device state is independent of WHP; acknowledge only after CPU injection succeeds.
pub struct LocalApic {id:u8,svr:u32,tpr:u8,ldr:u8,irr:[u32;8],isr:[u32;8],irr_level:[u32;8],isr_level:[u32;8],other_lvt:[u32;5],timer_lvt:u32,timer_initial:u32,timer_current:u32,timer_divide:u32,timer_fraction:u64}
fn highest(bits:&[u32;8])->Option<u8> {
    for i in (0..8).rev() {if bits[i]!=0 {return Some((i*32+31-bits[i].leading_zeros() as usize) as u8);}}
    None
}
impl LocalApic {
    pub fn new(id:u8)->Self {Self{id,svr:255,tpr:0,ldr:0,irr:[0;8],isr:[0;8],irr_level:[0;8],isr_level:[0;8],other_lvt:[1<<16;5],timer_lvt:1<<16,timer_initial:0,timer_current:0,timer_divide:0,timer_fraction:0}}
    /// Advance monotonic virtual bus time, independent of host scheduling frequency.
    /// Fractional divider ticks survive rendezvous; missed periods coalesce in IRR.
    pub fn advance_ns(&mut self,ns:u64)->Result<(),String> {
        let shift=match self.timer_divide&0xb {0=>1,1=>2,2=>3,3=>4,8=>5,9=>6,10=>7,_=>0};
        let quantum=10u64<<shift;
        if self.timer_current==0 {return Ok(());}
        let total=u128::from(ns)+u128::from(self.timer_fraction);
        let ticks=total/u128::from(quantum);self.timer_fraction=(total%u128::from(quantum)) as u64;
        if ticks<u128::from(self.timer_current) {self.timer_current-=ticks as u32;return Ok(());}
        let remaining=ticks-u128::from(self.timer_current);
        self.timer_current=if self.timer_lvt&(1<<17)!=0&&self.timer_initial!=0 {
            self.timer_initial-(remaining%u128::from(self.timer_initial)) as u32
        } else {0};
        if self.timer_lvt&(1<<16)==0 {self.request_edge(self.timer_lvt as u8)?;}
        Ok(())
    }
    fn ppr(&self)->u8 {
        let service=highest(&self.isr).map_or(0,|v|v&0xf0);
        if self.tpr&0xf0>=service {self.tpr} else {service}
    }
    pub fn request_edge(&mut self,vector:u8)->Result<(),String> {
        if vector<16 {return Err("APIC fixed vector must be 16..255".into());}
        self.irr[usize::from(vector/32)]|=1u32<<(vector%32);Ok(())
    }
    pub fn request_level(&mut self,vector:u8)->Result<(),String> {
        self.request_edge(vector)?;self.irr_level[usize::from(vector/32)]|=1u32<<(vector%32);Ok(())
    }
    pub fn in_service_level(&self,vector:u8)->bool {self.isr_level[usize::from(vector/32)]&(1u32<<(vector%32))!=0}
    pub fn pending(&self)->Option<u8> {
        if self.svr&0x100==0 {return None;}
        highest(&self.irr).filter(|v|v&0xf0>self.ppr()&0xf0)
    }
    pub fn acknowledge(&mut self,vector:u8)->Result<(),String> {
        if self.pending()!=Some(vector) {return Err("APIC acknowledge without deliverable vector".into());}
        self.irr[usize::from(vector/32)]&=!(1u32<<(vector%32));
        let index=usize::from(vector/32);let bit=1u32<<(vector%32);
        self.isr[index]|=bit;self.isr_level[index]=(self.isr_level[index]&!bit)|(self.irr_level[index]&bit);self.irr_level[index]&=!bit;Ok(())
    }
    pub fn service_vector(&self)->Option<u8> {highest(&self.isr)}
    pub fn eoi(&mut self)->Option<u8> {
        let v=highest(&self.isr)?;self.isr[usize::from(v/32)]&=!(1u32<<(v%32));self.isr_level[usize::from(v/32)]&=!(1u32<<(v%32));Some(v)
    }
    pub fn read(&self,offset:u32)->Result<u32,String> {
        match offset {
            0x330..=0x370 if offset%16==0=>Ok(self.other_lvt[((offset-0x330)/16) as usize]),
            0x280|0x300=>Ok(0),
            0x320=>Ok(self.timer_lvt),0x380=>Ok(self.timer_initial),0x390=>Ok(self.timer_current),0x3e0=>Ok(self.timer_divide),
            0x20=>Ok(u32::from(self.id)<<24),0x30=>Ok(0x00050014),
            0x80=>Ok(u32::from(self.tpr)),0xd0=>Ok(u32::from(self.ldr)<<24),0xe0=>Ok(0xffffffff),0xa0=>Ok(u32::from(self.ppr())),0xf0=>Ok(self.svr),
            0x100..=0x170 if offset%16==0=>Ok(self.isr[((offset-0x100)/16) as usize]),
            0x180..=0x1f0 if offset%16==0=>Ok(self.isr_level[((offset-0x180)/16) as usize]),
            0x200..=0x270 if offset%16==0=>Ok(self.irr[((offset-0x200)/16) as usize]),
            _=>Err(format!("APIC register read 0x{offset:x} not implemented")),
        }
    }
    pub fn write(&mut self,offset:u32,value:u32)->Result<(),String> {
        match offset {
            0x280=>Ok(()), // no local APIC error source is generated
            0x330..=0x370 if offset%16==0&&value&(1<<16)!=0=>{self.other_lvt[((offset-0x330)/16) as usize]=value;Ok(())},
            0x320 if value&!0x300ff==0&&value&(1<<18)==0&&((value&0xff)>=16||value&(1<<16)!=0)=>{self.timer_lvt=value;Ok(())},
            0x380=>{self.timer_initial=value;self.timer_current=value;self.timer_fraction=0;Ok(())},
            0x3e0 if value&!0xb==0=>{self.timer_divide=value;self.timer_fraction=0;Ok(())},
            0x80=>{self.tpr=value as u8;Ok(())},
            0xb0=>{self.eoi();Ok(())},
            0xd0=>{self.ldr=(value>>24) as u8;Ok(())},
            0xe0 if value&0xf0000000==0xf0000000=>Ok(()), // flat logical mode, reserved bits read as ones

            0xf0 if value&!0x1ff==0=>{self.svr=value;Ok(())},
            _=>Err(format!("APIC register write 0x{offset:x} not implemented")),
        }
    }
}
/// Physical destination or ICR shorthand: 1=self, 2=all, 3=all excluding self.
/// Validate the whole request before changing any destination IRR.
pub fn send_fixed(cpus:&mut [LocalApic],source:usize,destination:u8,shorthand:u8,vector:u8)->Result<(),String> {
    send_fixed_mode(cpus,source,destination,shorthand,vector,false)
}
pub fn send_fixed_logical(cpus:&mut [LocalApic],source:usize,destination:u8,shorthand:u8,vector:u8)->Result<(),String> {
    send_fixed_mode(cpus,source,destination,shorthand,vector,true)
}
fn send_fixed_mode(cpus:&mut [LocalApic],source:usize,destination:u8,shorthand:u8,vector:u8,logical:bool)->Result<(),String> {
    if cpus.is_empty()||cpus.len()>8||source>=cpus.len()||vector<16||shorthand>3 {return Err("Invalid fixed IPI request".into());}
    let explicit=cpus.iter().position(|cpu|cpu.id==destination);
    if !logical&&shorthand==0&&explicit.is_none() {return Err("Fixed IPI targets nonexistent APIC ID".into());}
    for (index,cpu) in cpus.iter_mut().enumerate() {
        let selected=match shorthand {0=>if logical {cpu.ldr&destination!=0} else {Some(index)==explicit},1=>index==source,2=>true,3=>index!=source,_=>false};
        if selected {cpu.request_edge(vector)?;}
    }Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn trigger_mode_tracks_acknowledged_vector_and_clears_on_eoi() {
        let mut a=LocalApic::new(0);a.write(0xf0,0x1ff).unwrap();
        a.request_level(0x41).unwrap();assert!(!a.in_service_level(0x41));
        a.acknowledge(0x41).unwrap();assert!(a.in_service_level(0x41));
        a.request_edge(0x60).unwrap();a.acknowledge(0x60).unwrap();assert!(!a.in_service_level(0x60));
        assert_eq!(a.service_vector(),Some(0x60));a.eoi();assert!(a.in_service_level(0x41));
        a.request_edge(0x41).unwrap();a.eoi();assert!(!a.in_service_level(0x41));
        a.acknowledge(0x41).unwrap();assert!(!a.in_service_level(0x41));
        assert!(a.request_level(15).is_err());
    }
    #[test] fn timer_countdown_periodic_masking_and_reprogramming() {
        let mut a=LocalApic::new(0);a.write(0xf0,0x1ff).unwrap();
        assert_eq!(a.read(0x320).unwrap(),1<<16);
        a.write(0x320,0x50).unwrap();a.write(0x380,3).unwrap();
        a.advance_ns(19).unwrap();assert_eq!(a.read(0x390).unwrap(),3);
        a.advance_ns(1).unwrap();assert_eq!(a.read(0x390).unwrap(),2);
        a.advance_ns(40).unwrap();assert_eq!(a.pending(),Some(0x50));assert_eq!(a.read(0x390).unwrap(),0);
        a.acknowledge(0x50).unwrap();a.eoi();a.advance_ns(u64::MAX).unwrap();assert_eq!(a.pending(),None);
        a.write(0x320,0x30050).unwrap();a.write(0x3e0,0xb).unwrap();a.write(0x380,3).unwrap();
        a.advance_ns(100).unwrap();assert_eq!(a.read(0x390).unwrap(),2);assert_eq!(a.pending(),None);
        a.write(0x320,0x20050).unwrap();a.advance_ns(20).unwrap();assert_eq!(a.pending(),Some(0x50));
        a.acknowledge(0x50).unwrap();a.advance_ns(300).unwrap();assert_eq!(a.pending(),None);
        a.eoi();assert_eq!(a.pending(),Some(0x50));
        a.write(0x380,0).unwrap();a.advance_ns(u64::MAX).unwrap();assert_eq!(a.read(0x390).unwrap(),0);
        assert!(a.write(0x320,0x40050).is_err());assert!(a.write(0x390,9).is_err());
    }
    #[test] fn timer_all_dividers_and_disabled_apic() {
        for (encoding,divide) in [(0,2),(1,4),(2,8),(3,16),(8,32),(9,64),(10,128),(11,1)] {
            let mut a=LocalApic::new(0);a.write(0x320,0x50).unwrap();a.write(0x3e0,encoding).unwrap();a.write(0x380,1).unwrap();
            a.advance_ns(divide*10-1).unwrap();assert_eq!(a.read(0x390).unwrap(),1);
            a.advance_ns(1).unwrap();assert_eq!(a.pending(),None);
            a.write(0xf0,0x1ff).unwrap();assert_eq!(a.pending(),Some(0x50));
        }
    }
    #[test] fn flat_logical_destinations_and_mode_validation() {
        let mut cpus=vec![LocalApic::new(0),LocalApic::new(1),LocalApic::new(2)];
        for (i,cpu) in cpus.iter_mut().enumerate() {cpu.write(0xf0,0x1ff).unwrap();cpu.write(0xd0,1<<(24+i)).unwrap();}
        assert!(cpus[0].write(0xe0,0).is_err());assert_eq!(cpus[0].read(0xe0).unwrap(),0xffffffff);
        send_fixed_logical(&mut cpus,0,6,0,0x50).unwrap();
        assert_eq!(cpus.iter().map(LocalApic::pending).collect::<Vec<_>>(),vec![None,Some(0x50),Some(0x50)]);
        send_fixed_logical(&mut cpus,0,0,1,0x60).unwrap();assert_eq!(cpus[0].pending(),Some(0x60));
    }
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
