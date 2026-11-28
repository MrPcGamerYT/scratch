//! I2C master controller with a small register interface. Supports a single
//! bus with start/stop/read/write transfer semantics via register commands.
//! Not a full SMBus implementation; provides the core register semantics a
//! guest driver uses for bus transactions.
#[derive(Default)]
pub struct I2cController {
    pub control: u32,
    pub status: u32,
    pub address: u32,
    pub data: u32,
    pub accesses: u64,
    pub transfers: u64,
    busy: bool,
    error: bool,
}
impl I2cController {
    pub fn new() -> Self { Self::default() }

    fn read_reg(&self, offset: u64, size: u8) -> Result<u32, String> {
        let value = match offset {
            0x00 => self.control,
            0x04 => self.status,
            0x08 => self.address,
            0x0c => self.data,
            _ => return Err(format!("I2C read outside register window: offset 0x{offset:x}")),
        };
        match size {
            1 => Ok((value & 0xff) as u32),
            2 => Ok((value & 0xffff) as u32),
            4 => Ok(value),
            _ => Err(format!("I2C read size {size} not supported")),
        }
    }

    fn write_reg(&mut self, offset: u64, size: u8, value: u32) -> Result<(), String> {
        let masked = match size {
            1 => value & 0xff,
            2 => value & 0xffff,
            4 => value,
            _ => return Err(format!("I2C write size {size} not supported")),
        };
        match offset {
            0x00 => {
                // Control: bit0=start, bit1=stop, bit2=read, bit3=write.
                // A transfer is counted only when the control register is
                // written with a non-zero command, not on every register access.
                self.control = masked;
                if masked & 0xf != 0 {
                    self.transfers += 1;
                    self.busy = false;
                    self.error = false;
                    self.status = 0;
                }
            }
            0x08 => self.address = masked & 0x7f,
            0x0c => self.data = masked,
            0x04 => return Err("I2C status register is read-only".into()),
            _ => return Err(format!("I2C write outside register window: offset 0x{offset:x}")),
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
    pub fn irq_asserted(&self) -> bool { self.error || self.busy }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_address_data_roundtrip() {
        let mut i2c = I2cController::new();
        i2c.write(0x00, 4, 0x01).unwrap(); // start
        i2c.write(0x08, 4, 0x50).unwrap();
        i2c.write(0x0c, 4, 0xab).unwrap();
        assert_eq!(i2c.read(0x00, 4).unwrap(), 0x01);
        assert_eq!(i2c.read(0x08, 4).unwrap(), 0x50);
        assert_eq!(i2c.read(0x0c, 4).unwrap(), 0xab);
        assert_eq!(i2c.transfers, 1);
    }

    #[test]
    fn status_is_read_only() {
        let mut i2c = I2cController::new();
        assert!(i2c.write(0x04, 4, 0xff).is_err());
        assert_eq!(i2c.read(0x04, 4).unwrap(), 0);
    }

    #[test]
    fn no_transfer_on_zero_command() {
        let mut i2c = I2cController::new();
        i2c.write(0x00, 4, 0x00).unwrap();
        assert_eq!(i2c.transfers, 0);
    }

    #[test]
    fn unsupported_offsets_and_sizes() {
        let mut i2c = I2cController::new();
        assert!(i2c.read(0x10, 4).is_err());
        assert!(i2c.write(0x10, 4, 0).is_err());
        assert!(i2c.read(0x00, 8).is_err());
        assert!(i2c.write(0x00, 8, 0).is_err());
    }

    #[test]
    fn address_is_masked_to_7bits() {
        let mut i2c = I2cController::new();
        i2c.write(0x08, 4, 0x1ff).unwrap();
        assert_eq!(i2c.address, 0x7f);
    }
}
