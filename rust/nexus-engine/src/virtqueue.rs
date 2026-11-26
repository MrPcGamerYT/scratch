//! Bounded split-ring snapshot parser for a stopped single-vCPU machine.
//! No indirect descriptors, EVENT_IDX, packed rings, MMIO transport or concurrent DMA.
use crate::guest_memory::GuestMemory;
pub const MAX_QUEUE:u16=256;
pub const MAX_TRANSFER:usize=128*1024;
#[derive(Clone,Copy,Debug)]
pub struct Descriptor {pub address:u64,pub length:u32,pub writable:bool}
fn overlap(a:u64,al:u64,b:u64,bl:u64)->bool {a<b+bl&&b<a+al}
fn read16(m:&GuestMemory<'_>,address:u64)->Result<u16,String> {let mut b=[0;2];m.read(address,&mut b)?;Ok(u16::from_le_bytes(b))}
pub struct SplitQueue {size:u16,desc:u64,avail:u64,used:u64,last_avail:u16,next_used:u16}
pub struct Chain {pub head:u16,pub descriptors:Vec<Descriptor>,pub notify:bool}
impl SplitQueue {
    pub fn new(m:&GuestMemory<'_>,size:u16,desc:u64,avail:u64,used:u64)->Result<Self,String> {
        if size==0||size>MAX_QUEUE||!size.is_power_of_two()||desc%16!=0||avail%2!=0||used%4!=0 {return Err("Invalid split queue size/alignment".into());}
        let regions=[(desc,16*u64::from(size),false),(avail,4+2*u64::from(size),false),(used,4+8*u64::from(size),true)];
        for (address,length,write) in regions {m.validate(address,length as usize,write)?;}
        for (n,a) in regions.iter().enumerate() {for b in &regions[n+1..] {if overlap(a.0,a.1,b.0,b.1) {return Err("Overlapping queue metadata".into());}}}
        if read16(m,used+2)?!=0 {return Err("Fresh queue used index must be zero".into());}
        Ok(Self{size,desc,avail,used,last_avail:0,next_used:0})
    }
    pub fn peek(&self,m:&GuestMemory<'_>)->Result<Option<Chain>,String> {
        if read16(m,self.used+2)?!=self.next_used {return Err("Guest changed device-owned used index".into());}
        let avail_flags=read16(m,self.avail)?;if avail_flags&!1!=0 {return Err("Unsupported available-ring flags".into());}
        let pending=read16(m,self.avail+2)?.wrapping_sub(self.last_avail);
        if pending>self.size {return Err("Available index advanced beyond queue capacity".into());}
        if pending==0 {return Ok(None);}
        let head=read16(m,self.avail+4+2*u64::from(self.last_avail%self.size))?;
        let mut index=head;let mut visited=[false;MAX_QUEUE as usize];let mut descriptors=Vec::new();
        loop {
            if index>=self.size||visited[index as usize] {return Err("Descriptor cycle or invalid index".into());}visited[index as usize]=true;
            let mut raw=[0;16];m.read(self.desc+16*u64::from(index),&mut raw)?;
            let address=u64::from_le_bytes(raw[..8].try_into().unwrap());let length=u32::from_le_bytes(raw[8..12].try_into().unwrap());
            let flags=u16::from_le_bytes(raw[12..14].try_into().unwrap());let next=u16::from_le_bytes(raw[14..].try_into().unwrap());
            if flags&!3!=0||length==0 {return Err("Unsupported indirect/flags or empty descriptor".into());}
            let writable=flags&2!=0;m.validate(address,length as usize,writable)?;
            for (base,len) in [(self.desc,16*u64::from(self.size)),(self.avail,4+2*u64::from(self.size)),(self.used,4+8*u64::from(self.size))] {
                if overlap(address,u64::from(length),base,len) {return Err("Descriptor aliases queue metadata".into());}
            }
            if descriptors.iter().any(|d:&Descriptor|overlap(address,u64::from(length),d.address,u64::from(d.length))) {return Err("Aliased request descriptors".into());}
            descriptors.push(Descriptor{address,length,writable});
            if flags&1==0 {break;}index=next;
        }
        Ok(Some(Chain{head,descriptors,notify:avail_flags&1==0}))
    }
    // Only call after all request writes have finished, while the vCPU remains stopped.
    pub(crate) fn complete(&mut self,m:&mut GuestMemory<'_>,head:u16,written:u32)->Result<(),String> {
        let address=self.used+4+8*u64::from(self.next_used%self.size);let mut entry=[0;8];
        entry[..4].copy_from_slice(&u32::from(head).to_le_bytes());entry[4..].copy_from_slice(&written.to_le_bytes());m.write(address,&entry)?;
        let next=self.next_used.wrapping_add(1);m.write(self.used+2,&next.to_le_bytes())?;
        self.next_used=next;self.last_avail=self.last_avail.wrapping_add(1);Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guest_memory::RamRegion;
    #[test] fn indices_wrap_without_replaying_or_losing_a_descriptor() {
        let mut ram=vec![0;0x4000];
        ram[0x100..0x108].copy_from_slice(&0x1000u64.to_le_bytes());ram[0x108..0x10c].copy_from_slice(&1u32.to_le_bytes());
        let mut m=GuestMemory::new(vec![RamRegion{gpa:0,bytes:&mut ram,writable:true}]).unwrap();
        let mut q=SplitQueue::new(&m,8,0x100,0x200,0x300).unwrap();q.last_avail=u16::MAX;q.next_used=u16::MAX;
        m.write(0x302,&u16::MAX.to_le_bytes()).unwrap();m.write(0x202,&0u16.to_le_bytes()).unwrap();
        let c=q.peek(&m).unwrap().unwrap();assert_eq!(c.head,0);q.complete(&mut m,0,1).unwrap();
        assert_eq!(q.last_avail,0);assert_eq!(q.next_used,0);assert!(q.peek(&m).unwrap().is_none());
    }
}
