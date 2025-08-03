// Frostsnap custom PCB rev 2.x

#![no_std]
#![no_main]

extern crate alloc;
use cst816s::{TouchGesture, CST816S};
use display_interface_spi::SPIInterface;
use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use embedded_hal as hal;
use esp_hal::{
    delay::Delay,
    gpio::{Input, Level, Output, Pull},
    hmac::Hmac,
    i2c::master::{Config as i2cConfig, I2c},
    ledc::{
        channel::{self, ChannelIFace},
        timer::{self as timerledc, LSClockSource, TimerIFace},
        LSGlobalClkSource, Ledc, LowSpeed,
    },
    peripherals::Peripherals,
    prelude::*,
    rng::Trng,
    spi::{
        master::{Config as spiConfig, Spi},
        SpiMode,
    },
    timer::{
        self,
        timg::{Timer, TimerGroup},
    },
    uart::{self, Uart},
    usb_serial_jtag::UsbSerialJtag,
    Blocking,
};
use frostsnap_comms::Downstream;
use frostsnap_device::{
    display_utils::PALETTE, efuse::{self, EfuseHmacKeys}, esp32_run, io::SerialInterface, touch_calibration::{x_based_adjustment, y_based_adjustment}, ui::{
        BusyTask, Prompt, UiEvent, UserInteraction, Workflow
    }, widget_tree::WidgetTree, DownstreamConnectionState, Instant, UpstreamConnectionState
};
use frostsnap_embedded_widgets::{Widget, Welcome, DeviceNameScreen, keygen_check::KeygenCheck, FadeSwitcher};
use mipidsi::{error::Error, models::ST7789, options::ColorInversion};

// # Pin Configuration
//
// GPIO21:     USB UART0 TX  (connect upstream)
// GPIO20:     USB UART0 RX  (connect upstream)
//
// GPIO18:     JTAG/UART1 TX (connect downstream)
// GPIO19:     JTAG/UART1 RX (connect downstream)
//
// GPIO0: Upstream detection
// GPIO10: Downstream detection

macro_rules! init_display {
    (peripherals: $peripherals:ident, delay: $delay:expr) => {{
        let spi = Spi::new_with_config(
            $peripherals.SPI2,
            spiConfig {
                frequency: 80.MHz(),
                mode: SpiMode::Mode2,
                ..spiConfig::default()
            },
        )
        .with_sck($peripherals.GPIO8)
        .with_mosi($peripherals.GPIO7);

        let spi_device = embedded_hal_bus::spi::ExclusiveDevice::new_no_delay(spi, NoCs);
        let di = SPIInterface::new(spi_device, Output::new($peripherals.GPIO9, Level::Low));

        let display = mipidsi::Builder::new(ST7789, di)
            .display_size(240, 280)
            .display_offset(0, 20) // 240*280 panel
            .invert_colors(ColorInversion::Inverted)
            .reset_pin(Output::new($peripherals.GPIO6, Level::Low))
            .init($delay)
            .unwrap();

        display
    }};
}

#[entry]
fn main() -> ! {
    esp_alloc::heap_allocator!(256 * 1024);
    let peripherals = esp_hal::init({
        let mut config = esp_hal::Config::default();
        config.cpu_clock = CpuClock::max();
        config
    });
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let timg1 = TimerGroup::new(peripherals.TIMG1);
    let timer0 = timg0.timer0;
    let timer1 = timg1.timer0;

    let mut delay = Delay::new();

    let upstream_detect = Input::new(peripherals.GPIO0, Pull::Up);
    let downstream_detect = Input::new(peripherals.GPIO10, Pull::Up);

    // Turn off backlight to hide artifacts as display initializes
    let mut ledc = Ledc::new(peripherals.LEDC);
    ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);
    let mut lstimer0 = ledc.timer::<LowSpeed>(timerledc::Number::Timer0);
    lstimer0
        .configure(timerledc::config::Config {
            duty: timerledc::config::Duty::Duty10Bit,
            clock_source: LSClockSource::APBClk,
            frequency: 24u32.kHz(),
        })
        .unwrap();
    let mut channel0 = ledc.channel(channel::Number::Channel0, peripherals.GPIO1);
    channel0
        .configure(channel::config::Config {
            timer: &lstimer0,
            duty_pct: 0, // Turn off backlight to hide artifacts as display initializes
            pin_config: channel::config::PinConfig::PushPull,
        })
        .unwrap();

    let mut display = init_display!(peripherals: peripherals, delay: &mut delay);
    
    // Clear the screen to black at startup
    display.clear(PALETTE.background).unwrap();

    let i2c = I2c::new(
        peripherals.I2C0,
        i2cConfig {
            frequency: 400u32.kHz(),
            ..i2cConfig::default()
        },
    )
    .with_sda(peripherals.GPIO4)
    .with_scl(peripherals.GPIO5);
    let mut capsense = CST816S::new(
        i2c,
        Input::new(peripherals.GPIO2, Pull::Down),
        Output::new(peripherals.GPIO3, Level::Low),
    );
    capsense.setup(&mut delay).unwrap();

    // Initial display setup will be handled by widget tree
    channel0.start_duty_fade(0, 30, 500).unwrap();

    let detect_device_upstream = upstream_detect.is_low();
    let upstream_serial = if detect_device_upstream {
        let serial_conf = uart::Config {
            baudrate: frostsnap_comms::BAUDRATE,
            ..Default::default()
        };
        SerialInterface::new_uart(
            Uart::new_with_config(
                peripherals.UART1,
                serial_conf,
                peripherals.GPIO18,
                peripherals.GPIO19,
            )
            .unwrap(),
            &timer0,
        )
    } else {
        SerialInterface::new_jtag(UsbSerialJtag::new(peripherals.USB_DEVICE), &timer0)
    };
    let downstream_serial: SerialInterface<_, Downstream> = {
        let serial_conf = uart::Config {
            baudrate: frostsnap_comms::BAUDRATE,
            ..Default::default()
        };
        let uart = Uart::new_with_config(
            peripherals.UART0,
            serial_conf,
            peripherals.GPIO21,
            peripherals.GPIO20,
        )
        .unwrap();
        SerialInterface::new_uart(uart, &timer0)
    };
    let mut sha256 = esp_hal::sha::Sha::new(peripherals.SHA);

    let mut adc = peripherals.ADC1;
    let mut hal_rng = Trng::new(peripherals.RNG, &mut adc);
    // extract more entropy from the trng that we theoretically need
    let mut first_rng = frostsnap_device::extract_entropy(&mut hal_rng, &mut sha256, 1024);

    let efuse = efuse::EfuseController::new(peripherals.EFUSE);

    let do_read_protect = cfg!(feature = "read_protect_hmac_key");

    let hal_hmac = core::cell::RefCell::new(Hmac::new(peripherals.HMAC));
    let mut hmac_keys =
        EfuseHmacKeys::load_or_init(&efuse, &hal_hmac, do_read_protect, &mut hal_rng)
            .expect("should load efuse hmac keys");

    // Don't use the hal_rng directly -- first mix in entropy from the HMAC efuse.
    // TODO: maybe re-key the rng based on entropy from touces etc
    let rng = hmac_keys.fixed_entropy.mix_in_rng(&mut first_rng);

    let ui = FrostyUi {
        display,
        page_switcher: FadeSwitcher::new(
            WidgetTree::default(),
            500, // 500ms fade duration
            PALETTE.background,
        ),
        capsense,
        downstream_connection_state: DownstreamConnectionState::Disconnected,
        upstream_connection_state: None,
        last_touch: None,
        timer: &timer1,
        busy_task: Default::default(),
        recovery_mode: false,
        current_workflow: Workflow::None,
    };

    let run = esp32_run::Run {
        upstream_serial,
        downstream_serial,
        rng,
        ui,
        timer: &timer0,
        downstream_detect,
        sha256,
        hmac_keys,
    };
    run.run()
}

/// Dummy CS pin for our display
struct NoCs;

impl embedded_hal::digital::OutputPin for NoCs {
    fn set_low(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl embedded_hal::digital::ErrorType for NoCs {
    type Error = core::convert::Infallible;
}

pub struct FrostyUi<'t, T, DT, I2C, PINT, RST> {
    display: DT,
    page_switcher: FadeSwitcher<WidgetTree>,
    capsense: CST816S<I2C, PINT, RST>,
    last_touch: Option<(Point, Instant)>,
    downstream_connection_state: DownstreamConnectionState,
    upstream_connection_state: Option<UpstreamConnectionState>,
    timer: &'t Timer<T, Blocking>,
    busy_task: Option<BusyTask>,
    recovery_mode: bool,
    current_workflow: Workflow,
}

impl<T, DT, I2C, PINT, RST, CommE, PinE> UserInteraction for FrostyUi<'_, T, DT, I2C, PINT, RST>
where
    I2C: hal::i2c::I2c<Error = CommE>,
    PINT: hal::digital::InputPin,
    RST: hal::digital::StatefulOutputPin<Error = PinE>,
    T: timer::timg::Instance,
    DT: DrawTarget<Color = Rgb565, Error = Error> + OriginDimensions,
{
    fn set_downstream_connection_state(
        &mut self,
        state: frostsnap_device::DownstreamConnectionState,
    ) {
        if state != self.downstream_connection_state {
            self.downstream_connection_state = state;
            self.page_switcher.force_full_redraw();
        }
    }

    fn set_upstream_connection_state(&mut self, state: frostsnap_device::UpstreamConnectionState) {
        if Some(state) != self.upstream_connection_state {
            self.upstream_connection_state = Some(state);
            self.page_switcher.force_full_redraw();
        }
    }



    fn take_workflow(&mut self) -> Workflow {
        core::mem::replace(&mut self.current_workflow, Workflow::None)
    }

    fn set_workflow(&mut self, workflow: Workflow) {
        // Store workflow immediately to avoid move issues
        let old_workflow = core::mem::replace(&mut self.current_workflow, workflow);
        
        // Check if we can update the current widget instead of switching
        let current_widget = self.page_switcher.current_mut();
        
        match (current_widget, &self.current_workflow) {
            // If we're already showing a Welcome screen and need a Welcome screen, just leave it
            (WidgetTree::Welcome(_), Workflow::None | Workflow::WaitingFor(_) | Workflow::Debug(_) | 
             Workflow::DisplayBackup { .. } | Workflow::EnteringBackup(_) | 
             Workflow::DisplayAddress { .. } | Workflow::FirmwareUpgrade(_)) => {
                // Already showing Welcome, no need to change
                return;
            }
            
            // If we're already showing DeviceNaming and get another NamingDevice workflow, just update the text
            (WidgetTree::DeviceNaming(device_name_screen), Workflow::NamingDevice { new_name }) => {
                device_name_screen.set_name(new_name);
                return;
            }
            
            // If we're showing KeygenCheck and get another KeyGen prompt, we need a new one
            // because the security code would be different
            
            _ => {} // Different widget types, need to switch
        };
        
        // Convert workflow to widget tree - take the workflow back out to consume it
        let workflow = core::mem::replace(&mut self.current_workflow, old_workflow);
        let new_page = match workflow {
            Workflow::None => WidgetTree::Welcome(Welcome::new()),
            
            Workflow::WaitingFor(_) => {
                // TODO: Create proper waiting screens
                WidgetTree::Welcome(Welcome::new())
            }
            
            Workflow::UserPrompt(prompt) => {
                match prompt {
                    Prompt::KeyGen { phase } => {
                        // Extract the security check code from the session hash
                        let session_hash = phase.session_hash();
                        let security_check_code: [u8; 4] = [
                            session_hash.0[0],
                            session_hash.0[1],
                            session_hash.0[2],
                            session_hash.0[3],
                        ];
                        let t_of_n = phase.t_of_n();
                        
                        // Create the KeygenCheck widget
                        WidgetTree::KeygenCheck(KeygenCheck::new(security_check_code, t_of_n))
                    }
                    _ => {
                        // TODO: Handle other prompt types
                        // For now, just show welcome
                        WidgetTree::Welcome(Welcome::new())
                    }
                }
            }
            
            Workflow::Debug(_text) => {
                // TODO: Create text widget to display debug text
                WidgetTree::Welcome(Welcome::new())
            }
            
            Workflow::NamingDevice { new_name } => {
                let mut device_name_screen = DeviceNameScreen::new(new_name);
                device_name_screen.set_edit_mode(true);
                WidgetTree::DeviceNaming(device_name_screen)
            }
            
            Workflow::DisplayBackup { .. } => {
                // TODO: Create backup display screen
                WidgetTree::Welcome(Welcome::new())
            }
            
            Workflow::EnteringBackup(_) => {
                // TODO: Create backup entry screens
                WidgetTree::Welcome(Welcome::new())
            }
            
            Workflow::DisplayAddress { .. } => {
                // TODO: Create address display screen
                WidgetTree::Welcome(Welcome::new())
            }
            
            Workflow::FirmwareUpgrade(_status) => {
                // TODO: Create firmware upgrade progress screen
                WidgetTree::Welcome(Welcome::new())
            }
        };
        
        // Switch to the new page with fade transition
        self.page_switcher.switch_to(new_page);
    }

    fn poll(&mut self) -> Option<UiEvent> {
        // keep the timer register fresh
        let now = self.timer.now();
        let current_time = frostsnap_embedded_widgets::Instant::from_millis(
            now.duration_since_epoch().to_millis()
        );
        
        // Handle touch input
        match self.capsense.read_one_touch_event(true) {
            Some(touch) => {
                let corrected_y = touch.y + x_based_adjustment(touch.x) + y_based_adjustment(touch.y);
                let corrected_point = Point::new(touch.x, corrected_y);
                let is_release = touch.action == 1;
                
                // Handle vertical drag
                if let (Some((last_point, _)), TouchGesture::SlideUp | TouchGesture::SlideDown) = 
                    (self.last_touch, touch.gesture) {
                    self.page_switcher.handle_vertical_drag(
                        Some(last_point.y as u32),
                        corrected_y as u32,
                        is_release
                    );
                }
                
                // Update last touch
                if !is_release {
                    self.last_touch = Some((corrected_point, now));
                } else {
                    self.last_touch = None;
                }
                
                // Handle touch
                let _ = self.page_switcher.handle_touch(corrected_point, current_time, is_release);
            }
            None => {},
        };
        
        // Draw the widget tree
        let _ = self.page_switcher.draw(&mut self.display, current_time);
        
        // Check widget states and generate UI events
        match self.page_switcher.current() {
            WidgetTree::KeygenCheck(keygen_check) => {
                if keygen_check.is_confirmed() {
                    // User has confirmed the keygen security check
                    // Extract the phase from the current workflow
                    if let Workflow::UserPrompt(Prompt::KeyGen { phase }) = &self.current_workflow {
                        return Some(UiEvent::KeyGenConfirm {
                            phase: phase.clone(),
                        });
                    }
                }
            }
            _ => {}
        }
        
        None
    }
    
    fn set_busy_task(&mut self, task: BusyTask) {
        self.busy_task = Some(task);
        // TODO: Update widget tree based on busy task
        self.page_switcher.force_full_redraw();
    }
    
    fn clear_busy_task(&mut self) {
        self.busy_task = None;
        self.page_switcher.force_full_redraw();
    }
    
    fn set_recovery_mode(&mut self, value: bool) {
        self.recovery_mode = value;
        self.page_switcher.force_full_redraw();
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    use core::fmt::Write;
    // XXX: Don't try and remove this steal. This is the only way to get the peripherals after start
    // up.
    let peripherals = unsafe { Peripherals::steal() };
    let mut bl = Output::new(peripherals.GPIO1, Level::Low);

    let mut delay = Delay::new();
    let mut panic_buf = frostsnap_device::panic::PanicBuffer::<512>::default();

    let _ = match info.location() {
        Some(location) => write!(
            &mut panic_buf,
            "{}:{} {}",
            location.file().split('/').next_back().unwrap_or(""),
            location.line(),
            info
        ),
        None => write!(&mut panic_buf, "{info}"),
    };

    let mut display = init_display!(peripherals: peripherals, delay: &mut delay);
    frostsnap_device::display_utils::error_print(&mut display, panic_buf.as_str());
    bl.set_high();
    loop {}
}
