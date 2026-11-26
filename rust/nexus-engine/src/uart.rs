//! Polled COM1 subset for early Linux logs. No interrupts, timing, DMA or UART IRQ.
use crate::io::IoExit;
#[derive(Default)]
pub struct EarlyUart {regs:[u8;8],divisor:[u8;2],pub output:Vec<u8>}
impl EarlyUart {
    pub fn access(&mut self,io:IoExit)->Result<Option<u64>,String> {
        if io.port==0x80&&io.write&&io.size==1 {return Ok(None);} // POST delay sink only.
        if io.size!=1 || !(0x3f8..=0x3ff).contains(&io.port) {return Err(format!("Unsupported kernel port 0x{:x}, write={}, size={}",io.port,io.write,io.size));}
        let i=(io.port-0x3f8) as usize;let dlab=self.regs[3]&0x80!=0;
        if io.write {
            let byte=io.rax as u8;
            if dlab&&i<2 {self.divisor[i]=byte;}
            else if i==0 {
                if self.output.len()>=1024*1024 {return Err("Kernel serial output limit exceeded".into());}
                self.output.push(byte);
            } else {self.regs[i]=byte;}
            Ok(None)
        } else {
            let byte=if dlab&&i<2 {self.divisor[i]} else {match i {0=>0,2=>1,5=>0x60,6=>0xb0,_=>self.regs[i]}};
            Ok(Some(io.input_register(u32::from(byte))))
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn io(port:u16,write:bool,value:u64)->IoExit {IoExit{port,size:1,write,rax:value,rip:0,instruction_len:1}}
    #[test] fn divisor_does_not_emit_output() {
        let mut u=EarlyUart::default();u.access(io(0x3fb,true,0x80)).unwrap();u.access(io(0x3f8,true,1)).unwrap();
        assert!(u.output.is_empty());assert_eq!(u.access(io(0x3f8,false,0)).unwrap(),Some(1));
        u.access(io(0x3fb,true,3)).unwrap();u.access(io(0x3f8,true,65)).unwrap();assert_eq!(u.output,b"A");
        assert_eq!(u.access(io(0x3fd,false,0)).unwrap(),Some(0x60));assert_eq!(u.access(io(0x3fa,false,0)).unwrap(),Some(1));
    }
    #[test] fn strict_ports_and_widths() {
        let mut u=EarlyUart::default();assert!(u.access(io(0x70,false,0)).is_err());
        let mut request=io(0x3f8,true,0);request.size=4;assert!(u.access(request).is_err());
        u.access(io(0x80,true,0)).unwrap();assert!(u.output.is_empty());
    }
}
