//! Checked scatter/gather guest RAM access. No guest pointer is cast to a host pointer.
//! Caller must stop all vCPUs/device workers accessing these regions for this borrow.
//! This is NOT a concurrent DMA/shared-memory synchronization API.
use std::ops::Range;
pub struct RamRegion<'a> {pub gpa:u64,pub bytes:&'a mut [u8],pub writable:bool}
pub struct GuestMemory<'a> {regions:Vec<RamRegion<'a>>}
impl<'a> GuestMemory<'a> {
    pub fn new(mut regions:Vec<RamRegion<'a>>)->Result<Self,String> {
        if regions.is_empty()||regions.len()>128 {return Err("RAM map must contain 1..128 regions".into());}
        regions.sort_by_key(|r|r.gpa);let mut previous=0;
        for (n,r) in regions.iter().enumerate() {
            let end=r.gpa.checked_add(r.bytes.len() as u64).ok_or("RAM region overflow")?;
            if r.bytes.is_empty()||(n!=0&&r.gpa<previous) {return Err("Empty or overlapping RAM region".into());}previous=end;
        }Ok(Self{regions})
    }
    fn spans(&self,gpa:u64,length:usize,write:bool)->Result<Vec<(usize,Range<usize>)>,String> {
        if length==0 {return Err("Empty DMA range".into());}
        let end=gpa.checked_add(length as u64).ok_or("DMA address overflow")?;let mut cursor=gpa;let mut spans=Vec::new();
        for (n,r) in self.regions.iter().enumerate() {
            let region_end=r.gpa+r.bytes.len() as u64;
            if cursor>=region_end {continue;}
            if cursor<r.gpa {break;}
            if write&&!r.writable {return Err("DMA targets read-only RAM".into());}
            let stop=end.min(region_end);spans.push((n,(cursor-r.gpa) as usize..(stop-r.gpa) as usize));cursor=stop;
            if cursor==end {return Ok(spans);}
        }Err("DMA range crosses unmapped RAM/MMIO hole".into())
    }
    pub fn validate(&self,gpa:u64,length:usize,write:bool)->Result<(),String> {self.spans(gpa,length,write).map(|_|())}
    pub fn read(&self,gpa:u64,data:&mut [u8])->Result<(),String> {
        let spans=self.spans(gpa,data.len(),false)?;let mut offset=0;
        for (n,range) in spans {let size=range.len();data[offset..offset+size].copy_from_slice(&self.regions[n].bytes[range]);offset+=size;}Ok(())
    }
    pub fn write(&mut self,gpa:u64,data:&[u8])->Result<(),String> {
        let spans=self.spans(gpa,data.len(),true)?;let mut offset=0;
        for (n,range) in spans {let size=range.len();self.regions[n].bytes[range].copy_from_slice(&data[offset..offset+size]);offset+=size;}Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn scatter_gather_across_adjacent_noncontiguous_host_allocations() {
        let mut a=[1;8];let mut b=[2;8];
        let mut m=GuestMemory::new(vec![RamRegion{gpa:8,bytes:&mut b,writable:true},RamRegion{gpa:0,bytes:&mut a,writable:true}]).unwrap();
        let mut data=[0;8];m.read(4,&mut data).unwrap();assert_eq!(data,[1,1,1,1,2,2,2,2]);m.write(6,&[3;4]).unwrap();
        m.read(4,&mut data).unwrap();assert_eq!(data,[1,1,3,3,3,3,2,2]);
    }
    #[test] fn holes_overflow_and_readonly_are_rejected_before_any_copy() {
        let mut a=[1;8];let mut b=[2;8];
        let mut m=GuestMemory::new(vec![RamRegion{gpa:0,bytes:&mut a,writable:true},RamRegion{gpa:16,bytes:&mut b,writable:false}]).unwrap();
        assert!(m.write(4,&[7;16]).is_err());assert!(m.write(16,&[7;4]).is_err());assert!(m.validate(u64::MAX,2,false).is_err());
        let mut data=[0;8];m.read(0,&mut data).unwrap();assert_eq!(data,[1;8]);assert!(m.read(0,&mut []).is_err());
    }
    #[test] fn overlapping_and_overflowing_regions_fail() {
        let mut a=[0;8];let mut b=[0;8];
        assert!(GuestMemory::new(vec![RamRegion{gpa:0,bytes:&mut a,writable:true},RamRegion{gpa:4,bytes:&mut b,writable:true}]).is_err());
        assert!(GuestMemory::new(vec![RamRegion{gpa:u64::MAX,bytes:&mut a,writable:true}]).is_err());
    }
}
