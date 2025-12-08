use crate::{
    layout::Padding, palette::PALETTE, prelude::*, string_ext::StringWrap, switcher::Switcher,
    DynWidget, Fps,
};
use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec::Vec;
use embedded_graphics::pixelcolor::Rgb565;
#[cfg(target_arch = "riscv32")]
use embedded_graphics::pixelcolor::RgbColor;
use embedded_graphics::{
    draw_target::DrawTarget,
    mono_font::{iso_8859_1::FONT_7X13, MonoTextStyle},
    prelude::{Point, Size},
};

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for which debug overlays to enable
#[derive(Clone, Copy, Debug, Default)]
pub struct EnabledDebug {
    pub logs: bool,
    pub memory: bool,
    pub fps: bool,
}

impl EnabledDebug {
    pub const ALL: Self = Self {
        logs: true,
        memory: true,
        fps: true,
    };

    pub const NONE: Self = Self {
        logs: false,
        memory: false,
        fps: false,
    };
}

// ============================================================================
// Logging functionality
// ============================================================================

static mut LOG_BUFFER: Option<VecDeque<String>> = None;
static mut LOG_DIRTY: bool = false;
static mut INITIAL_STACK_PTR: Option<usize> = None;

/// Set the initial stack pointer (called by init_log_stack_pointer! macro)
pub fn set_initial_stack_pointer(sp: usize) {
    unsafe {
        INITIAL_STACK_PTR = Some(sp);
    }
}

/// Get the initial stack pointer (used by log_stack! macro)
pub fn get_initial_stack_pointer() -> Option<usize> {
    unsafe { INITIAL_STACK_PTR }
}

pub fn init_logging() {
    unsafe {
        LOG_BUFFER = Some(VecDeque::with_capacity(32));
        LOG_DIRTY = false;
    }
}

pub fn log(msg: String) {
    unsafe {
        if let Some(ref mut buffer) = LOG_BUFFER {
            // Enforce capacity limit
            if buffer.len() == buffer.capacity() {
                buffer.pop_front();
            }
            buffer.push_back(msg);
            LOG_DIRTY = true;
        }
    }
}

#[inline(never)]
pub fn log_stack_usage(label: &str) {
    let stack_var = 0u32;
    let current_sp = &stack_var as *const _ as usize;

    unsafe {
        if let Some(initial_sp) = INITIAL_STACK_PTR {
            // Stack grows downward, so initial - current = bytes used
            let stack_used = initial_sp.saturating_sub(current_sp);
            log(alloc::format!("{}: {}", label, stack_used));
        }
    }
}

// Font for logging - FONT_7X13 is monospace
const FONT_HEIGHT: u32 = 13;
const FONT_WIDTH: u32 = 7;

pub struct DebugLogWidget {
    // Cache of formatted text widgets wrapped in expanded container
    display_cache:
        Container<Switcher<Padding<Column<Vec<Text<MonoTextStyle<'static, Rgb565>, StringWrap>>>>>>,
    max_lines: usize,
    chars_per_line: usize,
    max_size: Size,
}

impl Default for DebugLogWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl DebugLogWidget {
    pub fn new() -> Self {
        // Create empty column with padding
        let column = Column::new(Vec::new());
        let padded = Padding::only(column).bottom(20).build();
        let switcher = Switcher::new(padded);
        let container = Container::new(switcher).with_expanded();

        Self {
            display_cache: container,
            max_lines: 0,
            chars_per_line: 0,
            max_size: Size::zero(),
        }
    }

    fn rebuild_display(&mut self) {
        unsafe {
            if !LOG_DIRTY {
                return;
            }

            if let Some(ref mut buffer) = LOG_BUFFER {
                loop {
                    let mut text_widgets = Vec::new();

                    // Create text widgets for each log entry with line wrapping
                    for msg in buffer.iter() {
                        // Create StringWrap with line wrapping based on character width
                        let wrapped = StringWrap::from_str(msg, self.chars_per_line);

                        let text = Text::new_with(
                            wrapped,
                            MonoTextStyle::new(&FONT_7X13, PALETTE.text_secondary),
                        )
                        .with_underline(PALETTE.on_background);
                        text_widgets.push(text);
                    }

                    // Column with newest messages at bottom, aligned to left
                    let column = Column::new(text_widgets)
                        .with_main_axis_alignment(MainAxisAlignment::End)
                        .with_cross_axis_alignment(CrossAxisAlignment::Start);

                    // Wrap in padding to add 20px bottom padding
                    let mut padded = Padding::only(column).bottom(20).build();

                    // Set constraints to check for overflow
                    padded.set_constraints(self.max_size);

                    // If there's no overflow or the buffer is empty, we're done
                    if !padded.child.has_overflow() || buffer.is_empty() {
                        self.display_cache.child.switch_to(padded);
                        LOG_DIRTY = false;
                        break;
                    }

                    // Remove the oldest message and try again
                    buffer.pop_front();
                }
            }
        }
    }
}

impl Widget for DebugLogWidget {
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        // Just delegate to the child - no screen clearing!
        self.rebuild_display();
        self.display_cache.draw(target, current_time)
    }
}

impl DynWidget for DebugLogWidget {
    fn set_constraints(&mut self, max_size: Size) {
        // Store the max size for overflow detection
        self.max_size = max_size;

        // Calculate chars per line based on font width constant
        self.chars_per_line = (max_size.width / FONT_WIDTH) as usize;

        // Calculate max lines based on font height constant (kept for reference but not used)
        self.max_lines = (max_size.height / FONT_HEIGHT) as usize;

        // Update the display cache constraints
        self.display_cache.set_constraints(max_size);

        // Force rebuild with new constraints
        unsafe {
            LOG_DIRTY = true;
        }
    }

    fn sizing(&self) -> crate::Sizing {
        // Just return the display cache's sizing
        self.display_cache.sizing()
    }

    fn force_full_redraw(&mut self) {
        unsafe {
            LOG_DIRTY = true;
        }
    }
}

// ============================================================================
// Memory indicator (only on RISC-V/ESP32)
// ============================================================================

#[cfg(target_arch = "riscv32")]
const MEM_TEXT_SIZE: usize = 13; // Size for "U:123456 F:123456"

#[cfg(target_arch = "riscv32")]
type MemText = Text<MonoTextStyle<'static, Rgb565>, crate::string_ext::StringFixed<MEM_TEXT_SIZE>>;

/// Memory usage indicator component that polls esp_alloc directly
#[cfg(target_arch = "riscv32")]
pub struct MemoryIndicator {
    display: Container<Switcher<MemText>>,
    last_draw_time: Option<Instant>,
}

#[cfg(target_arch = "riscv32")]
impl Default for MemoryIndicator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_arch = "riscv32")]
const MEM_TEXT_STYLE: MonoTextStyle<'static, Rgb565> = MonoTextStyle::new(&FONT_7X13, Rgb565::CYAN);

#[cfg(target_arch = "riscv32")]
impl MemoryIndicator {
    fn new() -> Self {
        let initial_text = Text::new_with(
            crate::string_ext::StringFixed::from_string("000000/000000"),
            MEM_TEXT_STYLE,
        );
        let display = Container::new(Switcher::new(initial_text).with_shrink_to_fit());

        Self {
            display,
            last_draw_time: None,
        }
    }
}

#[cfg(target_arch = "riscv32")]
impl DynWidget for MemoryIndicator {
    fn set_constraints(&mut self, max_size: Size) {
        self.display.set_constraints(max_size);
    }

    fn sizing(&self) -> crate::Sizing {
        self.display.sizing()
    }

    fn force_full_redraw(&mut self) {
        self.display.force_full_redraw();
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        self.display.handle_touch(point, current_time, is_release)
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        self.display.handle_vertical_drag(prev_y, new_y, is_release);
    }
}

#[cfg(target_arch = "riscv32")]
impl Widget for MemoryIndicator {
    type Color = Rgb565;

    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: Instant,
    ) -> Result<(), D::Error> {
        // Update every 500ms (same rate as FPS widget)
        let should_update = match self.last_draw_time {
            Some(last_time) => current_time.saturating_duration_since(last_time) >= 500,
            None => true,
        };

        if should_update {
            self.last_draw_time = Some(current_time);

            // Get heap used and free from esp_alloc
            let used = esp_alloc::HEAP.used();
            let free = esp_alloc::HEAP.free();

            // Format the memory stats into StringBuffer
            use core::fmt::Write;
            let mut buf = crate::string_ext::StringFixed::<MEM_TEXT_SIZE>::new();
            let _ = write!(&mut buf, "{}/{}", used, free);

            // Create a new text widget with the updated text
            let text_widget = Text::new_with(buf, MEM_TEXT_STYLE);
            self.display.child.switch_to(text_widget);
        }

        // Always draw the display (it handles its own dirty tracking)
        self.display.draw(target, current_time)
    }
}

// Stub type for when not on RISC-V
#[cfg(not(target_arch = "riscv32"))]
type MemoryIndicator = ();

// ============================================================================
// OverlayDebug - Main overlay widget
// ============================================================================

/// A widget that overlays debug info (stats and/or logging) on top of another widget
pub struct OverlayDebug<W>
where
    W: DynWidget,
{
    // Outer stack with all the overlays
    outer_stack: Stack<(
        Stack<(W, Option<DebugLogWidget>)>,
        Option<Row<(Option<Fps>, Option<MemoryIndicator>)>>,
    )>,

    // Track current view index (0 = main, 1 = logs if enabled)
    current_index: usize,

    // Whether logs are enabled
    logs_enabled: bool,
}

impl<W> OverlayDebug<W>
where
    W: Widget<Color = Rgb565>,
{
    pub fn new(child: W, config: EnabledDebug) -> Self {
        // Create optional log widget
        let log_widget = if config.logs {
            init_logging();
            Some(DebugLogWidget::new())
        } else {
            None
        };

        // Create indexed stack for main widget and optional log viewer
        let mut indexed_stack = Stack::builder().push(child).push(log_widget);
        indexed_stack.set_index(Some(0)); // Start showing main widget

        // Create optional FPS widget
        let fps_widget = if config.fps {
            Some(Fps::new(500))
        } else {
            None
        };

        // Create optional memory widget (only on RISC-V)
        #[cfg(target_arch = "riscv32")]
        let mem_widget = if config.memory {
            Some(MemoryIndicator::default())
        } else {
            None
        };
        #[cfg(not(target_arch = "riscv32"))]
        let mem_widget: Option<MemoryIndicator> = if config.memory { Some(()) } else { None };

        // Create stats row if either FPS or memory is enabled
        let stats_row = if config.fps || config.memory {
            let row = Row::builder()
                .push(fps_widget)
                .gap(8)
                .push(mem_widget)
                .with_cross_axis_alignment(CrossAxisAlignment::Start)
                .with_main_axis_alignment(MainAxisAlignment::Center);
            Some(row)
        } else {
            None
        };

        // Create outer stack with all overlays
        let outer_stack = Stack::builder()
            .push(indexed_stack)
            .push_aligned(stats_row, Alignment::TopLeft);

        Self {
            outer_stack,
            current_index: 0,
            logs_enabled: config.logs,
        }
    }

    /// Get mutable reference to the inner child widget
    pub fn inner_mut(&mut self) -> &mut W {
        &mut self.outer_stack.children.0.children.0
    }

    /// Get reference to the inner child widget
    pub fn inner(&self) -> &W {
        &self.outer_stack.children.0.children.0
    }

    /// Switch to showing the log view (no-op if logs not enabled)
    pub fn show_logs(&mut self) {
        if self.logs_enabled {
            self.current_index = 1;
            self.outer_stack.children.0.set_index(Some(1));
        }
    }

    /// Switch to showing the main widget
    pub fn show_main(&mut self) {
        self.current_index = 0;
        self.outer_stack.children.0.set_index(Some(0));
    }

    /// Toggle between main widget and log view (no-op if logs not enabled)
    pub fn toggle_view(&mut self) {
        if self.logs_enabled {
            if self.current_index == 0 {
                self.show_logs();
            } else {
                self.show_main();
            }
        }
    }
}

impl<W> DynWidget for OverlayDebug<W>
where
    W: Widget<Color = Rgb565>,
{
    fn set_constraints(&mut self, max_size: Size) {
        self.outer_stack.set_constraints(max_size);
    }

    fn sizing(&self) -> crate::Sizing {
        self.outer_stack.sizing()
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        self.outer_stack
            .handle_touch(point, current_time, is_release)
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        self.outer_stack
            .handle_vertical_drag(prev_y, new_y, is_release)
    }

    fn force_full_redraw(&mut self) {
        self.outer_stack.force_full_redraw()
    }
}

impl<W> Widget for OverlayDebug<W>
where
    W: Widget<Color = Rgb565>,
{
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        self.outer_stack.draw(target, current_time)
    }
}

// ============================================================================
// Macros for logging
// ============================================================================

/// Initialize the stack pointer at the caller's location
#[macro_export]
macro_rules! init_log_stack_pointer {
    () => {{
        // Capture stack pointer at the macro call site
        let stack_var = 0u32;
        let sp = &stack_var as *const _ as usize;
        $crate::debug::set_initial_stack_pointer(sp);
    }};
}

/// Log current stack usage with file and line info
#[macro_export]
macro_rules! log_stack {
    (once) => {{
        static mut LOGGED: bool = false;
        if unsafe { !LOGGED } {
            unsafe {
                LOGGED = true;
            }
            $crate::log_stack!();
        }
    }};
    () => {
        $crate::log!("{}:{}", file!().rsplit('/').next().unwrap_or(file!());, line!())
    };
}
