pub trait Interface {
    fn tx_space(&self) -> u16;
    fn tx_start(&self);
    fn tx_finished(&self);
    fn tx_byte(&self, port: u8, byte:u8);
}
