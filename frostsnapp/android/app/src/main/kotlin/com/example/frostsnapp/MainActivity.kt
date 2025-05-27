package com.example.frostsnapp

import android.content.Context
import android.content.Intent
import android.hardware.usb.UsbDevice
import android.hardware.usb.UsbManager
import android.os.Bundle
import android.util.Log
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel // Import MethodChannel
import androidx.annotation.NonNull // For @NonNull

class MainActivity : FlutterActivity() { // Or your actual base class
    private val usbTAG = "USB_ACTIVITY_TEST" // Tag for USB specific logs
    private val flutterTAG = "FLUTTER_BRIDGE" // Tag for channel logs

    // Define a unique name for your method channel.
    // This string must be EXACTLY THE SAME on the Dart side.
    private val USB_DEVICE_CHANNEL = "com.example.frostsnapp/usb_device_channel"
    private var methodChannel: MethodChannel? = null

    override fun configureFlutterEngine(@NonNull flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        Log.i(flutterTAG, "Configuring Flutter Engine and Method Channel.")
        methodChannel = MethodChannel(flutterEngine.dartExecutor.binaryMessenger, USB_DEVICE_CHANNEL)
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        Log.i(usbTAG, "onCreate() called. Intent action: " + (intent?.action ?: "null intent"))
        handleIntent(intent)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        Log.i(usbTAG, "onNewIntent() called. Intent action: " + (intent.action ?: "null intent"))
        setIntent(intent) // Important: update the activity's intent
        handleIntent(intent)
    }

    private fun handleIntent(intent: Intent?) {
        if (intent != null && UsbManager.ACTION_USB_DEVICE_ATTACHED == intent.action) {
            Log.i(usbTAG, ">>> ACTION_USB_DEVICE_ATTACHED received in handleIntent <<<")
            val device = intent.getParcelableExtra<UsbDevice>(UsbManager.EXTRA_DEVICE) // Modern way to get parcelable
            if (device != null) {
                Log.i(usbTAG, "Device attached: ${device.deviceName} (VID: ${device.vendorId} PID: ${device.productId} OS_DeviceID: ${device.deviceId})")

                val usbManager = getSystemService(Context.USB_SERVICE) as UsbManager
                val hasPermissionInActivity = usbManager.hasPermission(device)
                Log.i(usbTAG, "HAS PERMISSION IN ACTIVITY (for device from intent)? ---> $hasPermissionInActivity")

                // Only proceed if permission is granted or if you intend to let the plugin ask.
                // For this flow, we are assuming the system's "Open..." dialog implies session permission.
                if (hasPermissionInActivity) {
                    // Prepare data to send to Dart
                    val deviceDetails = HashMap<String, Any>()
                    deviceDetails["vid"] = device.vendorId
                    deviceDetails["pid"] = device.productId
                    deviceDetails["deviceId"] = device.deviceId // OS-assigned device ID
                    deviceDetails["deviceName"] = device.deviceName ?: "Unknown USB Device"

                    // Invoke the method on the Dart side
                    methodChannel?.invokeMethod("onUsbDeviceAttached", deviceDetails, object : MethodChannel.Result {
                        override fun success(result: Any?) {
                            Log.i(flutterTAG, "Successfully notified Dart about USB device. Dart returned: $result")
                        }
                        override fun error(errorCode: String, errorMessage: String?, errorDetails: Any?) {
                            Log.e(flutterTAG, "Failed to notify Dart. Error: $errorCode, $errorMessage")
                        }
                        override fun notImplemented() {
                            Log.w(flutterTAG, "Dart side did not implement 'onUsbDeviceAttached'.")
                        }
                    })
                    Log.i(flutterTAG, "Attempted to invoke 'onUsbDeviceAttached' on Dart side.")
                } else {
                    Log.w(usbTAG, "Permission not granted in Activity for attached device. Not notifying Dart to auto-connect via this path.")
                    // If permission is NOT granted here, you might need to decide:
                    // 1. Still notify Dart but with a flag indicating no permission, let Dart decide to call plugin's create (which will ask).
                    // 2. Do nothing from this path, and rely on the plugin's own attach listener + explicit permission request if that's the desired flow.
                    // Given our goal to avoid the double dialog, we'd hope hasPermissionInActivity is true.
                    // If it's false, the original double dialog issue might stem from this very point if we proceed.
                }
            } else {
                Log.w(usbTAG, "Device extra was null for ACTION_USB_DEVICE_ATTACHED.")
            }
        }
    }
}
