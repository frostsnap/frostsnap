package com.frostsnap

import android.content.Context
import android.content.Intent
import android.hardware.usb.UsbDevice
import android.hardware.usb.UsbManager
import android.os.Bundle
import android.util.Log
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import androidx.annotation.NonNull

import com.frostsnap.CdcAcmPlugin.*;


class MainActivity : FlutterActivity() {
    // The code here mostly handles being notified of the user approving permissions for the the app
    // to access the USB device therough the intent described by `device_filter.xml`
    private val TAG = "USB_PERMISSIONS"

    private val USB_PERMISSIONS_CHANNEL_TO_DART = "com.frostsnap/usb_permissions_channel"
    private var mainActivityToDartMethodChannel: MethodChannel? = null

    override fun configureFlutterEngine(@NonNull flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        // 1. Setup channel for MainActivity to send device attached events TO Dart
        mainActivityToDartMethodChannel = MethodChannel(flutterEngine.dartExecutor.binaryMessenger, USB_PERMISSIONS_CHANNEL_TO_DART)

        try {
            flutterEngine.plugins.add(CdcAcmPlugin())
            Log.i("configureFlutterEngine", "CdcAcmPlugin successfully registered with FlutterEngine.")
        } catch (e: Exception) {
            Log.e("configureFlutterEngine", "Error REGISTERING CdcAcmPlugin: ${e.message}", e)
        }
        
        try {
            flutterEngine.plugins.add(SecureKeyManager())
            Log.i("configureFlutterEngine", "SecureKeyManager successfully registered with FlutterEngine.")
        } catch (e: Exception) {
            Log.e("configureFlutterEngine", "Error REGISTERING SecureKeyManager: ${e.message}", e)
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        Log.i(TAG, "onCreate() called. Intent action: " + (intent?.action ?: "null intent"))
        handleIntent(getIntent())
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        Log.i(TAG, "onNewIntent() called. Intent action: " + (intent.action ?: "null intent"))
        setIntent(intent)
        handleIntent(intent)
    }

    private fun handleIntent(intent: Intent?) {
        if (intent != null && UsbManager.ACTION_USB_DEVICE_ATTACHED == intent.action) {
            val device = intent.getParcelableExtra<UsbDevice>(UsbManager.EXTRA_DEVICE)
            if (device != null) {
                Log.i(TAG, "Device attached: ${device.deviceName} (VID: ${device.vendorId} PID: ${device.productId} OS_DeviceID: ${device.deviceId})")
                val usbManager = getSystemService(Context.USB_SERVICE) as UsbManager
                val hasPermissionInActivity = usbManager.hasPermission(device)
                Log.i(TAG, "HAS PERMISSION IN ACTIVITY (for device from intent)? ---> $hasPermissionInActivity")

                if (hasPermissionInActivity) {
                    val deviceDetails = HashMap<String, Any>()
                    deviceDetails["vid"] = device.vendorId
                    deviceDetails["pid"] = device.productId
                    deviceDetails["id"] = deviceId(device)

                    mainActivityToDartMethodChannel?.invokeMethod("onUsbDeviceAttached", deviceDetails, object : MethodChannel.Result {
                        override fun success(result: Any?) { Log.i(TAG, "Successfully notified Dart (MainActivity channel). Dart returned: $result") }
                        override fun error(errorCode: String, errorMessage: String?, errorDetails: Any?) { Log.e(TAG, "Failed to notify Dart (MainActivity channel). Error: $errorCode, $errorMessage") }
                        override fun notImplemented() { Log.w(TAG, "Dart side did not implement 'onUsbDeviceAttached' (MainActivity channel).") }
                    })
                    Log.i(TAG, "Attempted to invoke 'onUsbDeviceAttached' on Dart side (MainActivity channel).")
                } else {
                    Log.w(TAG, "Permission not granted in Activity for attached device. Not notifying Dart via MainActivity channel.")
                }
            } else {
                Log.w(TAG, "Device extra was null for ACTION_USB_DEVICE_ATTACHED.")
            }
        }
    }
}
