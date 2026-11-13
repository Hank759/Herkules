//! Complete the two TODOs in this file.

use embassy_stm32::{
    Peripherals,
    bind_interrupts,
    peripherals,
    can::Can,
    gpio::{Flex, Level, Output, Speed},
    usart::{self, BufferedUart},
};

use static_cell::StaticCell;

use crate::swd::Swd;

bind_interrupts!(struct Irqs {
    UART7 => usart::BufferedInterruptHandler<peripherals::UART7>;
});

pub struct Ports {
    pub can: Can<'static>,
    pub uart: Option<BufferedUart<'static>>,
}

pub fn configure(p: Peripherals) -> (crate::board::Usb, Ports, Option<Swd>) {
    let usb = crate::board::usb(p.USB_OTG_HS, p.PA12, p.PA11);

    // TODO 1: Pick two exposed 3.3 V GPIOs and pass them to Swd::new.
    // Use Output for SWCLK (initially low), and Flex for bidirectional SWDIO.

    let clock = Output::new(p.PE9, Level::Low, Speed::Low);

    let data = Flex::new(p.PE13);

    let swd = Some(Swd::new(clock, data));

    // TODO 2: Configure a buffered UART at 115200 baud, 8N1.
    // Identify the peripheral, RX/TX pins and interrupt from the schematic.
    // Bind the interrupt and provide static RX and TX buffers.
    
    static TX_BUF: StaticCell<[u8; 128]> = StaticCell::new();
    static RX_BUF: StaticCell<[u8; 128]> = StaticCell::new();

    let mut config = usart::Config::default();
    config.baudrate = 115_200;
    config.data_bits = usart::DataBits::DataBits8;
    config.parity = usart::Parity::ParityNone;
    config.stop_bits = usart::StopBits::STOP1;

    let uart = Some(BufferedUart::new(
        p.UART7, p.PE7, p.PE8,
        TX_BUF.init([0; 128]),
        RX_BUF.init([0; 128]),
        Irqs, config,
    ).unwrap());

    let can = crate::board::can1(p.FDCAN1, p.PD0, p.PD1);
    (usb, Ports { can, uart }, swd)
}
