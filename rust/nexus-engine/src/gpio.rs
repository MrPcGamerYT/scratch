//! General-purpose I/O controller. 32 pins with per-pin direction, output,
//! input, and interrupt-status registers. MMIO-mapped; not a full gpiolib
//! compatible controller, but provides the register semantics a guest driver
//! needs for basic pin control.
#[derive(Default)]
pub struct GpioController {
    pub direction: u32,
    pub output: u32,
    pub input: u32,
    pub status: u32,
    pub accesses: u64,
}
impl GpioController {
    pub fn new() -> Self { Self::default() }

    fn read_reg(&self, offset: u64, size: u8) -> Result<u32, String> {
        match offset {
            0x00 => Ok(self.direction),
            0x04 => Ok(self.output),
            0x08 => Ok(self.input),
            0x0c => Ok(self.status),
            _ => Err(format!("GPIO read outside register window: offset 0x{offset:x}")),
        }
        .map(|v| match size {
            1 => (v & 0xff) as u32,
            2 => (v & 0xffff) as u32,
            4 => v,
            _ => return Err(format!("GPIO read size {size} not supported")),
        })
    }

    fn write_reg(&mut self, offset: u64, size: u8, value: u32) -> Result<(), String> {
        let masked = match size {
            1 => value & 0xff,
            2 => value & 0xffff,
            4 => value,
            _ => return Err(format!("GPIO write size {size} not supported")),
        };
        match offset {
            0x00 => self.direction = masked,
            0x04 => self.output = masked,
            0x0c => self.status = masked,
            0x08 => return Err("GPIO input register is read-only".into()),
            _ => return Err(format!("GPIO write outside register window: offset 0x{offset:x}")),
        }
        self.accesses += 1;
        Ok(())
    }

    pub fn read(&self, offset: u64, size: u8) -> Result<u32, String> {
        self.read_reg(offset, size)
    }
    pub fn write(&mut self, offset: u64, size: u8, value: u32) -> Result<(), String> {
        self.write_reg(offset, size, value)
    }
    pub fn work_pending(&self) -> bool { false }
    pub fn irq_asserted(&self) -> bool { self.status != 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direction_output_input_roundtrip() {
        let mut g = GpioController::new();
        g.write(0x00, 4, 0xffffffff).unwrap(); // all outputs
        assert_eq!(g.read(0x00, 4).unwrap(), 0xffffffff);
        g.write(0x04, 4, 0xdeadbeef).unwrap();
        assert_eq!(g.read(0x04, 4).unwrap(), 0xdeadbeef);
        // Input register reflects whatever the "pins" read — default 0.
        assert_eq!(g.read(0x08, 4).unwrap(), 0);
    }

    #[test]
    fn input_is_read_only() {
        let mut g = GpioController::new();
        assert!(g.write(0x08, 4, 0xff).is_err());
    }

    #[test]
    fn byte_and_word_access_masks() {
        let mut g = GpioController::new();
        g.write(0x00, 1, 0xab).unwrap();
        assert_eq!(g.direction, 0xab);
        g.write(0x04, 2, 0x1234).unwrap();
        assert_eq!(g.output, 0x1234);
        assert_eq!(g.read(0x00, 1).unwrap(), 0xab);
        assert_eq!(g.read(0x04, 2).unwrap(), 0x1234);
    }

    #[test]
    fn status_irq_and_unsupported_offsets() {
        let mut g = GpioController::new();
        g.write(0x0c, 4, 0x42).unwrap();
        assert_eq!(g.read(0x0c, 4).unwrap(), 0x42);
        assert!(g.irq_asserted());
        assert!(!GpioController::new().irq_asserted());
        assert!(g.read(0x10, 4).is_err());
        assert!(g.write(0x10, 4, 0).is_err());
        assert!(g.read(0x00, 8).is_err());
    }
}
