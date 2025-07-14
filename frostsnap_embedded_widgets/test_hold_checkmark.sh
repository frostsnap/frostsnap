#!/bin/bash

# Test script for hold to confirm with checkmark widget

# Start the simulator in the background
echo "Starting hold_checkmark demo..."
cargo run --bin simulate hold_checkmark 2>/dev/null &
PID=$!

# Wait for it to start
sleep 3

# Touch down in the center of the circle (screen is 240x280, widget is 100x100 centered)
echo "Touching down on fingerprint icon..."
echo "touch 120,140" | cargo run --bin simulate hold_checkmark 2>&1 >/dev/null &
sleep 0.5

# Take first screenshot
echo "screenshot hold_start.png" | nc localhost 8080 2>/dev/null || echo "screenshot hold_start.png"
sleep 0.5

# Wait for hold progress (hold for 2 seconds)
echo "Holding for 2 seconds..."
echo "wait 2000" | nc localhost 8080 2>/dev/null || echo "wait 2000"

# Take screenshot after hold complete
echo "screenshot hold_complete.png" | nc localhost 8080 2>/dev/null || echo "screenshot hold_complete.png"
sleep 0.5

# Release
echo "Releasing touch..."
echo "release 120,140" | nc localhost 8080 2>/dev/null || echo "release 120,140"
sleep 0.1

# Take screenshot during checkmark animation
echo "wait 500" | nc localhost 8080 2>/dev/null || echo "wait 500"
echo "screenshot checkmark_anim1.png" | nc localhost 8080 2>/dev/null || echo "screenshot checkmark_anim1.png"

echo "wait 500" | nc localhost 8080 2>/dev/null || echo "wait 500"
echo "screenshot checkmark_anim2.png" | nc localhost 8080 2>/dev/null || echo "screenshot checkmark_anim2.png"

echo "wait 500" | nc localhost 8080 2>/dev/null || echo "wait 500"
echo "screenshot checkmark_final.png" | nc localhost 8080 2>/dev/null || echo "screenshot checkmark_final.png"

# Clean up
echo "quit" | nc localhost 8080 2>/dev/null || echo "quit"
kill $PID 2>/dev/null

echo "Test complete! Check the following screenshots:"
echo "  - hold_start.png: Initial touch with fingerprint icon"
echo "  - hold_complete.png: After 2 second hold"
echo "  - checkmark_anim1.png: Checkmark animation phase 1"
echo "  - checkmark_anim2.png: Checkmark animation phase 2"
echo "  - checkmark_final.png: Final checkmark state"