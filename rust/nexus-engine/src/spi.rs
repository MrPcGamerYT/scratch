//! SPI master controller. Provides a register interface for configuring the
//! clock, mode, and performing single-byte transfers via FIFO-like registers.
//! Not a full SPI subsystem; covers the core register semantics a guest
//! driver needs for basic bus transactions.
#[derive(Default)]
pub struct SpiController {
    pub control: u32,
    pub status: u32,
    pub clock_divider: u32,
    pub tx: u32,
    pub rx: u32,
    pub accesses: u64,
    pub transfers: u64,
    busy: bool,
}
impl SpiController {
    pub fn new() -> Self { Self::default() }

    fn read_reg(&self, offset: u64, size: u8) -> Result<u32, String> {
        let value = match offset {
            0x00 => self.control,
            0x04 => self.status,
            0x08 => self.clock_divider,
            0x0c => self.tx,
            0x10 => self.rx,
            _ => return Err(format!("SPI read outside register window: offset 0x{offset:x}")),
        };
        match size {
            1 => Ok((value & 0xff) as u32),
            2 => Ok((value & 0xffff) as u32),
            4 => Ok(value),
            _ => Err(format!("SPI read size {size} not supported")),
        }
    }

    fn write_reg(&mut self, offset: u64, size: u8, value: u32) -> Result<(), String> {
        let masked = match size {
            1 => value & 0xff,
            2 => value & 0xffff,
            4 => value,
            _ => return Err(format!("SPI write size {size} not supported")),
        };
        match offset {
            0x00 => {
                // Control: bit0=enable, bit1=master, bit2=mode0/1/2/3 in bits 3:2
                self.control = masked;
            }
            0x08 => self.clock_divider = masked,
            0x0c => {
                self.tx = masked;
                // A write to TX triggers an immediate transfer completion.
                if self.control & 1 != 0 {
                    self.transfers += 1;
                    self.rx = masked; // echo for loopback smoke
                    self.status = 1; // busy done
                    self.busy = false;
                }
            }
            0x04 => return Err("SPI status register is read-only".into()),
            0x10 => return Err("SPI RX register is read-only".into()),
            _ => return Err(format!("SPI write outside register window: offset 0x{offset:x}")),
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
    pub fn work_pending(&self) -> bool { self.busy }
    pub fn irq_asserted(&self) -> bool { self.busy }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enable_and_transfer() {
        let mut spi = SpiController::new();
        spi.write(0x00, 4, 0x01).unwrap(); // enable
        spi.write(0x08, 4, 0x10).unwrap(); // clock divider
        spi.write(0x0c, 4, 0xa5).unwrap(); // TX
        assert_eq!(spi.transfers, 1);
        assert_eq!(spi.read(0x10, 4).unwrap(), 0xa5);
        assert_eq!(spi.read(0x04, 4).unwrap(), 1);
    }

    #[test]
    fn no_transfer_when_disabled() {
        let mut spi = SpiController::new();
        spi.write(0x0c, 4, 0xa5).unwrap();
        assert_eq!(spi.transfers, 0);
    }

    #[test]
    fn read_only_registers() {
        let mut spi = SpiController::new();
        assert!(spi.write(0x04, 4, 0xff).is_err());
        assert!(spi.write(0x10, 4, 0xff).is_err());
    }

    #[test]
    fn unsupported_offsets_and_sizes() {
        let mut spi = SpiController::new();
        assert!(spi.read(0x14, 4).is_err());
        assert!(spi.write(0x14, 4, 0).is_err());
        assert!(spi.read(0x00, 8).is_err());
        assert!(spi.write(0x00, 8, 0).is_err());
    }

    #[test]
    fn clock_divider_and_mode_roundtrip() {
        let mut spi = SpiController::new();
        spi.write(0x00, 4, 0x0f).unwrap(); // enable + mode3
        spi.write(0x08, 4, 0xff).unwrap();
        assert_eq!(spi.read(0x00, 4).unwrap(), 0x0f);
        assert_eq!(spi.read(0x08, 4).unwrap(), 0xff);
    }
}
