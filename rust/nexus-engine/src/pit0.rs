//! 8254 channel 0 binary LSB/MSB mode 0 and rate-generator mode 2.
//! Monotonic elapsed time, latched counter reads and edge coalescing, not I/O-count timing.
const HZ:u128=1_193_182;
#[derive(Default)]
pub struct Pit0 {
    mode:Option<u8>,low:Option<u8>,reload:u32,start:Option<u64>,
    read_high:bool,latch:Option<(u16,bool)>,reported:u64,pub programs:u64,pub edges:u64,
}
impl Pit0 {
    fn ticks(&self,now:u64)->u64 {self.start.map_or(0,|s|((u128::from(now.saturating_sub(s))*HZ)/1_000_000_000) as u64)}
    fn count(&self,now:u64)->u16 {
        if self.start.is_none() {return 0;}
        let ticks=self.ticks(now);
        if self.mode==Some(2) {(u64::from(self.reload)-ticks%u64::from(self.reload)) as u16}
        else {u64::from(self.reload).wrapping_sub(ticks) as u16}
    }
    pub fn poll(&mut self,now:u64)->bool {
        if self.start.is_none() {return false;}
        let total=if self.mode==Some(2) {self.ticks(now)/u64::from(self.reload)} else {u64::from(self.ticks(now)>=u64::from(self.reload))};
        let new=total.saturating_sub(self.reported);self.reported=total;self.edges=self.edges.saturating_add(new);new!=0
    }
    pub fn access(&mut self,port:u16,write:bool,value:u8,now:u64)->Result<Option<u8>,String> {
        match (port,write) {
            (0x43,true)=>match value {
                0=>{if self.latch.is_none() {self.latch=Some((self.count(now),false));}Ok(None)},
                0x30|0x34=>{
                    self.mode=Some((value>>1)&7);self.low=None;self.start=None;self.latch=None;self.read_high=false;self.reported=0;
                    self.programs+=1;Ok(None)
                },
                _=>Err(format!("Unsupported PIT channel0 control 0x{value:02x}")),
            },
            (0x40,true)=>{
                if self.mode.is_none() {return Err("PIT channel0 must be programmed before count writes".into());}
                if let Some(low)=self.low.take() {
                    let reload=u16::from_le_bytes([low,value]);let reload=if reload==0 {65536} else {u32::from(reload)};
                    if self.mode==Some(2)&&reload<2 {return Err("PIT mode2 count must be at least two".into());}
                    self.reload=reload;self.start=Some(now);self.reported=0;self.read_high=false;
                } else {self.low=Some(value);self.start=None;}
                Ok(None)
            },
            (0x40,false)=>{
                if let Some((count,high))=self.latch {
                    self.latch=if high {None} else {Some((count,true))};
                    return Ok(Some(if high {(count>>8) as u8} else {count as u8}));
                }
                let count=self.count(now);let byte=if self.read_high {(count>>8) as u8} else {count as u8};
                self.read_high = !self.read_high;Ok(Some(byte))
            },
            _=>Err(format!("Unsupported PIT channel0 access 0x{port:x}")),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn setup(mode:u8,count:u16)->Pit0 {
        let mut p=Pit0::default();p.access(0x43,true,mode,0).unwrap();p.access(0x40,true,count as u8,0).unwrap();p.access(0x40,true,(count>>8) as u8,0).unwrap();p
    }
    #[test] fn periodic_edges_and_no_io_driven_time() {
        let mut p=setup(0x34,1193);
        for _ in 0..1000 {assert!(!p.poll(0));}
        assert!(!p.poll(999_000));assert!(p.poll(1_000_000));assert!(!p.poll(1_000_000));
        assert!(p.poll(10_000_000));assert_eq!(p.edges,10);assert_eq!(p.count(10_000_000),1192);
    }
    #[test] fn oneshot_zero_reload_and_reprogram() {
        let mut p=setup(0x30,0);assert!(!p.poll(54_000_000));assert!(p.poll(55_000_000));assert!(!p.poll(100_000_000));
        p.access(0x43,true,0x34,100_000_000).unwrap();assert!(!p.poll(200_000_000));
        p.access(0x40,true,0,200_000_000).unwrap();assert!(!p.poll(300_000_000));
        p.access(0x40,true,0,300_000_000).unwrap();assert!(!p.poll(354_000_000));assert!(p.poll(355_000_000));
    }
    #[test] fn stable_latch_and_strict_control() {
        let mut p=setup(0x34,65535);let count=p.count(1_000_000);
        p.access(0x43,true,0,1_000_000).unwrap();let low=p.access(0x40,false,0,2_000_000).unwrap().unwrap();
        p.access(0x43,true,0,3_000_000).unwrap();let high=p.access(0x40,false,0,4_000_000).unwrap().unwrap();
        assert_eq!(u16::from_le_bytes([low,high]),count);
        for v in [0x36,0x31,0x54,0xc2] {assert!(p.access(0x43,true,v,0).is_err());}
    }
}
