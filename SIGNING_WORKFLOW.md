## Sign Transaction Workflow

### Overview
The sign transaction workflow is a comprehensive multi-screen flow that guides users through reviewing and confirming transaction details before signing.

### Screen Structure
The workflow dynamically adapts based on transaction complexity:
- **Formula**: `(External Outputs × 2) + 1 (Network Fee) + (Active Cautions) + 1 (Confirmation)`
- **Navigation**: Swipe up/down gestures between screens
- **Final Action**: Hold-to-confirm for transaction signing

### Screen-by-Screen Breakdown

#### 1. Amount Screens (2 screens per output)

**Amount Display Screen**
- **Title**: "Send Amount #[n]" (slate-400, text-lg)
- **Amount**: Large blue-300 text (text-3xl font-semibold)
- **Unit**: "BTC" in slate-500 (text-lg)
- **Action**: Swipe up to continue

**Recipient Address Screen**
- **Title**: "Recipient Address #[n]" (slate-400, text-lg)
- **Address**: Formatted in chunks with blue-300 highlighting
- **Action**: Swipe up to continue

#### 2. Network Fee Screen
- **Title**: "Network Fee" (slate-400, text-lg)
- **Fee Amount**: Bitcoin amount in blue-300 (text-3xl font-semibold)
- **Fee Rate**: "[rate] sats/vB" in slate-500 (text-xl font-bold)
- **Action**: Swipe up to continue

#### 3. Caution Screens (Conditional)

Displayed when transaction triggers warnings:

**High Fee Caution**
- **Icon**: ⚠️ warning symbol
- **Title**: "Caution" in yellow-400 (text-xl font-bold)
- **Message**: "High fee detected" (slate-100, font-bold text-xl)
- **Warning Types**:
  - Absolute fee > 0.001 BTC: "Fee exceeds 0.001 BTC"
  - Fee rate > 50 sats/vB: "Fee rate exceeds 50 sats/vB"
  - Fee > 5% of amount: "Fee exceeds 5% of transaction amount"
- **Action**: Swipe up to acknowledge and continue

#### 4. Final Confirmation Screen
- **Status**: "Hold to Sign" (slate-200, text-base font-semibold)
- **Instruction**: "Press and hold for 5 seconds" (slate-400, text-sm)
- **Visual Feedback**: Circular green progress bar during hold
- **Button Colors**: Green gradient (green-600 to green-700) with green-500 border
- **Completion**: Animated checkmark with drawing effect

### User Interactions

#### Swipe Navigation
- **Swipe Up**: Advance to next screen
  - Threshold: 30px upward movement
  - Active zone: Bottom 40% of screen
  - Disabled on final confirmation screen
  
- **Swipe Down**: Return to previous screen
  - Threshold: 30px downward movement
  - Active zone: Top 40% of screen
  - Disabled on first screen

- **Visual Indicator**: "Swipe up" text at bottom (slate-400, text-sm)

#### Hold-to-Confirm
- **Duration**: 5 seconds (5000ms)
- **Progress**: Real-time circular progress bar
- **Update Rate**: 2% every 100ms
- **Color Scheme**: Green theme throughout
- **Success**: Animated checkmark upon completion

### Animations and Transitions

#### Screen Transitions
- **Slide Up Animation**: `animate-slide-up-screen` (0.3s ease-out)
  - Opacity: 0 → 1
  - Transform: translateY(20px) → translateY(0)
  
- **Slide Down Animation**: `animate-slide-down-screen` (0.3s ease-out)
  - Opacity: 0 → 1
  - Transform: translateY(-20px) → translateY(0)

#### Confirmation Animations
- **Progress Bar**: Circular overlay with smooth fill animation
- **Success State**: 
  - Fade-in: 0.5s ease-out
  - Checkmark draw: 0.8s ease-out
  - SVG stroke-dasharray animation (24 units)

### Timing Specifications
- **Hold Duration**: 5 seconds for transaction signing
- **Screen Transition**: 0.3 seconds
- **Checkmark Animation**: 0.8 seconds
- **Success Display**: Brief pause before reset
- **Auto-return**: Device returns to waiting screen after confirmation

### Visual Design System

#### Color Palette
- **Background**: Dark gradient (slate-900 to slate-800)
- **Headers/Labels**: slate-400
- **Values (Amount/Address)**: blue-300
- **Instructions**: slate-200/slate-400
- **Cautions**: yellow-400
- **Confirmations**: green-600/green-500
- **Destructive Actions**: red-600

#### Typography Scale
- **Headers**: text-lg (18px)
- **Large Values**: text-3xl (30px, font-semibold)
- **Secondary Info**: text-xl (20px, font-bold)
- **Instructions**: text-sm (14px) to text-base (16px)

### Adaptive Behavior
The workflow automatically adjusts based on:
- **Number of outputs**: More outputs = more screens
- **Transaction warnings**: Caution screens only appear when triggered
- **Fee levels**: Different warnings for different fee thresholds
- **User navigation**: Can review previous screens before confirming

---

## Technical Implementation Notes

### State Management
- Each workflow maintains its own section state counter
- Animation direction tracked separately for smooth transitions
- Confirmation progress stored per device

### Gesture Detection
- Mouse/touch events with 30px threshold
- Invisible touch zones for swipe areas
- Event cleanup after gesture completion

### Performance Considerations
- CSS transitions for smooth animations
- Key-based re-rendering for screen changes
- Optimistic state updates for responsive feel

