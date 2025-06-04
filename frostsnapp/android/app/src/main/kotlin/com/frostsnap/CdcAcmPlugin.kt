package com.frostsnap;

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.hardware.usb.UsbDevice
import android.hardware.usb.UsbManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.annotation.NonNull
import io.flutter.embedding.engine.plugins.FlutterPlugin
import io.flutter.plugin.common.BinaryMessenger
import io.flutter.plugin.common.EventChannel
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel
import io.flutter.plugin.common.MethodChannel.MethodCallHandler
import java.util.concurrent.ConcurrentHashMap
import android.system.Os
import android.os.ParcelFileDescriptor
import android.hardware.usb.*



// Small class to handle listing ports and opening them so they can be passed to rust.
// Also handles listening for detached events.
class CdcAcmPlugin : FlutterPlugin, MethodCallHandler {
    private lateinit var applicationContext: Context
    private lateinit var usbManager: UsbManager
    private lateinit var binaryMessenger: BinaryMessenger
    private lateinit var mainMethodChannel: MethodChannel

    // For system-wide USB attach/detach events
    private var systemEventsChannel: EventChannel? = null
    private var systemEventsSink: EventChannel.EventSink? = null
    private var usbAttachDetachReceiver: BroadcastReceiver? = null
    private val mainThreadHandler = Handler(Looper.getMainLooper())

    companion object {
        private const val TAG = "CdcAcmPlugin"
        // Main command channel for Dart to call Kotlin (list ports, open ports)
        private const val MAIN_CHANNEL_NAME = "com.frostsnap.cdc_acm_plugin/main"
        // EventChannel for system-wide USB attach/detach events. Mostly for detach events.
        private const val SYSTEM_USB_EVENTS_CHANNEL_NAME = "com.frostsnap.cdc_acm_plugin/system_usb_events"
    }

    override fun onAttachedToEngine(@NonNull binding: FlutterPlugin.FlutterPluginBinding) {
        applicationContext = binding.applicationContext
        usbManager = applicationContext.getSystemService(Context.USB_SERVICE) as UsbManager
        binaryMessenger = binding.binaryMessenger
        mainMethodChannel = MethodChannel(binaryMessenger, MAIN_CHANNEL_NAME)
        mainMethodChannel.setMethodCallHandler(this)

        systemEventsChannel = EventChannel(binaryMessenger, SYSTEM_USB_EVENTS_CHANNEL_NAME)
        systemEventsChannel!!.setStreamHandler(object : EventChannel.StreamHandler {
            override fun onListen(arguments: Any?, events: EventChannel.EventSink?) { systemEventsSink = events }
            override fun onCancel(arguments: Any?) { systemEventsSink = null }
        })

        usbAttachDetachReceiver = object : BroadcastReceiver() {
            override fun onReceive(context: Context, intent: Intent) {
                val action = intent.action
                val device = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                    intent.getParcelableExtra(UsbManager.EXTRA_DEVICE, UsbDevice::class.java)
                } else {
                    intent.getParcelableExtra(UsbManager.EXTRA_DEVICE) as UsbDevice?
                }
                device?.let { dev ->
                    val eventPayload = deviceToMap(dev)
                    val eventType = if (UsbManager.ACTION_USB_DEVICE_DETACHED == action) "detached"
                                    else if (UsbManager.ACTION_USB_DEVICE_ATTACHED == action) "attached"
                                    else null

                    eventType?.let { type ->
                        Log.i(TAG, "System USB Event: $type for ${eventPayload["id"]}")
                        mainThreadHandler.post { systemEventsSink?.success(mapOf("event" to type, "device" to eventPayload)) }
                    }
                }
            }
        }
        val filter = IntentFilter().apply {
            addAction(UsbManager.ACTION_USB_DEVICE_DETACHED); addAction(UsbManager.ACTION_USB_DEVICE_ATTACHED)
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            applicationContext.registerReceiver(usbAttachDetachReceiver, filter, Context.RECEIVER_NOT_EXPORTED)
        } else {
            applicationContext.registerReceiver(usbAttachDetachReceiver, filter)
        }
        Log.i(TAG, "CdcAcmPlugin attached. Main channel: $MAIN_CHANNEL_NAME")
    }

    override fun onDetachedFromEngine(@NonNull binding: FlutterPlugin.FlutterPluginBinding) {
        mainMethodChannel.setMethodCallHandler(null)
        systemEventsChannel?.setStreamHandler(null); systemEventsSink = null
        usbAttachDetachReceiver?.let {
            try { applicationContext.unregisterReceiver(it) }
            catch (e: IllegalArgumentException) { Log.w(TAG, "Detach receiver already unregistered or never registered.") }
        }
        usbAttachDetachReceiver = null

        Log.d(TAG, "CdcAcmPlugin (FD Mode) detached.")
    }

    private fun openAndDupFd(device: UsbDevice): Int? {
        val connection = usbManager.openDevice(device) ?: return null

        val origFd = connection.fileDescriptor
        if (origFd < 0) {
            connection.close(); return null
        }

        // Duplicate the fd so Rust owns its own copy
        val dupFd = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            ParcelFileDescriptor.dup(
                ParcelFileDescriptor.fromFd(origFd).fileDescriptor
            ).detachFd()
        } else {
            val pfd = ParcelFileDescriptor.fromFd(origFd)
            val dupFile = Os.dup(pfd.fileDescriptor) // API-21 +
            val dupFdInt = ParcelFileDescriptor.dup(dupFile).detachFd()
            pfd.close()
            dupFdInt
        }

        connection.close()     // This only closes the original fd
        return dupFd
    }


    override fun onMethodCall(@NonNull call: MethodCall, @NonNull result: MethodChannel.Result) {
        Log.d(TAG, "onMethodCall (FD Mode): ${call.method} args: ${call.arguments}")
        when (call.method) {
            "listDevices" -> {
                val deviceList = usbManager.deviceList.values.map(::deviceToMap)
                result.success(deviceList)
            }
            "openDeviceAndGetFd" -> {
                try {
                    val id = call.argument<String>("id")

                    val targetDevice = usbManager.deviceList.values.firstOrNull {
                        deviceId(it) == id
                    }

                    if (targetDevice == null) {
                        result.error("DEVICE_NOT_FOUND", "USB $id not found.", null); return
                    }

                    if (usbManager.hasPermission(targetDevice)) {
                        val fd = openAndDupFd(targetDevice)
                        Log.e(TAG, "successfully opened $id", null)
                        result.success(mapOf("fd" to fd))
                    } else {
                        result.error(TAG, "cannot open $id -- no permission", null)
                    }
                } catch (e: Exception) {
                    Log.e(TAG, "Exception in openDeviceAndGetFd: ${e.message}", e)
                    result.error("OPEN_EXCEPTION", e.message, e.stackTraceToString())
                }
            }
            else -> result.notImplemented()
        }
    }


}

fun deviceId(dev: UsbDevice) : String {
    return dev.deviceName ?: "usb_fd_conn_${dev.deviceId}"
}

fun deviceToMap(dev: UsbDevice) : Map<String, Any> {
    val id = deviceId(dev)
    return mapOf(
        "id" to id,
        "vid" to dev.vendorId,
        "pid" to dev.productId,
    )
}
