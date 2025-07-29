# Refactoring Plan: `frostsnap_embedded_graphics` Integration

This document outlines the plan to refactor the device UI by integrating `frostsnap_embedded_widgets` and removing the old `graphics.rs` module.

## Goals

1.  **Remove `graphics.rs`:** Replace the custom `Graphics` struct and its framebuffer with direct drawing from widgets.
2.  **Decouple UI State from `Workflow`:** The `Workflow` enum should only be used for informational purposes and not store UI state.
3.  **Introduce a Widget Tree:** Create a "widget tree" (represented by a `WidgetTree` enum) to manage the current UI drawing state.
4.  **Simplify `poll` loop:** The main UI loop should handle touch input and delegate drawing to the current widget in the tree.
5.  **Enum-based Widget Management:** Use an enum to manage the different UI pages/states since `dyn Widget` is not possible.

## How UiEvents Will Be Handled

The `UiEvent` is the primary way the UI communicates back to the application logic. The `WidgetTree` enum will act as a translator between low-level widget interactions and high-level application `UiEvent`s. The widgets themselves (e.g., `HoldToConfirm`) will not know about `UiEvent`; they will communicate state changes through polling methods.

### Flow of Events

1.  **Context is Paired with the Widget:** `WidgetTree` enum variants will hold both the active widget and the associated application-level data. For example: `WidgetTree::Prompt { widget: HoldToConfirm<...>, data: Prompt }`. The `data` field contains the specific information, like `Prompt::Signing { phase }`.

2.  **`handle_touch` Updates State:** A widget's `handle_touch` method is called on every poll cycle with user input. Its sole responsibility is to update the widget's internal state (e.g., tracking the duration of a press for `HoldToConfirm`).

3.  **State is Polled for Completion:** After calling `handle_touch`, the `WidgetTree` will immediately call a separate polling method on the widget, such as `is_confirmed()` or `is_finished()`.

4.  **`WidgetTree` Translates Completion to `UiEvent`s:** If the polling method returns `true`, it signifies the user has completed the action. The `WidgetTree` then uses its stored `data` to construct and return the correct, specific `UiEvent`.

This design keeps the widgets in `frostsnap_embedded_widgets` generic and reusable while centralizing application-specific UI logic within the `WidgetTree` enum.

## Execution Steps

1.  **Initial Analysis:**
    *   Read `device/src/ui.rs` to understand the `Ui` struct, `set_workflow`, and `poll` methods.
    *   Read `device/src/graphics.rs` to understand the `Graphics` struct and framebuffer.
    *   Read `frostsnap_embedded_widgets/src/lib.rs` to see available widgets.
    *   Read `device/src/bin/v2.rs` to understand UI initialization.

2.  **Introduce `WidgetTree` Enum:**
    *   In `device/src/ui.rs`, create a `WidgetTree` enum.
    *   Each variant of the enum will correspond to a different screen/state of the UI.
    *   Each variant will hold a specific widget from `frostsnap_embedded_widgets`.

3.  **Refactor `Ui` Struct:**
    *   In `device/src/ui.rs`, modify the `Ui` struct (which will likely be `FrostyUi` in `v2.rs`).
    *   Remove the `graphics` and `workflow` fields.
    *   Add a new `page` field of type `WidgetTree`.
    *   The `Ui` struct will now directly manage the display, touch sensor, and the current `WidgetTree`.

4.  **Update `Ui::new()`:**
    *   Adjust the `Ui` constructor to initialize the `page` field with a default state, likely `WidgetTree::Welcome`.

5.  **Refactor `Workflow` Enum and Rework `set_workflow` Function:**
    *   The `Workflow` enum in `device/src/ui.rs` will be refactored to be completely stateless. Variants that currently hold state (like `EnteringBackup(EnteringBackupStage)` which contains a screen, or `UserPrompt` which contains an `AnimationState`) will be simplified to only carry identifying data.
    *   The `set_workflow` function will be kept, but its internal logic will change.
    *   It will take an instance of the newly refactored, stateless `Workflow` enum.
    *   It will be responsible for constructing the appropriate stateful `WidgetTree` variant based on the input `Workflow` and updating the `Ui`'s internal `page` field.

6.  **Simplify `poll`:**
    *   The `poll` method will be streamlined to perform a single UI update cycle:
        1.  Read touch input from the `cst816s` sensor.
        2.  Delegate touch handling to the current `page` widget.
        3.  Call the `draw` method on the current `page` widget, passing the display as the target.

7.  **Remove `graphics.rs`:**
    *   Once the `Graphics` struct and its framebuffer are no longer used, delete the `device/src/graphics.rs` file.
    *   Remove the `mod graphics;` declaration from `device/src/lib.rs` or `device/src/bin/v2.rs`.

8.  **Update Entrypoint (`v2.rs`):**
    *   Update the `main` function in `device/src/bin/v2.rs`.
    *   Modify the initialization of the `FrostyUi` struct to align with the new design.
    *   Ensure the main loop correctly calls the new `poll` method.

---

## Appendix: Informal Workflow to Widget Mapping

This section describes the intended refactoring of the `Workflow` enum and how each variant will map to a stateful widget from `frostsnap_embedded_widgets`. The principle is that the `Workflow` enum becomes a stateless message, and the `set_workflow` function acts as a factory or updater for the stateful `WidgetTree`.

### Refactored `Workflow` and Widget Pairings

| Original `Workflow` Variant | Refactored `Workflow` (Stateless) | Target `WidgetTree` Variant | Widget Used | Notes |
| :--- | :--- | :--- | :--- | :--- |
| `None` / `WaitingFor::CoordinatorInstruction` | `Workflow::Ready` | `WidgetTree::Ready` | `ReadyScreen` | **Missing widget.** Shows device name. The main idle screen. |
| `WaitingFor::LookingForUpstream` | `Workflow::LookingForUpstream` | `WidgetTree::Text` | `text::Text` | Displays a simple static message. |
| `WaitingFor::CoordinatorAnnounceAck` | `Workflow::CoordinatorAnnounceAck` | `WidgetTree::Text` | `text::Text` | Displays a simple static message. |
| `WaitingForKeyGenFinalize` | `Workflow::WaitingForKeyGenFinalize` | `WidgetTree::KeyGenPendingFinalize` | `KeyGenPendingFinalizeScreen` | **Missing widget.** Displays the specific key generation finalization screen. |
| `UserPrompt` | `Workflow::Prompt(Prompt)` | `WidgetTree::Prompt` | `hold_to_confirm::HoldToConfirm` | The `AnimationState` is removed from the `Workflow` and its logic is now fully contained within the `HoldToConfirm` widget. |
| `NamingDevice` | `Workflow::NamingDevice` | `WidgetTree::Prompt` | `hold_to_confirm::HoldToConfirm` | This workflow will be handled by `set_workflow` by creating a `Prompt::NewName` and constructing a `HoldToConfirm` widget. |
| `DisplayBackup` | `Workflow::DisplayBackup` | `WidgetTree::DisplayBackup` | `BackupScreen` | **Missing widget.** Re-implements the existing bech32 backup display logic. |
| `EnteringBackup` | `Workflow::EnterBackup(phase)` | `WidgetTree::EnterBackup(EnteringBackupStage)` | `EnterShareIndexScreen` / `EnterShareScreen` | **Missing widgets.** The state machine logic from `v2.rs` that transitions between these two screens will be moved into the `WidgetTree::EnterBackup`'s `handle_touch` and `draw` implementations. |
| `DisplayAddress` | `Workflow::DisplayAddress` | `WidgetTree::Address` | `AddressScreen` | **Missing widget.** Re-implements the address display with the QR-like visualization. |
| `FirmwareUpgrade` | `Workflow::FirmwareUpgrade(status)` | `WidgetTree::FirmwareUpgrade` | `progress_bars::ProgressBar` | The `progress` value is passed to the widget. See state handling below. |
| `Debug` | `Workflow::Debug(message)` | `WidgetTree::Text` | `text::Text` | Displays the debug string. |

### Handling Stateful Updates (e.g., Progress Bar)

A key part of this refactoring is how `set_workflow` handles updates to a page that is already displayed. It must be intelligent enough to update the existing widget rather than creating a new one from scratch.

**Example: `FirmwareUpgrade`**

1.  **Initial Call:** The coordinator sends a message that starts the download. The application calls:
    `set_workflow(Workflow::FirmwareUpgrade(FirmwareUpgradeStatus::Download { progress: 0.1 }))`

2.  **`set_workflow` Logic:**
    *   It checks the *current* `self.page`.
    *   It sees the current page is, for example, `WidgetTree::Ready`.
    *   Because the new workflow is `FirmwareUpgrade` and the current page is not, it creates a **new** widget: `let bar = ProgressBar::new(0.1)`. 
    *   It then sets the page: `self.page = WidgetTree::FirmwareUpgrade(bar)`. The screen now shows a progress bar at 10%.

3.  **Subsequent Call:** The coordinator sends a progress update. The application calls:
    `set_workflow(Workflow::FirmwareUpgrade(FirmwareUpgradeStatus::Download { progress: 0.2 }))`

4.  **`set_workflow` Logic (Update Path):**
    *   It checks the *current* `self.page`.
    *   It sees the current page is `WidgetTree::FirmwareUpgrade(bar)`. 
    *   Because the new workflow is of the same type as the current page, it does **not** create a new widget. 
    *   Instead, it calls a method on the existing widget to update it: `bar.set_progress(0.2)`.

This ensures smooth, efficient updates without flickering or loss of state, as the state is always owned by the widget itself, not the transient `Workflow` message.

---

## Appendix: Proposed `WidgetTree` Enum Definition

Based on the workflow-to-widget mapping, here is a proposed initial definition for the `WidgetTree` enum. This enum will live in `device/src/ui.rs` and will be the central component for managing the UI state. To simplify development and avoid the complexity of lifetimes, all widgets will use owned data (e.g., `String`) instead of borrowed slices (`&str`).

```rust
// In device/src/ui.rs

// Forward-declare the stage enum for clarity
pub enum EnteringBackupStage {
    ShareIndex(EnterShareIndexScreen),
    Share(EnterShareScreen),
}

// The main state-holding UI enum
pub enum WidgetTree {
    // Simple static screens
    Welcome(Welcome),
    Text(TextWidget),

    // Required new widgets (currently missing from frostsnap_embedded_widgets)
    Ready(ReadyScreen),
    KeyGenPendingFinalize(KeyGenPendingFinalizeScreen),
    Address(AddressScreen),
    DisplayBackup(BackupScreen),

    // Interactive prompts
    Prompt {
        widget: HoldToConfirm<TextWidget, TextWidget>,
        data: Prompt, // The associated prompt data
    },

    // Complex interactive screens
    EnterBackup {
        stage: EnteringBackupStage,
        phase: EnterBackupPhase, // The context needed to process the result
    },

    // Progress indicators
    FirmwareUpgrade(ProgressBar),
}

// Note: The associated types like TextWidget, EnteringBackupStage, etc.,
// will also be defined in this file for clarity.

*   **`TextWidget` Type Alias:** `TextWidget` is a type alias for `ColorMap<Text, Rgb565>`. This is necessary because the base `Text` widget draws in `BinaryColor`, but the screen requires `Rgb565`. The `ColorMap` wraps the `Text` widget and translates its colors to the appropriate format for the display. Using a type alias keeps the `WidgetTree` definition clean and readable.
```

### Notes on the Definition:

*   **Owned Data:** The `WidgetTree` and its contained widgets use owned data types (like `String`) instead of borrowed references (`&str`). This simplifies the code by removing the need for lifetime parameters (`'a`), at the cost of potentially more memory allocations. This is a deliberate trade-off for faster initial development.
*   **Missing Widgets:** The widgets `ReadyScreen`, `KeyGenPendingFinalizeScreen`, `AddressScreen`, `BackupScreen`, `EnterShareIndexScreen` and `EnterShareScreen` are listed as if they exist, but they are currently **missing** from `frostsnap_embedded_widgets`. A key part of this task will be to create these new widgets, implementing the `Widget` trait to draw directly to the display.
*   **`Prompt` Variant:** This variant explicitly pairs the `HoldToConfirm` widget with the `Prompt` data it relates to. This is key for translating the confirmation event into the correct `UiEvent`.
*   **`EnterBackup` Variant:** This variant encapsulates the state machine logic that was previously in the main `poll` loop. The `EnteringBackupStage` enum holds the currently active screen (`EnterShareIndexScreen` or `EnterShareScreen`), and the `phase` data is carried along, ready to be used when the user completes the entry.
