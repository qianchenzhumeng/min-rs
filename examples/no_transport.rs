extern crate min_rs as min;
use std::cell::RefCell;
use log::LevelFilter;
use env_logger;

struct Uart {
    tx_space_avaliable: u16,
    rx_buf: RefCell<[u8; 255]>,
    rx_buf_index: RefCell<u8>,
    loopback: bool,
}

impl Uart {
    fn open(&self) {
        println!("Open uart.");
    }

    fn close(&self) {
        println!("Close uart.");
    }

    fn available_for_write(&self) -> u16 {
        self.tx_space_avaliable
    }

    fn tx(&self, byte: u8) {
        print!("0x{:02x} ", byte);
        if self.loopback {
            let mut rx_buf = self.rx_buf.borrow_mut();
            let mut rx_buf_index = self.rx_buf_index.borrow_mut();
            rx_buf[*rx_buf_index as usize] = byte;
            *rx_buf_index += 1;
        }
    }

    fn get_rx_data_len(&self) -> u8 {
        *self.rx_buf_index.borrow()
    }
}

fn tx_start(uart: &Uart) {
    print!("[ ");
    let mut rx_buf_index = uart.rx_buf_index.borrow_mut();
    *rx_buf_index = 0;
}

fn tx_finished(_: &Uart) {
    println!("]");
}
fn tx_space(uart: &Uart) -> u16 {
    uart.available_for_write()
}

fn tx_byte(uart: &Uart, _port: u8, byte: u8) {
    uart.tx(byte);
}

fn rx_byte(min: &mut min::Context<Uart>, buf: &[u8], buf_len: u32) {
    min.poll(buf, buf_len);
}

fn main() {
    log::set_max_level(LevelFilter::Trace);
    env_logger::init();

    let id: u8 = 0;
    let tx_data: [u8; 8] = [0xaa, 0xaa, 0xaa, 0, 0, 0, 0, 1];
    let rx_data: [u8; 255] = [0; 255];
    let uart = Uart{
        tx_space_avaliable: 128,
        rx_buf: RefCell::new(rx_data),
        rx_buf_index: RefCell::new(0),
        loopback: true,
    };
    let mut min = min::Context::new(
        String::from("min"),
        &uart,
        0,
        false,
        tx_start,
        tx_finished,
        tx_space,
        tx_byte,
    );

    uart.open();

    print!("The data to be sent: [");
    for item in tx_data {
        print!(" 0x{:x}", item);
    }
    println!(" ]");

    print!("MIN Frame: ");
    let sent = match min.send_frame(id, &tx_data, tx_data.len() as u8) {
        Ok(size) => size,
        Err(_) => panic!("Opps!"),
    };

    assert_eq!(tx_data.len(), sent as usize);

    rx_byte(&mut min, &uart.rx_buf.borrow()[0..255], uart.get_rx_data_len() as u32);

    if 0 != min.get_rx_frame_len() {
        println!("The checksum in frame: 0x{:x}", min.get_rx_frame_checksum());
    }

    if let Ok(msg) = min.get_msg() {
        print!("app1 receive data: [ ");
        for i in 0..msg.len {
            print!("0x{:02x} ", msg.buf[i as usize]);
        }
        println!("]");
    }
    uart.close();
}
