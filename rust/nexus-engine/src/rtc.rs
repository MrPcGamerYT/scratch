//! Read-only UTC MC146818 clock subset. No alarms, periodic IRQs or guest time setting.
//! Wall-clock epoch sampled once; subsequent updates use the engine monotonic clock.
use std::time::{SystemTime,UNIX_EPOCH};
pub struct Rtc {index:u8,nmi_disabled:bool,epoch:u64,shutdown:u8,pub accesses:u64}
impl Default for Rtc {
    fn default()->Self {Self::new(SystemTime::now().duration_since(UNIX_EPOCH).map_or(0,|d|d.as_secs()))}
}
fn bcd(value:u8)->u8 {(value/10)*16+value%10}
fn calendar(seconds:u64)->[u8;7] {
    let days=(seconds/86400) as i64;
    let z=days+719468;let era=z/146097;let doe=z-era*146097;
    let yoe=(doe-doe/1460+doe/36524-doe/146096)/365;
    let y=yoe+era*400;let doy=doe-(365*yoe+yoe/4-yoe/100);let mp=(5*doy+2)/153;
    let d=doy-(153*mp+2)/5+1;let m=mp+if mp<10 {3} else {-9};let y=y+i64::from(m<=2);
    [bcd((seconds%60) as u8),bcd(((seconds/60)%60) as u8),bcd(((seconds/3600)%24) as u8),
        bcd(((days+4)%7+1) as u8),bcd(d as u8),bcd(m as u8),bcd((y%100) as u8)]
}
impl Rtc {
    pub fn new(epoch:u64)->Self {Self{index:0,nmi_disabled:false,epoch,shutdown:0,accesses:0}}
    pub fn access(&mut self,port:u16,write:bool,value:u8,now_ns:u64)->Result<Option<u8>,String> {
        self.accesses+=1;
        match (port,write) {
            (0x70,true)=>{self.index=value&0x7f;self.nmi_disabled=value&0x80!=0;Ok(None)}
            (0x70,false)=>Ok(Some(self.index|(u8::from(self.nmi_disabled)<<7))),
            (0x71,true)=>match self.index {
                0x0a if value==0x26=>Ok(None), // 32.768kHz divider, rate6; interrupts stay off.
                0x0b if value==2=>Ok(None), // 24-hour BCD, no interrupts or SET.
                0x0f=>{self.shutdown=value;Ok(None)}
                _=>Err(format!("Unsupported RTC write register=0x{:02x}, value=0x{value:02x}; alarms, IRQs and time setting not implemented",self.index)),
            },
            (0x71,false)=>{
                let seconds=self.epoch.saturating_add(now_ns/1_000_000_000);
                let date=calendar(seconds);
                let result=match self.index {
                    0=>date[0],2=>date[1],4=>date[2],6=>date[3],7=>date[4],8=>date[5],9=>date[6],
                    0x0a=>0x26|if now_ns%1_000_000_000>=999_756_000 {0x80} else {0},
                    0x0b=>2,0x0c=>0,0x0d=>0x80,0x0f=>self.shutdown,
                    _=>return Err(format!("Unsupported RTC read register=0x{:02x}",self.index)),
                };Ok(Some(result))
            }
            _=>Err(format!("Unsupported RTC port 0x{port:x}")),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn read(r:&mut Rtc,index:u8,ns:u64)->u8 {r.access(0x70,true,index,ns).unwrap();r.access(0x71,false,0,ns).unwrap().unwrap()}
    #[test] fn utc_epoch_leap_date_and_rollover() {
        assert_eq!(calendar(0),[0,0,0,5,1,1,0x70]);
        assert_eq!(calendar(951782400),[0,0,0,3,0x29,2,0]); // 2000-02-29 Tuesday
        let mut r=Rtc::new(86399);
        assert_eq!(read(&mut r,0,0),0x59);assert_eq!(read(&mut r,4,0),0x23);
        assert_eq!(read(&mut r,0,1_000_000_000),0);assert_eq!(read(&mut r,7,1_000_000_000),2);
    }
    #[test] fn update_window_nmi_shadow_and_strict_modes() {
        let mut r=Rtc::new(0);
        assert_eq!(read(&mut r,0x0a,999_755_999),0x26);
        assert_eq!(read(&mut r,0x0a,999_756_000),0xa6);
        assert_eq!(read(&mut r,0x0a,1_000_000_000),0x26);
        r.access(0x70,true,0x8b,0).unwrap();assert_eq!(r.access(0x70,false,0,0).unwrap(),Some(0x8b));
        assert!(r.access(0x71,true,0x42,0).is_err());r.access(0x71,true,2,0).unwrap();
        assert_eq!(read(&mut r,0x0c,0),0);assert_eq!(read(&mut r,0x0d,0),0x80);
        assert!(r.access(0x72,false,0,0).is_err());assert!(r.access(0x71,true,0,0).is_err());
    }
}
