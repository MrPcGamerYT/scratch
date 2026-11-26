//! Intel 8254 channel-2 binary mode-0 subset, using elapsed monotonic nanoseconds.
//! No IRQ0/channel0, refresh, audio output, BCD, read-back status or other modes.
const HZ:u128=1_193_182;
#[derive(Default)]
pub struct Pit2 {
    control:u8, gate:bool, speaker:bool, loaded:bool, low:Option<u8>,
    reload:u32, elapsed:u64, started:Option<u64>, read_high:bool,
    latch:Option<(u16,bool)>,pub programs:u64,pub accesses:u64,
}
impl Pit2 {
    fn ticks(&self,now:u64)->u64 {
        let elapsed=self.elapsed.saturating_add(self.started.map_or(0,|start|now.saturating_sub(start)));
        ((u128::from(elapsed)*HZ)/1_000_000_000) as u64
    }
    fn count(&self,now:u64)->u16 {u64::from(self.reload).wrapping_sub(self.ticks(now)) as u16}
    fn output(&self,now:u64)->bool {
        if self.control==0 {true} else {self.loaded&&self.ticks(now)>=u64::from(self.reload)}
    }
    fn set_gate(&mut self,gate:bool,now:u64) {
        if gate==self.gate {return;}
        if let Some(start)=self.started.take() {self.elapsed=self.elapsed.saturating_add(now.saturating_sub(start));}
        self.gate=gate;
        if gate&&self.loaded {self.started=Some(now);}
    }
    pub fn access(&mut self,port:u16,write:bool,value:u8,now:u64)->Result<Option<u8>,String> {
        self.accesses+=1;
        match (port,write) {
            (0x61,true)=>{self.set_gate(value&1!=0,now);self.speaker=value&2!=0;Ok(None)}
            (0x61,false)=>Ok(Some(u8::from(self.gate)|(u8::from(self.speaker)<<1)|(u8::from(self.output(now))<<5))),
            (0x43,true)=>{
                if value==0x80 { // Channel 2 counter-latch command; don't replace an unread latch.
                    if self.latch.is_none() {self.latch=Some((self.count(now),false));}
                } else if value==0xb0 { // Channel2, LSB+MSB, mode0, binary.
                    self.control=value;self.loaded=false;self.low=None;self.started=None;
                    self.elapsed=0;self.read_high=false;self.latch=None;self.programs+=1;
                } else {return Err(format!("Unsupported PIT control 0x{value:02x}; only channel2 binary mode0/latch implemented"));}
                Ok(None)
            }
            (0x42,true)=>{
                if self.control!=0xb0 {return Err("PIT channel2 must be programmed before count writes".into());}
                if let Some(low)=self.low.take() {
                    let count=u16::from_le_bytes([low,value]);self.reload=if count==0 {65536} else {u32::from(count)};
                    self.elapsed=0;self.loaded=true;self.started=if self.gate {Some(now)} else {None};self.read_high=false;
                } else {
                    self.low=Some(value);self.loaded=false;self.elapsed=0;self.started=None;
                }
                Ok(None)
            }
            (0x42,false)=>{
                if let Some((count,high))=self.latch {
                    let byte=if high {(count>>8) as u8} else {count as u8};
                    self.latch=if high {None} else {Some((count,true))};return Ok(Some(byte));
                }
                let count=self.count(now);let byte=if self.read_high {(count>>8) as u8} else {count as u8};
                self.read_high = !self.read_high;Ok(Some(byte))
            }
            _=>Err(format!("Unsupported PIT access port=0x{port:x}, write={write}")),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn setup(count:u16)->Pit2 {
        let mut p=Pit2::default();p.access(0x61,true,1,0).unwrap();p.access(0x43,true,0xb0,0).unwrap();
        p.access(0x42,true,count as u8,0).unwrap();p.access(0x42,true,(count>>8) as u8,0).unwrap();p
    }
    #[test] fn clock_drives_terminal_output_not_read_count() {
        let mut p=setup(1193);
        for _ in 0..10000 {assert_eq!(p.access(0x61,false,0,0).unwrap(),Some(1));}
        assert_eq!(p.count(500_000),597);assert!(!p.output(999_000));
        assert_eq!(p.access(0x61,false,0,1_000_000).unwrap(),Some(33));
        assert!(p.output(2_000_000)); // OUT stays high after mode0 terminal count.
    }
    #[test] fn gate_pauses_and_resumes_and_speaker_is_only_shadow() {
        let mut p=setup(1193);p.access(0x61,true,2,500_000).unwrap();let count=p.count(500_000);
        assert_eq!(p.count(10_000_000),count);assert_eq!(p.access(0x61,false,0,10_000_000).unwrap(),Some(2));
        p.access(0x61,true,3,10_000_000).unwrap();assert!(!p.output(10_400_000));assert!(p.output(10_500_000));
        assert_eq!(p.access(0x61,false,0,10_500_000).unwrap(),Some(35));
    }
    #[test] fn zero_reload_and_coherent_latch() {
        let mut p=setup(0);assert_eq!(p.reload,65536);
        p.access(0x43,true,0x80,1_000_000).unwrap();let count=p.count(1_000_000);
        let low=p.access(0x42,false,0,2_000_000).unwrap().unwrap();
        p.access(0x43,true,0x80,3_000_000).unwrap(); // Must not clobber unread high byte.
        let high=p.access(0x42,false,0,4_000_000).unwrap().unwrap();assert_eq!(u16::from_le_bytes([low,high]),count);
        assert!(!p.output(54_000_000));assert!(p.output(55_000_000));
    }
    #[test] fn partial_reload_and_unsupported_modes() {
        let mut p=setup(10);p.access(0x42,true,20,100_000).unwrap();assert!(!p.output(200_000));
        p.access(0x42,true,0,200_000).unwrap();assert!(!p.output(210_000));assert!(p.output(220_000));
        for control in [0x34,0xb6,0xb1,0xc8] {assert!(p.access(0x43,true,control,0).is_err());}
        assert!(p.access(0x43,false,0,0).is_err());assert!(p.access(0x40,false,0,0).is_err());
    }
}
