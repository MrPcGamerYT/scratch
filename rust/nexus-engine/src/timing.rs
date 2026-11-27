//! Current Android Home goal: approximately 100 seconds end-to-end.
//! Host monotonic checkpoints, not guest wall clock, FPS or Android readiness.
#[derive(Default)]
pub struct BootTrace {
    pub header_validated_ms:Option<u64>,pub vm_ready_ms:Option<u64>,pub vcpu_run_requested_ms:Option<u64>,
    pub android_init_second_stage_ms:Option<u64>,pub linux_banner_ms:Option<u64>,pub linux_userspace_ready_ms:Option<u64>,pub stopped_ms:Option<u64>,pub exits:u64,pub pit_programs:u64,pub pit_accesses:u64,pub pic_accesses:u64,pub pic_initializations:u64,
    pub pit0_programs:u64,pub pit0_edges:u64,pub irq0_injections:u64,pub irq5_injections:u64,pub irq4_injections:u64,pub irq3_injections:u64,pub halt_waits:u64,
    pub serial_bytes:u64,pub stop_reason:String,matched:usize,ready_matched:usize,android_matched:usize,
}
fn number(value:Option<u64>)->String {value.map_or("null".into(),|v|v.to_string())}
fn quote(text:&str)->String {
    let mut result=String::from("\"");
    for ch in text.chars() {match ch {'"'=>result.push_str("\\\""),'\\'=>result.push_str("\\\\"),c if c<'\u{20}'=>result.push_str(&format!("\\u{:04x}",c as u32)),c=>result.push(c)}}
    result.push('"');result
}
impl BootTrace {
    pub fn after_header(elapsed_ms:u64)->Self {Self{header_validated_ms:Some(elapsed_ms),..Self::default()}}

    pub fn observe_serial(&mut self,bytes:&[u8],elapsed_ms:u64) {
        self.serial_bytes+=bytes.len() as u64;
        const MARKER:&[u8]=b"Linux version ";
        const ANDROID_INIT:&[u8]=b"init: init second stage started!";
        const READY:&[u8]=b"NEXUS_USERSPACE_READY";
        for &byte in bytes {
            if self.android_init_second_stage_ms.is_none() {
                if byte==ANDROID_INIT[self.android_matched] {self.android_matched+=1;}
                else {self.android_matched=usize::from(byte==ANDROID_INIT[0]);}
                if self.android_matched==ANDROID_INIT.len() {self.android_init_second_stage_ms=Some(elapsed_ms);self.android_matched=0;}
            }
            if self.linux_banner_ms.is_none() {
                if byte==MARKER[self.matched] {self.matched+=1;}
                else {self.matched=usize::from(byte==MARKER[0]);}
                if self.matched==MARKER.len() {self.linux_banner_ms=Some(elapsed_ms);self.matched=0;}
            }
            if self.linux_userspace_ready_ms.is_none() {
                if byte==READY[self.ready_matched] {self.ready_matched+=1;}
                else {self.ready_matched=usize::from(byte==READY[0]);}
                if self.ready_matched==READY.len() {self.linux_userspace_ready_ms=Some(elapsed_ms);self.ready_matched=0;}
            }
        }
    }
    pub fn json(&self)->String {
        format!(concat!("{{\n  \"schema\":1,\n  \"clock\":\"host_monotonic\",\n  \"origin\":\"engine_main_including_input_load\",\n",
            "  \"android_home_target_ms\":100000,\n  \"android_home_ready_ms\":null,\n  \"total_boot_verified\":false,\n  \"target_met\":null,\n",
            "  \"header_validated_ms\":{},\n  \"vm_ready_ms\":{},\n  \"vcpu_run_requested_ms\":{},\n  \"linux_banner_ms\":{},\n  \"linux_userspace_ready_ms\":{},\n  \"android_init_second_stage_ms\":{},\n  \"stopped_ms\":{},\n",
            "  \"vcpu_exits\":{},\n  \"pit_channel2_programs\":{},\n  \"pit_accesses\":{},\n  \"pic_accesses\":{},\n  \"pic_initializations\":{},\n  \"pit0_programs\":{},\n  \"pit0_edges\":{},\n  \"irq0_injections\":{},\n  \"irq3_injections\":{},\n  \"irq4_injections\":{},\n  \"irq5_injections\":{},\n  \"halt_waits\":{},\n  \"serial_bytes\":{},\n  \"stop_reason\":{}\n}}\n"),
            number(self.header_validated_ms),number(self.vm_ready_ms),number(self.vcpu_run_requested_ms),number(self.linux_banner_ms),number(self.linux_userspace_ready_ms),number(self.android_init_second_stage_ms),number(self.stopped_ms),
            self.exits,self.pit_programs,self.pit_accesses,self.pic_accesses,self.pic_initializations,self.pit0_programs,self.pit0_edges,self.irq0_injections,self.irq3_injections,self.irq4_injections,self.irq5_injections,self.halt_waits,self.serial_bytes,quote(&self.stop_reason))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn banner_across_chunks_is_not_android_readiness() {
        let mut t=BootTrace::default();t.observe_serial(b"noise Li",2);t.observe_serial(b"nux ver",3);t.observe_serial(b"sion 6.6",4);
        assert_eq!(t.linux_banner_ms,Some(4));t.observe_serial(b"Linux version ",10);assert_eq!(t.linux_banner_ms,Some(4));
        assert!(t.json().contains("\"android_home_target_ms\":100000"));
        assert!(t.json().contains("\"android_home_ready_ms\":null"));assert!(t.json().contains("\"target_met\":null"));
        assert!(t.json().contains("\"total_boot_verified\":false"));
    }
    #[test] fn diagnostic_userspace_marker_is_not_android_home() {
        let mut t=BootTrace::default();t.observe_serial(b"Linux version 6.6",1);
        t.observe_serial(b"NEXUS_USERSPACE_",2);t.observe_serial(b"READY",3);
        assert_eq!(t.linux_userspace_ready_ms,Some(3));
        t.observe_serial(b"NEXUS_USERSPACE_READY",4);assert_eq!(t.linux_userspace_ready_ms,Some(3));
        assert!(t.json().contains("\"android_home_target_ms\":100000"));
        assert!(t.json().contains("\"android_home_ready_ms\":null"));
        assert!(t.json().contains("\"total_boot_verified\":false"));
    }
    #[test] fn android_second_stage_is_neither_framework_nor_home() {
        let mut t=BootTrace::default();t.observe_serial(b"init: init second ",4);t.observe_serial(b"stage started!",5);
        assert_eq!(t.android_init_second_stage_ms,Some(5));assert_eq!(t.linux_userspace_ready_ms,None);
        assert!(t.json().contains("\"android_home_ready_ms\":null"));assert!(t.json().contains("\"total_boot_verified\":false"));
    }
    #[test] fn log_text_cannot_invent_a_successful_total_boot() {
        let mut t=BootTrace::default();t.observe_serial(b"Android Home ready in 1 second",1);
        assert_eq!(t.linux_banner_ms,None);assert!(t.json().contains("\"android_home_ready_ms\":null"));
        t.stop_reason="bad\n\"path\\test".into();assert!(t.json().contains("bad\\u000a\\\"path\\\\test"));
    }
}
