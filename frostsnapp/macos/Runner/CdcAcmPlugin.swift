import Foundation
import FlutterMacOS
import IOKit
import IOKit.usb
import IOKit.serial

class CdcAcmPlugin: NSObject, FlutterPlugin {
    private static let TAG = "CdcAcmPlugin"
    private static let MAIN_CHANNEL_NAME = "com.frostsnap.cdc_acm_plugin/main"
    private static let SYSTEM_USB_EVENTS_CHANNEL_NAME = "com.frostsnap.cdc_acm_plugin/system_usb_events"
    
    private var mainMethodChannel: FlutterMethodChannel?
    private var systemEventsChannel: FlutterEventChannel?
    private var eventSink: FlutterEventSink?
    private var notificationPort: IONotificationPortRef?
    private var runLoopSource: CFRunLoopSource?
    private var deviceAddedIterator: io_iterator_t = 0
    private var deviceRemovedIterator: io_iterator_t = 0
    
    static func register(with registrar: FlutterPluginRegistrar) {
        let instance = CdcAcmPlugin()
        let mainChannel = FlutterMethodChannel(
            name: MAIN_CHANNEL_NAME,
            binaryMessenger: registrar.messenger
        )
        registrar.addMethodCallDelegate(instance, channel: mainChannel)
        instance.mainMethodChannel = mainChannel
        
        let eventsChannel = FlutterEventChannel(
            name: SYSTEM_USB_EVENTS_CHANNEL_NAME,
            binaryMessenger: registrar.messenger
        )
        eventsChannel.setStreamHandler(instance)
        instance.systemEventsChannel = eventsChannel
        
        instance.setupUSBNotifications()
        
        print("\(TAG): Plugin registered")
    }
    
    func handle(_ call: FlutterMethodCall, result: @escaping FlutterResult) {
        print("\(CdcAcmPlugin.TAG): Method call: \(call.method) args: \(String(describing: call.arguments))")
        
        switch call.method {
        case "listDevices":
            let devices = listCDCACMDevices()
            result(devices)
            
        case "openDeviceAndGetFd":
            guard let args = call.arguments as? [String: Any],
                  let id = args["id"] as? String else {
                result(FlutterError(code: "INVALID_ARGS", message: "Missing device id", details: nil))
                return
            }
            
            if let fd = openDevice(withPath: id) {
                result(["fd": fd])
            } else {
                result(FlutterError(code: "OPEN_FAILED", message: "Failed to open device \(id)", details: nil))
            }
            
        default:
            result(FlutterMethodNotImplemented)
        }
    }
    
    private func listCDCACMDevices() -> [[String: Any]] {
        var devices: [[String: Any]] = []
        
        // Create matching dictionary for USB devices
        let matchingDict = IOServiceMatching(kIOUSBDeviceClassName)
        var iterator: io_iterator_t = 0
        
        let result = IOServiceGetMatchingServices(kIOMasterPortDefault, matchingDict, &iterator)
        guard result == KERN_SUCCESS else {
            print("\(CdcAcmPlugin.TAG): Failed to get USB devices")
            return devices
        }
        
        defer { IOObjectRelease(iterator) }
        
        var device: io_object_t = IOIteratorNext(iterator)
        while device != 0 {
            defer { 
                IOObjectRelease(device)
                device = IOIteratorNext(iterator)
            }
            
            // Check if this is a CDC ACM device
            if let deviceInfo = getCDCACMInfo(device: device) {
                devices.append(deviceInfo)
            }
        }
        
        return devices
    }
    
    private func getCDCACMInfo(device: io_object_t) -> [String: Any]? {
        // Get device properties
        var properties: Unmanaged<CFMutableDictionary>?
        let result = IORegistryEntryCreateCFProperties(device, &properties, kCFAllocatorDefault, 0)
        guard result == KERN_SUCCESS, let props = properties?.takeRetainedValue() as? [String: Any] else {
            return nil
        }
        
        // Check for CDC ACM class (class 2, subclass 2)
        guard let deviceClass = props[kUSBDeviceClass] as? Int,
              deviceClass == 2 else {
            // Also check interface level
            return checkInterfacesForCDCACM(device: device, props: props)
        }
        
        // Get VID/PID
        guard let vendorID = props[kUSBVendorID] as? Int,
              let productID = props[kUSBProductID] as? Int else {
            return nil
        }
        
        // Try to find the BSD name (serial port path)
        if let bsdName = findBSDName(device: device) {
            return [
                "id": bsdName,
                "vid": vendorID,
                "pid": productID
            ]
        }
        
        return nil
    }
    
    private func checkInterfacesForCDCACM(device: io_object_t, props: [String: Any]) -> [String: Any]? {
        // Sometimes CDC ACM is defined at interface level instead of device level
        // We need to iterate through the device's interfaces
        var childIterator: io_iterator_t = 0
        let result = IORegistryEntryGetChildIterator(device, kIOServicePlane, &childIterator)
        guard result == KERN_SUCCESS else { return nil }
        defer { IOObjectRelease(childIterator) }
        
        var child = IOIteratorNext(childIterator)
        while child != 0 {
            defer {
                IOObjectRelease(child)
                child = IOIteratorNext(childIterator)
            }
            
            var childProps: Unmanaged<CFMutableDictionary>?
            let propResult = IORegistryEntryCreateCFProperties(child, &childProps, kCFAllocatorDefault, 0)
            guard propResult == KERN_SUCCESS,
                  let cProps = childProps?.takeRetainedValue() as? [String: Any] else {
                continue
            }
            
            // Check if this interface is CDC ACM
            if let interfaceClass = cProps[kUSBInterfaceClass] as? Int,
               let interfaceSubClass = cProps[kUSBInterfaceSubClass] as? Int,
               interfaceClass == 2 && interfaceSubClass == 2 {
                
                // Found CDC ACM interface, get device info
                guard let vendorID = props[kUSBVendorID] as? Int,
                      let productID = props[kUSBProductID] as? Int else {
                    continue
                }
                
                if let bsdName = findBSDName(device: device) {
                    return [
                        "id": bsdName,
                        "vid": vendorID,
                        "pid": productID
                    ]
                }
            }
        }
        
        return nil
    }
    
    private func findBSDName(device: io_object_t) -> String? {
        // Traverse the IORegistry to find the BSD name
        var iterator: io_iterator_t = 0
        let result = IORegistryEntryGetChildIterator(device, kIOServicePlane, &iterator)
        guard result == KERN_SUCCESS else { return nil }
        defer { IOObjectRelease(iterator) }
        
        return findBSDNameRecursive(iterator: iterator)
    }
    
    private func findBSDNameRecursive(iterator: io_iterator_t) -> String? {
        var child = IOIteratorNext(iterator)
        while child != 0 {
            defer {
                IOObjectRelease(child)
                child = IOIteratorNext(iterator)
            }
            
            // Check if this object has IODialinDevice property
            if let bsdPath = IORegistryEntryCreateCFProperty(
                child,
                kIODialinDeviceKey as CFString,
                kCFAllocatorDefault,
                0
            )?.takeRetainedValue() as? String {
                return bsdPath
            }
            
            // Also check IOCalloutDevice
            if let bsdPath = IORegistryEntryCreateCFProperty(
                child,
                kIOCalloutDeviceKey as CFString,
                kCFAllocatorDefault,
                0
            )?.takeRetainedValue() as? String {
                // Convert cu.* to tty.* if needed
                return bsdPath.replacingOccurrences(of: "/dev/cu.", with: "/dev/tty.")
            }
            
            // Recursively check children
            var childIterator: io_iterator_t = 0
            if IORegistryEntryGetChildIterator(child, kIOServicePlane, &childIterator) == KERN_SUCCESS {
                defer { IOObjectRelease(childIterator) }
                if let found = findBSDNameRecursive(iterator: childIterator) {
                    return found
                }
            }
        }
        
        return nil
    }
    
    private func openDevice(withPath path: String) -> Int32? {
        // Open the device file and get file descriptor
        let fd = open(path, O_RDWR | O_NOCTTY | O_NONBLOCK)
        if fd < 0 {
            print("\(CdcAcmPlugin.TAG): Failed to open \(path): \(String(cString: strerror(errno)))")
            return nil
        }
        
        // Configure serial port settings if needed
        var options = termios()
        if tcgetattr(fd, &options) == 0 {
            // Set raw mode
            cfmakeraw(&options)
            
            // Set baud rate (example: 115200)
            cfsetispeed(&options, speed_t(B115200))
            cfsetospeed(&options, speed_t(B115200))
            
            // Apply settings
            tcsetattr(fd, TCSANOW, &options)
        }
        
        print("\(CdcAcmPlugin.TAG): Successfully opened \(path) with fd: \(fd)")
        return fd
    }
    
    private func setupUSBNotifications() {
        notificationPort = IONotificationPortCreate(kIOMasterPortDefault)
        guard let notificationPort = notificationPort else {
            print("\(CdcAcmPlugin.TAG): Failed to create notification port")
            return
        }
        
        runLoopSource = IONotificationPortGetRunLoopSource(notificationPort)
        CFRunLoopAddSource(CFRunLoopGetCurrent(), runLoopSource, .defaultMode)
        
        // Set up notifications for USB devices
        let matchingDict = IOServiceMatching(kIOUSBDeviceClassName)
        
        // Device added notification
        let addedCallback: IOServiceMatchingCallback = { (refcon, iterator) in
            let this = Unmanaged<CdcAcmPlugin>.fromOpaque(refcon!).takeUnretainedValue()
            this.handleDeviceAdded(iterator: iterator)
        }
        
        // Device removed notification
        let removedCallback: IOServiceMatchingCallback = { (refcon, iterator) in
            let this = Unmanaged<CdcAcmPlugin>.fromOpaque(refcon!).takeUnretainedValue()
            this.handleDeviceRemoved(iterator: iterator)
        }
        
        let selfPtr = Unmanaged.passUnretained(self).toOpaque()
        
        IOServiceAddMatchingNotification(
            notificationPort,
            kIOFirstMatchNotification,
            matchingDict,
            addedCallback,
            selfPtr,
            &deviceAddedIterator
        )
        
        // Need to create another matching dict for removal
        let matchingDict2 = IOServiceMatching(kIOUSBDeviceClassName)
        IOServiceAddMatchingNotification(
            notificationPort,
            kIOTerminatedNotification,
            matchingDict2,
            removedCallback,
            selfPtr,
            &deviceRemovedIterator
        )
        
        // Process initial iterators
        handleDeviceAdded(iterator: deviceAddedIterator)
        handleDeviceRemoved(iterator: deviceRemovedIterator)
    }
    
    private func handleDeviceAdded(iterator: io_iterator_t) {
        var device = IOIteratorNext(iterator)
        while device != 0 {
            defer {
                IOObjectRelease(device)
                device = IOIteratorNext(iterator)
            }
            
            if let deviceInfo = getCDCACMInfo(device: device) {
                print("\(CdcAcmPlugin.TAG): USB device attached: \(deviceInfo["id"] ?? "unknown")")
                eventSink?([
                    "event": "attached",
                    "device": deviceInfo
                ])
            }
        }
    }
    
    private func handleDeviceRemoved(iterator: io_iterator_t) {
        var device = IOIteratorNext(iterator)
        while device != 0 {
            defer {
                IOObjectRelease(device)
                device = IOIteratorNext(iterator)
            }
            
            // For removal, we might not be able to get full info
            // Just notify that a device was removed
            print("\(CdcAcmPlugin.TAG): USB device detached")
            eventSink?([
                "event": "detached",
                "device": [:]
            ])
        }
    }
    
    deinit {
        if let runLoopSource = runLoopSource {
            CFRunLoopRemoveSource(CFRunLoopGetCurrent(), runLoopSource, .defaultMode)
        }
        
        if deviceAddedIterator != 0 {
            IOObjectRelease(deviceAddedIterator)
        }
        
        if deviceRemovedIterator != 0 {
            IOObjectRelease(deviceRemovedIterator)
        }
        
        if let notificationPort = notificationPort {
            IONotificationPortDestroy(notificationPort)
        }
    }
}

// MARK: - FlutterStreamHandler
extension CdcAcmPlugin: FlutterStreamHandler {
    func onListen(withArguments arguments: Any?, eventSink events: @escaping FlutterEventSink) -> FlutterError? {
        self.eventSink = events
        return nil
    }
    
    func onCancel(withArguments arguments: Any?) -> FlutterError? {
        self.eventSink = nil
        return nil
    }
}