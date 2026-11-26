use crate::{virtio_mmio::BlockMmio,virtio_block::BlockBackend,virtio_block_tests::{ram,desc,put16,memory,read}};
fn write(d:&mut BlockMmio,offset:u64,value:u32) {d.write(offset,4,value).unwrap();}
fn negotiate(d:&mut BlockMmio,flush:bool) {
    write(d,0x70,1);write(d,0x70,3);write(d,0x24,0);write(d,0x20,if flush {512} else {0});
    write(d,0x24,1);write(d,0x20,1);write(d,0x70,11);
}
fn ready(d:&mut BlockMmio,flush:bool) {
    negotiate(d,flush);write(d,0x30,0);write(d,0x38,8);write(d,0x80,0x100);write(d,0x90,0x200);write(d,0xa0,0x300);write(d,0x44,1);write(d,0x70,15);
}
#[derive(Default)]
struct Disk {reads:u32,writes:u32,flushes:u32,fail:bool}
impl BlockBackend for Disk {
    fn capacity_sectors(&self)->u64 {4}
    fn read(&mut self,_s:u64,d:&mut [u8])->Result<(),String> {self.reads+=1;if self.fail {return Err("injected read error".into());}d.fill(7);Ok(())}
    fn write(&mut self,_s:u64,_d:&[u8])->Result<(),String> {self.writes+=1;Ok(())}
    fn flush(&mut self)->Result<(),String> {self.flushes+=1;Ok(())}
}
#[test] fn identity_selectors_config_byte_reads_and_access_shapes() {
    let mut d=BlockMmio::new(0x123456789,false).unwrap();
    for (offset,value) in [(0,0x74726976),(4,2),(8,2),(0x10,512),(0x34,256),(0xfc,0)] {assert_eq!(d.read(offset,4).unwrap(),value);}
    write(&mut d,0x14,1);assert_eq!(d.read(0x10,4).unwrap(),1);write(&mut d,0x14,u32::MAX);assert_eq!(d.read(0x10,4).unwrap(),0);
    assert_eq!(d.read(0x100,4).unwrap(),0x23456789);assert_eq!(d.read(0x104,4).unwrap(),1);assert_eq!(d.read(0x101,1).unwrap(),0x67);
    assert!(d.read(0x107,2).is_err());assert!(d.read(0,1).is_err());assert!(d.read(1,4).is_err());assert!(d.write(0x100,4,0).is_err());assert!(d.write(0x14,2,0).is_err());
    write(&mut d,0x30,1);assert_eq!(d.read(0x34,4).unwrap(),0);assert_eq!(d.read(0x44,4).unwrap(),0);
    assert!(BlockMmio::new(0,false).is_err());assert!(BlockMmio::new(u64::MAX,false).is_err());
}
#[test] fn invalid_negotiation_clears_features_ok_and_reset_unlocks_features() {
    for kind in 0..3 {
        let mut d=BlockMmio::new(4,false).unwrap();assert!(d.write(0x20,4,1).is_err());write(&mut d,0x70,1);write(&mut d,0x70,3);
        if kind!=0 {write(&mut d,0x24,1);write(&mut d,0x20,1);}
        write(&mut d,0x24,if kind==2 {2} else {0});write(&mut d,0x20,if kind==0 {512} else {1});write(&mut d,0x70,11);
        assert_eq!(d.read(0x70,4).unwrap(),3);assert!(d.write(0x44,4,1).is_err());write(&mut d,0x70,0);negotiate(&mut d,true);
        assert_eq!(d.read(0x70,4).unwrap(),11);assert!(d.write(0x20,4,0).is_err());assert!(d.write(0x70,4,3).is_err());
    }
}
#[test] fn lifecycle_requires_features_and_ready_and_protects_live_queue() {
    let mut d=BlockMmio::new(4,false).unwrap();assert!(d.write(0x50,4,0).is_err());assert!(d.write(0x70,4,2).is_err());
    ready(&mut d,true);assert!(d.write(0x80,4,0).is_err());assert!(d.write(0x50,4,1).is_err());write(&mut d,0x44,1);
    write(&mut d,0x50,0);assert!(d.work_pending());write(&mut d,0x44,0);assert!(!d.work_pending());assert_eq!(d.read(0x44,4).unwrap(),0);
    write(&mut d,0x70,0);assert_eq!(d.read(0x70,4).unwrap(),0);assert_eq!(d.read(0x44,4).unwrap(),0);assert!(!d.irq_asserted());assert!(d.write(0x50,4,0).is_err());
}
#[test] fn read_completion_irq_latch_ack_and_no_repeat_io() {
    let mut d=BlockMmio::new(4,false).unwrap();ready(&mut d,true);let mut r=ram(0);let mut m=memory(&mut r);let mut disk=Disk::default();
    assert!(d.service(&mut m,&mut disk,1).unwrap().is_empty());write(&mut d,0x50,0);let c=d.service(&mut m,&mut disk,1).unwrap();
    assert_eq!(c.len(),1);assert_eq!(c[0].status,0);assert_eq!(read(&m,0x2000,512),vec![7;512]);assert_eq!(disk.reads,1);
    assert_eq!(d.read(0x60,4).unwrap(),1);write(&mut d,0x64,2);assert!(d.irq_asserted());write(&mut d,0x64,1);assert!(!d.irq_asserted());
    assert!(d.service(&mut m,&mut disk,1).unwrap().is_empty());assert_eq!(disk.reads,1);
}
#[test] fn service_budget_retains_pending_work_and_reasserts_after_ack() {
    let mut d=BlockMmio::new(4,false).unwrap();ready(&mut d,true);let mut r=ram(0);
    desc(&mut r,3,0x1100,16,1,4);desc(&mut r,4,0x2200,512,3,5);desc(&mut r,5,0x3100,1,2,0);put16(&mut r,0x202,2);put16(&mut r,0x206,3);
    let mut m=memory(&mut r);let mut disk=Disk::default();write(&mut d,0x50,0);assert_eq!(d.service(&mut m,&mut disk,1).unwrap().len(),1);assert!(d.work_pending());
    write(&mut d,0x64,1);assert_eq!(d.service(&mut m,&mut disk,1).unwrap().len(),1);assert!(!d.work_pending());assert!(d.irq_asserted());assert_eq!(disk.reads,2);
    assert!(d.service(&mut m,&mut disk,0).is_err());assert!(d.service(&mut m,&mut disk,257).is_err());
}
#[test] fn fatal_queue_fault_requires_reset_without_disk_or_status_write() {
    let mut d=BlockMmio::new(4,false).unwrap();ready(&mut d,true);let mut r=ram(0);desc(&mut r,2,0x3000,1,3,0);
    let mut m=memory(&mut r);let mut disk=Disk::default();write(&mut d,0x50,0);assert!(d.service(&mut m,&mut disk,8).is_err());
    assert_eq!(d.read(0x70,4).unwrap()&64,64);assert_eq!(d.read(0x60,4).unwrap(),2);assert_eq!(disk.reads,0);assert_eq!(read(&m,0x3000,1),[0xaa]);
    assert!(d.write(0x50,4,0).is_err());write(&mut d,0x64,2);assert!(!d.irq_asserted());write(&mut d,0x70,0);assert_eq!(d.read(0x70,4).unwrap(),0);
}
#[test] fn ordinary_io_error_does_not_poison_device_and_suppression_is_honored() {
    let mut d=BlockMmio::new(4,false).unwrap();ready(&mut d,true);let mut r=ram(0);put16(&mut r,0x200,1);let mut m=memory(&mut r);let mut disk=Disk{fail:true,..Default::default()};
    write(&mut d,0x50,0);assert_eq!(d.service(&mut m,&mut disk,1).unwrap()[0].status,1);assert_eq!(d.read(0x70,4).unwrap(),15);assert!(!d.irq_asserted());
}
#[test] fn readonly_and_flush_feature_policy_enforced_before_backend() {
    let mut d=BlockMmio::new(4,true).unwrap();assert_eq!(d.read(0x10,4).unwrap()&32,32);ready(&mut d,true);
    let mut r=ram(1);let mut m=memory(&mut r);let mut disk=Disk::default();write(&mut d,0x50,0);assert_eq!(d.service(&mut m,&mut disk,1).unwrap()[0].status,1);assert_eq!(disk.writes,0);
    for flush in [false,true] {
        let mut d=BlockMmio::new(4,false).unwrap();ready(&mut d,flush);let mut r=ram(4);desc(&mut r,0,0x1000,16,1,2);let mut m=memory(&mut r);let mut disk=Disk::default();
        write(&mut d,0x50,0);assert_eq!(d.service(&mut m,&mut disk,1).unwrap()[0].status,if flush {0} else {2});assert_eq!(disk.flushes,u32::from(flush));
    }
}
#[test] fn invalid_ring_gpa_is_checked_before_backend_and_demands_reset() {
    let mut d=BlockMmio::new(4,false).unwrap();negotiate(&mut d,true);write(&mut d,0x38,8);write(&mut d,0x80,0);write(&mut d,0x84,0xd0000000);write(&mut d,0x90,0x200);write(&mut d,0xa0,0x300);write(&mut d,0x44,1);write(&mut d,0x70,15);write(&mut d,0x50,0);
    let mut r=ram(0);let mut m=memory(&mut r);let mut disk=Disk::default();assert!(d.service(&mut m,&mut disk,1).is_err());assert_eq!(disk.reads,0);assert_eq!(d.read(0x70,4).unwrap()&64,64);
}
#[test] fn mmio_model_cow_write_flush_reset_reopen_read_preserves_base() {
    use crate::{image,overlay::Overlay};
    let root=std::env::temp_dir().join(format!("nexus-mmio-cow-{}",std::process::id()));std::fs::create_dir(&root).unwrap();
    let raw=root.join("raw");let base=root.join("base.zlag");let cow=root.join("data.cow");std::fs::write(&raw,[7;2048]).unwrap();image::import_raw(&raw,&base).unwrap();let original=std::fs::read(&base).unwrap();
    {
        let mut disk=Overlay::create(&base,&cow).unwrap();let mut d=BlockMmio::new(4,false).unwrap();ready(&mut d,true);
        let mut r=ram(1);r[0x2000..0x2200].fill(21);let mut m=memory(&mut r);write(&mut d,0x50,0);assert_eq!(d.service(&mut m,&mut disk,1).unwrap()[0].status,0);
        write(&mut d,0x70,0);ready(&mut d,true);let mut r=ram(4);desc(&mut r,0,0x1000,16,1,2);let mut m=memory(&mut r);write(&mut d,0x50,0);assert_eq!(d.service(&mut m,&mut disk,1).unwrap()[0].status,0);
    }
    {
        let mut disk=Overlay::open(&base,&cow).unwrap();let mut d=BlockMmio::new(4,false).unwrap();ready(&mut d,true);
        let mut r=ram(0);let mut m=memory(&mut r);write(&mut d,0x50,0);assert_eq!(d.service(&mut m,&mut disk,1).unwrap()[0].status,0);assert_eq!(read(&m,0x2000,512),vec![21;512]);
    }
    assert_eq!(std::fs::read(&base).unwrap(),original);std::fs::remove_dir_all(root).unwrap();
}
