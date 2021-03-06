const CRC_POLYNOMIAL_NORMAL: u32 = 0x04c11db7;
const CRC_POLYNOMIAL_REVERSED: u32 = 0xedb88320;

pub struct Crc32Context {
    crc: u32,
    // Reversed or Normal
    reversed: bool,
    refin: bool,
    refout: bool,
}

impl Crc32Context {
    pub fn new(crc: u32, reversed: bool, refin: bool, refout: bool) -> Self {
        Crc32Context {
            crc: crc,
            reversed: reversed,
            refin: refin,
            refout: refout,
        }
    }

    fn step_normal(&mut self, byte: u8) {
        if self.refin {
            self.crc ^= (byte.reverse_bits() as u32) << 24;
        } else {
            self.crc ^= (byte as u32) << 24;
        }

        for _ in 0..8 {
            if self.crc & 0x80000000 != 0 {
                self.crc = (self.crc << 1) ^ CRC_POLYNOMIAL_NORMAL;
            } else {
                self.crc <<= 1;
            }
        }
    }

    fn step_reversed(&mut self, byte: u8) {
        self.crc ^= byte as u32;
        for _ in 0..8 {
            if self.crc & 1 == 1 {
                self.crc = (self.crc >> 1) ^ CRC_POLYNOMIAL_REVERSED;
            } else {
                self.crc >>= 1;
            }
        }
    }

    pub fn step(&mut self, byte: u8) {
        if self.reversed {
            self.step_reversed(byte);
        } else {
            self.step_normal(byte);
        }
    }

    pub fn finalize(&self) -> u32 {
        let crc: u32;

        if self.refout {
            crc = self.crc.reverse_bits();
        } else {
            crc = self.crc;
        }
        !crc
    }
}
