use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EepromState {
    Idle,
    Command,
    ReadWait,
    Read,
}

#[derive(Clone)]
pub struct Eeprom {
    pub data: Box<[u8; 8192]>,
    pub state: EepromState,
    pub command: u128,
    pub bit_count: usize,
    pub read_buffer: u64,
    pub address_bits: usize,
    pub dirty: bool,
}

impl Eeprom {
    pub fn new() -> Self {
        Self {
            data: vec![0xFF; 8192].into_boxed_slice().try_into().unwrap(),
            state: EepromState::Idle,
            command: 0,
            bit_count: 0,
            read_buffer: 0,
            address_bits: 6,
            dirty: false,
        }
    }

    pub fn notify_dma_transfer_size(&mut self, count: u32) {
        self.address_bits = match count {
            9 | 73 => 6,
            17 | 81 => 14,
            _ => self.address_bits,
        };
        if trace_enabled() {
            eprintln!(
                "[eeprom] dma-count={} address_bits={}",
                count, self.address_bits
            );
        }
    }

    pub fn write_halfword(&mut self, value: u16) {
        let bit = (value & 1) as u128;
        match self.state {
            EepromState::Idle => {
                if bit == 1 {
                    self.state = EepromState::Command;
                    self.bit_count = 1;
                    self.command = 1;
                }
            }
            EepromState::Command => {
                self.command = (self.command << 1) | bit;
                self.bit_count += 1;

                let read_bits = self.address_bits + 3;
                let write_bits = self.address_bits + 67;

                if self.bit_count == read_bits {
                    let op = ((self.command >> (self.address_bits + 1)) & 0x3) as u8;
                    if op == 0x3 {
                        let addr = self.decode_address(self.command >> 1);
                        if trace_enabled() {
                            eprintln!(
                                "[eeprom] read addr_bits={} raw={:X} addr={:03X} data={:016X}",
                                self.address_bits,
                                self.command,
                                addr,
                                self.peek_block(addr)
                            );
                        }
                        self.prepare_read(addr);
                        return;
                    }
                }

                if self.bit_count == write_bits {
                    let op = ((self.command >> (self.address_bits + 65)) & 0x3) as u8;
                    if op == 0x2 {
                        let addr = self.decode_address(self.command >> 65);
                        let data = ((self.command >> 1) & 0xFFFF_FFFF_FFFF_FFFF) as u64;
                        if trace_enabled() {
                            eprintln!(
                                "[eeprom] write addr_bits={} raw={:X} addr={:03X} data={:016X}",
                                self.address_bits, self.command, addr, data
                            );
                        }
                        self.write_block(addr, data);
                    }

                    self.state = EepromState::Idle;
                    self.bit_count = 0;
                    self.command = 0;
                }
            }
            _ => {}
        }
    }

    pub fn read_halfword(&mut self) -> u16 {
        match self.state {
            EepromState::ReadWait => {
                self.bit_count += 1;
                if self.bit_count >= 4 {
                    self.state = EepromState::Read;
                    self.bit_count = 0;
                }
                0
            }
            EepromState::Read => {
                let bit = (self.read_buffer >> (63 - self.bit_count)) & 1;
                self.bit_count += 1;
                if self.bit_count == 64 {
                    self.state = EepromState::Idle;
                }
                bit as u16
            }
            _ => 1,
        }
    }

    fn decode_address(&self, value: u128) -> usize {
        let raw = match self.address_bits {
            14 => (value & 0x3FFF) as usize,
            _ => (value & 0x003F) as usize,
        };
        raw & 0x03FF
    }

    fn prepare_read(&mut self, addr: usize) {
        self.read_buffer = self.peek_block(addr);
        self.state = EepromState::ReadWait;
        self.bit_count = 0;
        self.command = 0;
    }

    fn peek_block(&self, addr: usize) -> u64 {
        let offset = addr * 8;
        let mut read_buffer = 0;
        for i in 0..8 {
            if offset + i < self.data.len() {
                read_buffer |= (self.data[offset + i] as u64) << ((7 - i) * 8);
            }
        }
        read_buffer
    }

    fn write_block(&mut self, addr: usize, data: u64) {
        let offset = addr * 8;
        for i in 0..8 {
            if offset + i < self.data.len() {
                self.data[offset + i] = ((data >> ((7 - i) * 8)) & 0xFF) as u8;
            }
        }
        self.dirty = true;
    }
}

fn trace_enabled() -> bool {
    std::env::var_os("VIBE_TRACE_EEPROM").is_some()
}

impl Default for Eeprom {
    fn default() -> Self {
        Self::new()
    }
}
