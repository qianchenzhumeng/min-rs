
#[cfg(test)]
mod tests {
    extern crate min_rs as min;
    use std::cell::RefCell;

    struct App {
        name: String,
    }

    impl App {
        fn print_msg(&self, buffer: &[u8], len: u8) {
            print!("The data received: [ ");
            for i in 0..len {
                print!("0x{:02x} ", buffer[i as usize]);
            }
            println!("]");
        }
    }

    impl min::Name for App {
        fn name(&self) -> String {
            self.name.clone()
        }
    }

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

    fn application_handler(app: &App, _min_id: u8, buffer: &[u8], len: u8, _port: u8) {
        app.print_msg(buffer, len);
    }

    fn rx_byte(min: &mut min::Context<Uart, App>, buf: &[u8], buf_len: u32) {
        min.poll(buf, buf_len);
    }

    #[test]
    fn transport_receive_reset() {
        let app = App{
            name: String::from("app")
        };
        let uart = Uart{
            tx_space_avaliable: 128,
            rx_buf: RefCell::new([0; 255]),
            rx_buf_index: RefCell::new(0),
            loopback: true,
        };
        let mut min = min::Context::new(
            &uart,
            &app,
            0,
            true,
            tx_start,
            tx_finished,
            tx_space,
            tx_byte,
            application_handler,
        );

        uart.open();

        min.reset_transport(true).unwrap();
        rx_byte(&mut min, &uart.rx_buf.borrow()[0..255], uart.get_rx_data_len() as u32);

        assert_eq!(min.get_reset_cnt(), 1);

        uart.close();
    }

    #[test]
    fn transport_receive_spurious_ack() {
        let ack: [u8; 12] = [0xaa, 0xaa, 0xaa, 0xff, 0x02, 0x01, 0x02, 0x0b, 0xd0, 0x5d, 0xee, 0x55];
        let app = App{
            name: String::from("app")
        };
        let uart = Uart{
            tx_space_avaliable: 128,
            rx_buf: RefCell::new([0; 255]),
            rx_buf_index: RefCell::new(0),
            loopback: false,
        };
        let mut min = min::Context::new(
            &uart,
            &app,
            0,
            true,
            tx_start,
            tx_finished,
            tx_space,
            tx_byte,
            application_handler,
        );

        uart.open();

        min.poll(&ack[..], ack.len() as u32);

        assert_eq!(min.get_spurious_ack_cnt(), 1);

        uart.close();
    }

    #[test]
    fn transport_sequence_mismatch() {
        let frame: [u8; 19] = [
            0xaa, 0xaa, 0xaa,
            0x80,
            0x01,   // Sequence
            0x08,
            0xbb, 0xbb, 0xbb, 0x00, 0x00, 0x00, 0x00, 0x01,
            0xe6, 0x98, 0x4f, 0xde,
            0x55
        ];
        let app = App{
            name: String::from("app")
        };
        let uart = Uart{
            tx_space_avaliable: 128,
            rx_buf: RefCell::new([0; 255]),
            rx_buf_index: RefCell::new(0),
            loopback: false,
        };
        let mut min = min::Context::new(
            &uart,
            &app,
            0,
            true,
            tx_start,
            tx_finished,
            tx_space,
            tx_byte,
            application_handler,
        );

        uart.open();

        min.poll(&frame[..], frame.len() as u32);

        assert_eq!(min.get_drop_cnt(), 1);

        uart.close();
    }
}
