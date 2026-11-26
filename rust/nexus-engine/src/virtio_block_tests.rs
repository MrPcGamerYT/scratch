use crate::{guest_memory::{GuestMemory,RamRegion},virtqueue::SplitQueue,virtio_block::{BlockBackend,process_one},image,overlay::Overlay};
fn put16(r:&mut [u8],at:usize,v:u16) {r[at..at+2].copy_from_slice(&v.to_le_bytes());}
fn desc(r:&mut [u8],n:usize,address:u64,len:u32,flags:u16,next:u16) {
    let at=0x100+16*n;r[at..at+8].copy_from_slice(&address.to_le_bytes());r[at+8..at+12].copy_from_slice(&len.to_le_bytes());put16(r,at+12,flags);put16(r,at+14,next);
}
fn ram(operation:u32)->Vec<u8> {
    let mut r=vec![0;0x8000];desc(&mut r,0,0x1000,16,1,1);desc(&mut r,1,0x2000,512,if operation==0 {3} else {1},2);desc(&mut r,2,0x3000,1,2,0);
    r[0x1000..0x1004].copy_from_slice(&operation.to_le_bytes());r[0x1008..0x1010].copy_from_slice(&1u64.to_le_bytes());r[0x3000]=0xaa;put16(&mut r,0x202,1);r
}
fn memory(r:&mut [u8])->GuestMemory<'_> {GuestMemory::new(vec![RamRegion{gpa:0,bytes:r,writable:true}]).unwrap()}
fn queue(m:&GuestMemory<'_>)->SplitQueue {SplitQueue::new(m,8,0x100,0x200,0x300).unwrap()}
fn read(m:&GuestMemory<'_>,at:u64,n:usize)->Vec<u8> {let mut data=vec![0;n];m.read(at,&mut data).unwrap();data}
struct Disk {data:[u8;2048],ops:u32,flushes:u32,fail:bool}
impl Default for Disk {fn default()->Self {Self{data:[7;2048],ops:0,flushes:0,fail:false}}}
impl BlockBackend for Disk {
    fn capacity_sectors(&self)->u64 {4}
    fn read(&mut self,s:u64,d:&mut [u8])->Result<(),String> {self.ops+=1;if self.fail {return Err("injected disk failure".into());}d.copy_from_slice(&self.data[s as usize*512..s as usize*512+d.len()]);Ok(())}
    fn write(&mut self,s:u64,d:&[u8])->Result<(),String> {self.ops+=1;if self.fail {return Err("injected disk failure".into());}self.data[s as usize*512..s as usize*512+d.len()].copy_from_slice(d);Ok(())}
    fn flush(&mut self)->Result<(),String> {self.flushes+=1;if self.fail {Err("injected flush failure".into())} else {Ok(())}}
}
#[test] fn read_scatter_buffers_status_and_used_lengths() {
    let mut r=ram(0);desc(&mut r,1,0x2000,256,3,2);desc(&mut r,2,0x2100,256,3,3);desc(&mut r,3,0x3000,1,2,0);
    let mut m=memory(&mut r);let mut q=queue(&m);let mut disk=Disk::default();let result=process_one(&mut m,&mut q,&mut disk).unwrap().unwrap();
    assert_eq!(result.status,0);assert_eq!(result.written,513);assert!(result.notify);assert!(result.error.is_none());assert_eq!(disk.ops,1);
    assert_eq!(read(&m,0x2000,512),vec![7;512]);assert_eq!(read(&m,0x3000,1),[0]);assert_eq!(read(&m,0x302,2),1u16.to_le_bytes());
    assert_eq!(read(&m,0x304,8),[0,0,0,0,1,2,0,0]);assert!(process_one(&mut m,&mut q,&mut disk).unwrap().is_none());
}
#[test] fn write_and_suppressed_interrupt_hint() {
    let mut r=ram(1);r[0x2000..0x2200].fill(9);put16(&mut r,0x200,1);
    let mut m=memory(&mut r);let mut q=queue(&m);let mut disk=Disk::default();let c=process_one(&mut m,&mut q,&mut disk).unwrap().unwrap();
    assert_eq!(c.status,0);assert_eq!(c.written,1);assert!(!c.notify);assert_eq!(&disk.data[512..1024],&[9;512]);
}
#[test] fn flush_two_descriptors_and_unused_sector() {
    let mut r=ram(4);desc(&mut r,0,0x1000,16,1,2); // skip data descriptor; sector is unused for FLUSH.
    let mut m=memory(&mut r);let mut q=queue(&m);let mut disk=Disk::default();
    assert_eq!(process_one(&mut m,&mut q,&mut disk).unwrap().unwrap().status,0);assert_eq!(disk.flushes,1);assert_eq!(disk.ops,0);
}
#[test] fn protocol_errors_complete_only_valid_status_without_backend_access() {
    for kind in 0..5 {
        let mut r=ram(0);
        match kind {0=>desc(&mut r,0,0x1000,16,3,1),1=>desc(&mut r,1,0x2000,512,1,2),2=>desc(&mut r,1,0x2000,511,3,2),3=>r[0x1008..0x1010].copy_from_slice(&u64::MAX.to_le_bytes()),_=>r[0x1000..0x1004].copy_from_slice(&8u32.to_le_bytes())}
        let mut m=memory(&mut r);let mut q=queue(&m);let mut disk=Disk::default();let c=process_one(&mut m,&mut q,&mut disk).unwrap().unwrap();
        assert_eq!(c.status,if kind==4 {2} else {1});assert_eq!(c.written,1);assert_eq!(disk.ops,0);assert_eq!(read(&m,0x2000,512),vec![0;512]);
    }
}
#[test] fn malformed_chains_never_write_status_used_or_disk() {
    for kind in 0..8 {
        let mut r=ram(0);
        match kind {0=>desc(&mut r,2,0x3000,1,3,0),1=>desc(&mut r,0,0x1000,16,1,8),2=>desc(&mut r,1,0x2000,512,7,2),3=>desc(&mut r,1,u64::MAX,512,3,2),4=>desc(&mut r,1,0x300,512,3,2),5=>desc(&mut r,2,0x2001,1,2,0),6=>desc(&mut r,2,0x3000,1,0,0),_=>put16(&mut r,0x202,9)}
        let original=r.clone();let mut m=memory(&mut r);let mut q=queue(&m);let mut disk=Disk::default();assert!(process_one(&mut m,&mut q,&mut disk).is_err());
        assert_eq!(disk.ops,0);assert_eq!(read(&m,0,original.len()),original);
    }
}
#[test] fn backend_errors_report_ioerr_without_publishing_read_payload() {
    let mut r=ram(0);let mut m=memory(&mut r);let mut q=queue(&m);let mut disk=Disk{fail:true,..Default::default()};
    let c=process_one(&mut m,&mut q,&mut disk).unwrap().unwrap();assert_eq!(c.status,1);assert!(c.error.unwrap().contains("injected"));assert_eq!(read(&m,0x2000,512),vec![0;512]);
}
#[test] fn queue_configuration_and_owned_index_tampering_refused() {
    let mut r=ram(0);let mut m=memory(&mut r);
    for (size,d,a,u) in [(0,0x100,0x200,0x300),(3,0x100,0x200,0x300),(512,0x100,0x200,0x300),(8,0x101,0x200,0x300),(8,0x100,0x102,0x300),(8,u64::MAX-15,0x200,0x300)] {assert!(SplitQueue::new(&m,size,d,a,u).is_err());}
    let q=queue(&m);m.write(0x302,&1u16.to_le_bytes()).unwrap();assert!(q.peek(&m).is_err());
}
#[test] fn requests_crossing_a_ram_hole_fail_before_status_publication() {
    let mut r=ram(0);desc(&mut r,1,0x7f00,512,3,2);let mut m=memory(&mut r);let mut q=queue(&m);
    assert!(process_one(&mut m,&mut q,&mut Disk::default()).is_err());assert_eq!(read(&m,0x3000,1),[0xaa]);
}
#[test] fn actual_cow_backend_write_flush_reopen_and_base_preservation() {
    let root=std::env::temp_dir().join(format!("nexus-virtio-block-{}",std::process::id()));std::fs::create_dir(&root).unwrap();
    let raw=root.join("raw");let base=root.join("base.zlag");let cow=root.join("data.cow");std::fs::write(&raw,[7;2048]).unwrap();image::import_raw(&raw,&base).unwrap();let original=std::fs::read(&base).unwrap();
    {
        let mut disk=Overlay::create(&base,&cow).unwrap();let mut r=ram(1);r[0x2000..0x2200].fill(19);let mut m=memory(&mut r);let mut q=queue(&m);
        assert_eq!(process_one(&mut m,&mut q,&mut disk).unwrap().unwrap().status,0);
        let mut f=ram(4);desc(&mut f,0,0x1000,16,1,2);let mut fm=memory(&mut f);let mut fq=queue(&fm);assert_eq!(process_one(&mut fm,&mut fq,&mut disk).unwrap().unwrap().status,0);
    }
    {let mut disk=Overlay::open(&base,&cow).unwrap();let mut r=ram(0);let mut m=memory(&mut r);let mut q=queue(&m);assert_eq!(process_one(&mut m,&mut q,&mut disk).unwrap().unwrap().status,0);assert_eq!(read(&m,0x2000,512),vec![19;512]);}
    assert_eq!(std::fs::read(&base).unwrap(),original);std::fs::remove_dir_all(&root).unwrap();
}

#[test] fn over_limit_transfer_is_ioerr_before_backend_io() {
    let mut r=ram(0);r.resize(0x24000,0);desc(&mut r,1,0x2000,128*1024+512,3,2);desc(&mut r,2,0x23000,1,2,0);
    let mut m=memory(&mut r);let mut q=queue(&m);let mut disk=Disk::default();let c=process_one(&mut m,&mut q,&mut disk).unwrap().unwrap();
    assert_eq!(c.status,1);assert!(c.error.unwrap().contains("transfer limit"));assert_eq!(disk.ops,0);
}
#[test] fn device_write_to_readonly_ram_is_fatal_without_side_effects() {
    let mut r=ram(0);let (a,rest)=r.split_at_mut(0x2000);let (b,c)=rest.split_at_mut(512);
    let mut m=GuestMemory::new(vec![RamRegion{gpa:0,bytes:a,writable:true},RamRegion{gpa:0x2000,bytes:b,writable:false},RamRegion{gpa:0x2200,bytes:c,writable:true}]).unwrap();
    let mut q=queue(&m);let mut disk=Disk::default();assert!(process_one(&mut m,&mut q,&mut disk).is_err());assert_eq!(disk.ops,0);assert_eq!(read(&m,0x3000,1),[0xaa]);
}
#[test] fn readonly_backend_requires_verified_payload_and_refuses_writes() {
    use crate::virtio_block::ReadOnlyDisk;
    let root=std::env::temp_dir().join(format!("nexus-virtio-ro-{}",std::process::id()));std::fs::create_dir(&root).unwrap();
    let raw=root.join("raw");let base=root.join("base.zlag");std::fs::write(&raw,[7;2048]).unwrap();image::import_raw(&raw,&base).unwrap();let original=std::fs::read(&base).unwrap();
    {
        let mut disk=ReadOnlyDisk::open(&base).unwrap();let mut r=ram(1);let mut m=memory(&mut r);let mut q=queue(&m);
        let c=process_one(&mut m,&mut q,&mut disk).unwrap().unwrap();assert_eq!(c.status,1);assert!(c.error.unwrap().contains("Read-only"));
    }
    assert_eq!(std::fs::read(&base).unwrap(),original);let mut corrupt=original;corrupt[100]^=1;std::fs::write(&base,corrupt).unwrap();assert!(ReadOnlyDisk::open(&base).is_err());std::fs::remove_dir_all(&root).unwrap();
}
