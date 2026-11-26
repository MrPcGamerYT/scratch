//! Non-FIFO COM1 subset: immediate TX, THRE IRQ4, fixed modem pins and probe loopback.
//! One-byte host console input, no DMA, baud timing or FIFO emulation. Host serial ports never used.
use crate::io::IoExit;
#[derive(Default)]
pub struct EarlyUart {regs:[u8;8],divisor:[u8;2],tx_pending:bool,rx:Option<u8>,overrun:bool,modem_delta:u8,pub output:Vec<u8>}
impl EarlyUart {
    pub fn can_receive(&self)->bool {self.rx.is_none()&&self.regs[4]&0x10==0}
    pub fn receive(&mut self,byte:u8)->Result<(),String> {
        if !self.can_receive() {return Err("UART receiver busy or in loopback".into());}self.rx=Some(byte);Ok(())
    }
    fn modem(&self)->u8 {
        let m=self.regs[4];
        if m&0x10==0 {0xb0} else {((m&1)<<5)|((m&2)<<3)|((m&4)<<4)|((m&8)<<4)}
    }
    fn interrupt_id(&self)->u8 {
        let ier=self.regs[1];
        if ier&4!=0&&self.overrun {6}
        else if ier&1!=0&&self.rx.is_some() {4}
        else if ier&2!=0&&self.tx_pending {2}
        else if ier&8!=0&&self.modem_delta!=0 {0}
        else {1}
    }
    pub fn irq_asserted(&self)->bool {self.regs[4]&8!=0&&self.interrupt_id()!=1}
    pub fn access(&mut self,io:IoExit)->Result<Option<u64>,String> {
        if io.port==0x80&&io.write&&io.size==1 {return Ok(None);} // POST delay sink only.
        if io.size!=1 || !(0x3f8..=0x3ff).contains(&io.port) {return Err(format!("Unsupported kernel port 0x{:x}, write={}, size={}",io.port,io.write,io.size));}
        let i=(io.port-0x3f8) as usize;let dlab=self.regs[3]&0x80!=0;
        if io.write {
            let byte=io.rax as u8;
            if dlab&&i<2 {self.divisor[i]=byte;}
            else if i==0 {
                if self.regs[4]&0x10!=0 {
                    if self.rx.is_some() {self.overrun=true;}self.rx=Some(byte);
                } else {
                    if self.output.len()>=1024*1024 {return Err("Kernel serial output limit exceeded".into());}
                    self.output.push(byte);
                }
                self.tx_pending=true;
            } else if i==1 {
                if byte&2!=0&&self.regs[1]&2==0 {self.tx_pending=true;}
                self.regs[1]=byte&15;
            } else if i==2 {
                if byte&2!=0 {self.rx=None;self.overrun=false;}
            } else if i==4 {
                let old=self.modem();self.regs[4]=byte;let new=self.modem();
                self.modem_delta|=((old^new)>>4)&0x0b;
                if old&0x40!=0&&new&0x40==0 {self.modem_delta|=4;}
            } else if i!=5&&i!=6 {self.regs[i]=byte;}
            Ok(None)
        } else {
            let byte=if dlab&&i<2 {self.divisor[i]} else {match i {
                0=>self.rx.take().unwrap_or(0),
                2=>{let id=self.interrupt_id();if id==2 {self.tx_pending=false;}id},
                5=>{let value=0x60|u8::from(self.rx.is_some())|(u8::from(self.overrun)<<1);self.overrun=false;value},
                6=>{let value=self.modem()|self.modem_delta;self.modem_delta=0;value},
                _=>self.regs[i],
            }};
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
    #[test] fn thre_interrupt_enable_ack_rearm_and_out2_gate() {
        let mut u=EarlyUart::default();u.access(io(0x3f9,true,2)).unwrap();assert!(!u.irq_asserted());
        u.access(io(0x3fc,true,8)).unwrap();assert!(u.irq_asserted());
        assert_eq!(u.access(io(0x3fa,false,0)).unwrap(),Some(2));assert!(!u.irq_asserted());
        u.access(io(0x3f8,true,65)).unwrap();assert!(u.irq_asserted());assert_eq!(u.output,b"A");
        u.access(io(0x3f9,true,0)).unwrap();assert!(!u.irq_asserted());
        u.access(io(0x3f9,true,2)).unwrap();assert!(u.irq_asserted());
        u.access(io(0x3fb,true,0x80)).unwrap();u.access(io(0x3f9,true,0)).unwrap();
        assert!(u.irq_asserted()); // Divisor high byte is not IER.
    }
    #[test] fn probe_loopback_modem_rx_priority_and_no_output_leak() {
        let mut u=EarlyUart::default();u.access(io(0x3fc,true,0x1a)).unwrap();
        assert_eq!(u.access(io(0x3fe,false,0)).unwrap().unwrap()&0xf0,0x90);
        u.access(io(0x3f9,true,3)).unwrap();u.access(io(0x3f8,true,0xae)).unwrap();
        assert!(u.output.is_empty());assert_eq!(u.access(io(0x3fa,false,0)).unwrap(),Some(4));
        assert_eq!(u.access(io(0x3f8,false,0)).unwrap(),Some(0xae));
        assert_eq!(u.access(io(0x3fa,false,0)).unwrap(),Some(2));
        assert_eq!(u.access(io(0x3fa,false,0)).unwrap(),Some(1));
    }
    #[test] fn external_console_rx_irq_and_consume() {
        let mut u=EarlyUart::default();u.access(io(0x3fc,true,8)).unwrap();u.access(io(0x3f9,true,1)).unwrap();
        u.receive(b'a').unwrap();assert!(u.irq_asserted());assert!(!u.can_receive());assert!(u.receive(b'b').is_err());
        assert_eq!(u.access(io(0x3fa,false,0)).unwrap(),Some(4));assert_eq!(u.access(io(0x3f8,false,0)).unwrap(),Some(97));
        assert!(!u.irq_asserted());assert!(u.can_receive());u.receive(b'b').unwrap();assert!(u.irq_asserted());
    }
    #[test] fn strict_ports_and_widths() {
        let mut u=EarlyUart::default();assert!(u.access(io(0x70,false,0)).is_err());
        let mut request=io(0x3f8,true,0);request.size=4;assert!(u.access(request).is_err());
        u.access(io(0x80,true,0)).unwrap();assert!(u.output.is_empty());
    }
}
