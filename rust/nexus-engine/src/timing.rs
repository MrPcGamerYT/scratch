//! Host monotonic checkpoints, not guest wall clock, FPS or Android readiness.
#[derive(Default)]
pub struct BootTrace {
    pub header_validated_ms:Option<u64>,pub vm_ready_ms:Option<u64>,pub vcpu_run_requested_ms:Option<u64>,
    pub linux_banner_ms:Option<u64>,pub stopped_ms:Option<u64>,pub exits:u64,pub pit_programs:u64,pub pit_accesses:u64,
    pub serial_bytes:u64,pub stop_reason:String,matched:usize,
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
        if self.linux_banner_ms.is_some() {return;}
        for &byte in bytes {
            if byte==MARKER[self.matched] {self.matched+=1;}
            else {self.matched=usize::from(byte==MARKER[0]);}
            if self.matched==MARKER.len() {self.linux_banner_ms=Some(elapsed_ms);self.matched=0;break;}
        }
    }
    pub fn json(&self)->String {
        format!(concat!("{{\n  \"schema\":1,\n  \"clock\":\"host_monotonic\",\n  \"origin\":\"engine_main_including_input_load\",\n",
            "  \"android_home_target_ms\":60000,\n  \"android_home_ready_ms\":null,\n  \"total_boot_verified\":false,\n  \"target_met\":null,\n",
            "  \"header_validated_ms\":{},\n  \"vm_ready_ms\":{},\n  \"vcpu_run_requested_ms\":{},\n  \"linux_banner_ms\":{},\n  \"stopped_ms\":{},\n",
            "  \"vcpu_exits\":{},\n  \"pit_channel2_programs\":{},\n  \"pit_accesses\":{},\n  \"serial_bytes\":{},\n  \"stop_reason\":{}\n}}\n"),
            number(self.header_validated_ms),number(self.vm_ready_ms),number(self.vcpu_run_requested_ms),number(self.linux_banner_ms),number(self.stopped_ms),
            self.exits,self.pit_programs,self.pit_accesses,self.serial_bytes,quote(&self.stop_reason))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn banner_across_chunks_is_not_android_readiness() {
        let mut t=BootTrace::default();t.observe_serial(b"noise Li",2);t.observe_serial(b"nux ver",3);t.observe_serial(b"sion 6.6",4);
        assert_eq!(t.linux_banner_ms,Some(4));t.observe_serial(b"Linux version ",10);assert_eq!(t.linux_banner_ms,Some(4));
        assert!(t.json().contains("\"android_home_ready_ms\":null"));assert!(t.json().contains("\"target_met\":null"));
        assert!(t.json().contains("\"total_boot_verified\":false"));
    }
    #[test] fn log_text_cannot_invent_a_successful_total_boot() {
        let mut t=BootTrace::default();t.observe_serial(b"Android Home ready in 1 second",1);
        assert_eq!(t.linux_banner_ms,None);assert!(t.json().contains("\"android_home_ready_ms\":null"));
        t.stop_reason="bad\n\"path\\test".into();assert!(t.json().contains("bad\\u000a\\\"path\\\\test"));
    }
}
