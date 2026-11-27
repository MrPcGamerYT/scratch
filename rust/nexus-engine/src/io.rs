//! Minimal private test devices, not a UART/PCI chipset or Android hardware ABI.
//! All unsupported I/O is rejected; host ports are NEVER passed through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IoExit {
    pub port: u16,
    pub size: u8,
    pub write: bool,
    pub rax: u64,
    pub rip: u64,
    pub instruction_len: u8,
}
impl IoExit {
    /// AMD64 WHV_RUN_VP_EXIT_CONTEXT offsets, checked against the Windows SDK.
    pub fn decode(context: &[u8]) -> Result<Self, String> {
        if context.len() != 224 { return Err("Invalid WHP exit buffer size".into()); }
        let word = |i| u32::from_le_bytes(context[i..i+4].try_into().unwrap());
        if word(0) != 2 { return Err("Not a port-I/O exit".into()); }
        let info = word(68);
        if info & 0x30 != 0 { return Err("String/REP port I/O is not implemented".into()); }
        if info & !0x3f != 0 { return Err("Reserved I/O flags set".into()); }
        let size = ((info >> 1) & 7) as u8;
        if ![1, 2, 4].contains(&size) { return Err("Invalid I/O width".into()); }
        let instruction_len = context[10] & 15;
        if instruction_len == 0 { return Err("Missing I/O instruction length".into()); }
        Ok(Self {
            port: u16::from_le_bytes(context[72..74].try_into().unwrap()),
            size, write: info & 1 != 0,
            rax: u64::from_le_bytes(context[80..88].try_into().unwrap()),
            rip: u64::from_le_bytes(context[32..40].try_into().unwrap()), instruction_len,
        })
    }
    pub fn next_real_mode_rip(self) -> Result<u64, String> {
        crate::cpu::CpuMode::Real16.next_rip(self.rip,self.instruction_len)
    }
    /// IN AL/AX preserves upper bits, IN EAX zero-extends to RAX (x86 rules).
    pub fn input_register(self, value: u32) -> u64 {
        match self.size {
            1 => (self.rax & !0xff) | u64::from(value & 0xff),
            2 => (self.rax & !0xffff) | u64::from(value & 0xffff),
            _ => u64::from(value),
        }
    }
}
#[derive(Default)]
pub struct TestBus { pub console: Vec<u8>, pub latch: u32 }
impl TestBus {
    pub fn access(&mut self, io: IoExit) -> Result<Option<u64>, String> {
        match (io.port, io.write, io.size) {
            (0xe9, true, 1) => {
                if self.console.len() >= 4096 { return Err("Debug console limit exceeded".into()); }
                self.console.push(io.rax as u8); Ok(None)
            }
            (0x504, true, 1 | 2 | 4) => {
                self.latch = (io.rax as u32) & match io.size { 1 => 0xff, 2 => 0xffff, _ => u32::MAX }; Ok(None)
            }
            (0x504, false, 1 | 2 | 4) => Ok(Some(io.input_register(self.latch))),
            _ => Err(format!("Unsupported test port access: port=0x{:x}, write={}, size={}",io.port,io.write,io.size)),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn context(info:u32)->[u8;224] {
        let mut c=[0;224]; c[0]=2;c[10]=3;
        c[68..72].copy_from_slice(&info.to_le_bytes());c[72..74].copy_from_slice(&0x504u16.to_le_bytes());
        c[80..88].copy_from_slice(&0x1122334455667788u64.to_le_bytes()); c
    }
    #[test] fn decode_and_width_rules() {
        let mut io=IoExit::decode(&context(2)).unwrap();
        assert_eq!(io.port,0x504);assert_eq!(io.size,1);assert!(!io.write);
        assert_eq!(io.next_real_mode_rip().unwrap(),3);
        assert_eq!(io.input_register(0xaabbccdd),0x11223344556677dd);
        io.size=2;assert_eq!(io.input_register(0xaabbccdd),0x112233445566ccdd);
        io.size=4;assert_eq!(io.input_register(0xaabbccdd),0xaabbccdd);
        io.rip=u64::MAX;assert!(io.next_real_mode_rip().is_err());
        io.rip=65535;assert!(io.next_real_mode_rip().is_err());
    }
    #[test] fn reject_unsupported_contexts() {
        for info in [0,6,10,2|16,2|32,2|64] { assert!(IoExit::decode(&context(info)).is_err()); }
        assert!(IoExit::decode(&[0;20]).is_err());
        let mut c=context(2);c[0]=8;assert!(IoExit::decode(&c).is_err());
        c=context(2);c[10]=0;assert!(IoExit::decode(&c).is_err());
    }
    #[test] fn bus_is_private_and_bounded() {
        let mut bus=TestBus::default();let mut io=IoExit::decode(&context(9)).unwrap(); // OUT dword
        bus.access(io).unwrap();assert_eq!(bus.latch,0x55667788);
        io.write=false;io.size=1;assert_eq!(bus.access(io).unwrap(),Some(0x1122334455667788));
        io.port=0x3f8;assert!(bus.access(io).is_err()); // Not a real UART.
        io.port=0xe9;io.write=true;
        for _ in 0..4096 {bus.access(io).unwrap();}
        assert!(bus.access(io).is_err());
    }
}
