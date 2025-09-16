use core::cell::RefCell;
/// UART interrupt handling module
use critical_section::Mutex;
use esp_hal::macros::{handler, ram};
use esp_hal::uart::{self, AnyUart, Uart, UartInterrupt};
use esp_hal::{Blocking, InterruptConfigurable};
use heapless::spsc::{Consumer, Producer, Queue};
use nb;

/// Queue capacity for UART receive buffers
pub const QUEUE_CAPACITY: usize = 8192;

/// Type alias for the UART byte receiver
pub type UartReceiver = Consumer<'static, u8, QUEUE_CAPACITY>;
pub const RX_FIFO_THRESHOLD: u16 = 96;

/// Number of UARTs supported
const NUM_UARTS: usize = 2;

/// Helper function to drain bytes from both UARTs in round-robin fashion
/// Always drains all bytes to prevent interrupt re-triggering
/// Panics if queue overflows - this indicates consumer is too slow
#[ram]
fn drain_uart_to_queue(cs: critical_section::CriticalSection) {
    // Get references to both UARTs and producers
    let mut uart0 = UARTS[0].borrow_ref_mut(cs);
    let mut uart1 = UARTS[1].borrow_ref_mut(cs);
    let mut producer0 = UART_PRODUCERS[0].borrow_ref_mut(cs);
    let mut producer1 = UART_PRODUCERS[1].borrow_ref_mut(cs);

    // Round-robin between both UARTs to ensure fairness
    let mut any_data = true;
    while any_data {
        any_data = false;

        // Try to read from UART0 if it exists
        if let (Some(uart), Some(producer)) = (uart0.as_mut(), producer0.as_mut()) {
            if let Ok(byte) = uart.read_byte() {
                producer
                    .enqueue(byte)
                    .expect("UART0 receive queue overflow - consumer too slow");
                any_data = true;
            }
        }

        // Try to read from UART1 if it exists (upstream)
        if let (Some(uart), Some(producer)) = (uart1.as_mut(), producer1.as_mut()) {
            if let Ok(byte) = uart.read_byte() {
                producer
                    .enqueue(byte)
                    .expect("UART1 receive queue overflow - consumer too slow");
                any_data = true;
            }
        }
    }
}

/// Type alias for UART instance stored in static memory
type UartInstance = Mutex<RefCell<Option<Uart<'static, Blocking, AnyUart>>>>;

/// Type alias for UART producer stored in static memory
type UartProducer = Mutex<RefCell<Option<Producer<'static, u8, QUEUE_CAPACITY>>>>;

/// Global UART instances for interrupt handling
static UARTS: [UartInstance; NUM_UARTS] = [
    Mutex::new(RefCell::new(None)),
    Mutex::new(RefCell::new(None)),
];

/// Global UART producers for interrupt handling
static UART_PRODUCERS: [UartProducer; NUM_UARTS] = [
    Mutex::new(RefCell::new(None)),
    Mutex::new(RefCell::new(None)),
];

/// Global event queues for UARTs
static mut UART_QUEUES: [Queue<u8, QUEUE_CAPACITY>; NUM_UARTS] = [Queue::new(), Queue::new()];

/// Common interrupt handler logic
#[ram]
fn handle_uart_interrupt(uart_index: usize) {
    critical_section::with(|cs| {
        // Drain both UARTs in round-robin fashion
        drain_uart_to_queue(cs);

        // Clear the interrupt for the specific UART that triggered this
        let mut uart = UARTS[uart_index].borrow_ref_mut(cs);
        if let Some(uart) = uart.as_mut() {
            uart.clear_interrupts(UartInterrupt::RxFifoFull.into());
        }
    });
}

/// UART0 interrupt handler
#[ram]
#[handler]
fn uart0_interrupt_handler() {
    handle_uart_interrupt(0);
}

/// UART1 interrupt handler
#[ram]
#[handler]
fn uart1_interrupt_handler() {
    handle_uart_interrupt(1);
}

/// A handle for interacting with a UART stored in a Mutex
pub struct UartHandle {
    uart_num: UartNum,
}

impl UartHandle {
    fn uart_index(&self) -> usize {
        self.uart_num as usize
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), uart::Error> {
        let mut up_to = 0;
        let uart_index = self.uart_index();
        while up_to < bytes.len() {
            critical_section::with(|cs| {
                let mut uart_opt = UARTS[uart_index].borrow_ref_mut(cs);
                let uart = uart_opt.as_mut().unwrap();
                // Write as many bytes as possible until the TX FIFO is full
                while up_to < bytes.len() {
                    match uart.write_byte(bytes[up_to]) {
                        Ok(_) => {
                            up_to += 1;
                        }
                        Err(nb::Error::WouldBlock) => {
                            // TX FIFO is full, exit inner loop to release critical section
                            // The hope is that this break will allow other interrupts to fire.
                            break;
                        }
                        Err(nb::Error::Other(e)) => {
                            // Actual UART error
                            return Err(e);
                        }
                    }
                }
                Ok(())
            })?;
        }

        Ok(())
    }

    pub fn flush_tx(&mut self) -> nb::Result<(), esp_hal::uart::Error> {
        critical_section::with(|cs| {
            let uart_index = self.uart_index();
            let mut uart_opt = UARTS[uart_index].borrow_ref_mut(cs);

            // Safe to unwrap: UartHandle is only created when UART exists
            let uart = uart_opt.as_mut().unwrap();
            uart.flush_tx()
        })
    }

    pub fn change_baud(&mut self, baudrate: u32) {
        critical_section::with(|cs| {
            let uart_index = self.uart_index();
            let mut uart_opt = UARTS[uart_index].borrow_ref_mut(cs);

            // Safe to unwrap: UartHandle is only created when UART exists
            let uart = uart_opt.as_mut().unwrap();
            uart.apply_config(&esp_hal::uart::Config {
                baudrate,
                rx_fifo_full_threshold: RX_FIFO_THRESHOLD,
                ..Default::default()
            })
            .unwrap();
        })
    }

    /// Fill buffer with any remaining bytes (for when there are fewer than threshold bytes)
    /// This drains both UARTs in round-robin fashion to ensure we never miss data
    pub fn fill_buffer(&mut self) {
        critical_section::with(|cs| {
            drain_uart_to_queue(cs);
        });
    }
}

/// Register a UART for interrupt handling
pub fn register_uart(
    mut uart: Uart<'static, Blocking, AnyUart>,
    uart_num: UartNum,
) -> (UartHandle, UartReceiver) {
    let uart_index = uart_num as usize;

    unsafe {
        // Split the queue into producer and consumer
        let queue_ref = &raw mut UART_QUEUES[uart_index];
        let (producer, consumer) = (*queue_ref).split();

        match uart_num {
            UartNum::Uart0 => uart.set_interrupt_handler(uart0_interrupt_handler),
            UartNum::Uart1 => uart.set_interrupt_handler(uart1_interrupt_handler),
        }

        uart.listen(UartInterrupt::RxFifoFull);

        // Store the UART instance and producer
        critical_section::with(|cs| {
            UARTS[uart_index].borrow_ref_mut(cs).replace(uart);
            UART_PRODUCERS[uart_index]
                .borrow_ref_mut(cs)
                .replace(producer);
        });

        // Return handle and consumer
        (UartHandle { uart_num }, consumer)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum UartNum {
    Uart0 = 0,
    Uart1 = 1,
}
