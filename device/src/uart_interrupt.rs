use core::cell::{Cell, RefCell};
/// UART interrupt handling module
use critical_section::Mutex;
use esp_hal::delay::Delay;
use esp_hal::peripherals::{UART0, UART1};
use esp_hal::uart::{self, RxConfig, RxError, RxErrorKind, Uart, UartInterrupt};
use esp_hal::{handler, ram, Blocking};
use heapless::spsc::{Consumer, Producer, Queue};

/// Queue capacity for UART receive buffers
pub const QUEUE_CAPACITY: usize = 8192;

/// Type alias for the UART byte receiver
pub type UartReceiver = Consumer<'static, u8, QUEUE_CAPACITY>;
pub const RX_FIFO_THRESHOLD: u16 = 32;

/// Number of UARTs supported
const NUM_UARTS: usize = 2;

const RX_CHUNK: usize = RX_FIFO_THRESHOLD as usize;

/// Drains both UARTs round-robin until neither has data left, so the RX interrupt does not
/// re-trigger.
///
/// Panics if a receive queue overflows: the consumer is too slow.
#[ram]
fn drain_uart_to_queue(cs: critical_section::CriticalSection<'_>) {
    let mut any_data = true;
    while any_data {
        any_data = false;
        for uart_index in 0..NUM_UARTS {
            any_data |= drain_chunk(cs, uart_index);
        }
    }
}

/// Returns whether the drain loop should poll this UART again.
#[ram]
fn drain_chunk(cs: critical_section::CriticalSection<'_>, uart_index: usize) -> bool {
    let mut uart = UARTS[uart_index].borrow_ref_mut(cs);
    let mut producer = UART_PRODUCERS[uart_index].borrow_ref_mut(cs);
    let (Some(uart), Some(producer)) = (uart.as_mut(), producer.as_mut()) else {
        return false;
    };

    let mut buf = [0u8; RX_CHUNK];
    match uart.read_buffered(&mut buf) {
        Ok(n) => {
            for &byte in &buf[..n] {
                producer
                    .enqueue(byte)
                    .unwrap_or_else(|_| panic!("UART{uart_index} receive queue overflow"));
            }
            n > 0
        }
        // esp-hal has already reset the RX FIFO, discarding what it held.
        Err(RxError::FifoOverflowed) => {
            RX_OVERFLOW_PENDING[uart_index].borrow(cs).set(true);
            true
        }
        Err(_) => true,
    }
}

/// Type alias for UART instance stored in static memory
type UartInstance = Mutex<RefCell<Option<Uart<'static, Blocking>>>>;

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

static RX_OVERFLOW_PENDING: [Mutex<Cell<bool>>; NUM_UARTS] =
    [Mutex::new(Cell::new(false)), Mutex::new(Cell::new(false))];

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

    pub fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), uart::TxError> {
        let mut up_to = 0;
        let uart_index = self.uart_index();
        while up_to < bytes.len() {
            critical_section::with(|cs| -> Result<(), uart::TxError> {
                let mut uart_opt = UARTS[uart_index].borrow_ref_mut(cs);
                let uart = uart_opt.as_mut().unwrap();
                // Write as many bytes as possible until the TX FIFO is full
                while up_to < bytes.len() {
                    if !uart.write_ready() {
                        // TX FIFO is full — release the critical section so
                        // interrupts can run and the FIFO can drain.
                        break;
                    }
                    up_to += uart.write(&bytes[up_to..])?;
                }
                Ok(())
            })?;
        }

        Ok(())
    }

    /// Blocks until everything written has left the TX line.
    ///
    /// esp-hal's `Uart::flush` spins with the `Uart` borrowed, which here means inside the critical
    /// section the RX interrupt needs. The same status registers are read directly instead: the
    /// reads have no side effects, so they need no exclusion.
    pub fn flush_tx(&mut self) {
        while !self.tx_fifo_empty() {}
        // The FSM is briefly idle after the last byte leaves the FIFO; esp-hal waits 10 µs too.
        Delay::new().delay_micros(10);
        while !self.tx_fsm_idle() {}
    }

    fn tx_fifo_empty(&self) -> bool {
        match self.uart_num {
            UartNum::Uart0 => UART0::regs().status().read().txfifo_cnt().bits() == 0,
            UartNum::Uart1 => UART1::regs().status().read().txfifo_cnt().bits() == 0,
        }
    }

    fn tx_fsm_idle(&self) -> bool {
        match self.uart_num {
            UartNum::Uart0 => UART0::regs().fsm_status().read().st_utx_out().bits() == 0,
            UartNum::Uart1 => UART1::regs().fsm_status().read().st_utx_out().bits() == 0,
        }
    }

    pub fn change_baud(&mut self, baudrate: u32) {
        critical_section::with(|cs| {
            let uart_index = self.uart_index();
            let mut uart_opt = UARTS[uart_index].borrow_ref_mut(cs);

            // Safe to unwrap: UartHandle is only created when UART exists
            let uart = uart_opt.as_mut().unwrap();
            uart.apply_config(&uart_config(baudrate)).unwrap();
        })
    }

    /// Fill buffer with any remaining bytes (for when there are fewer than threshold bytes)
    /// This drains both UARTs in round-robin fashion to ensure we never miss data
    pub fn fill_buffer(&mut self) {
        critical_section::with(|cs| {
            drain_uart_to_queue(cs);
        });
    }

    /// Whether the RX FIFO overflowed since the last [`Self::take_rx_overflow`]; esp-hal reset the
    /// FIFO, so up to a FIFO's worth of received bytes were lost.
    pub fn rx_overflow_pending(&self) -> bool {
        critical_section::with(|cs| RX_OVERFLOW_PENDING[self.uart_index()].borrow(cs).get())
    }

    /// Takes the pending overflow. Taking it in the same critical section as the dequeue that
    /// follows is what orders it against bytes the ISR enqueues after the FIFO reset.
    pub fn take_rx_overflow(&self, cs: critical_section::CriticalSection<'_>) -> bool {
        RX_OVERFLOW_PENDING[self.uart_index()]
            .borrow(cs)
            .replace(false)
    }
}

/// The configuration every device-to-device UART runs with, at `baudrate`.
pub fn uart_config(baudrate: u32) -> uart::Config {
    uart::Config::default().with_baudrate(baudrate).with_rx(
        RxConfig::default()
            .with_fifo_full_threshold(RX_FIFO_THRESHOLD)
            .with_reported_errors(RxErrorKind::FifoOverflowed),
    )
}

/// Register a UART for interrupt handling
pub fn register_uart(
    mut uart: Uart<'static, Blocking>,
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
